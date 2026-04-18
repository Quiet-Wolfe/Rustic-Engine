use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// All note types supported by Psych Engine.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NoteKind {
    Normal,
    /// "Alt Animation" - uses the "-alt" animation variant
    Alt,
    /// "Hey!" - forces "hey" animation
    Hey,
    /// "Hurt Note" - damages player if hit
    Hurt,
    /// "GF Sing" - girlfriend sings instead of current character
    GfSing,
    /// "No Animation" - note hit but no character animation plays
    NoAnim,
    /// Custom note type (name matches a script file)
    Custom(String),
}

impl NoteKind {
    /// Return the note type as a string (for Lua callbacks).
    pub fn as_type_str(&self) -> &str {
        match self {
            Self::Normal => "",
            Self::Alt => "Alt Animation",
            Self::Hey => "Hey!",
            Self::Hurt => "Hurt Note",
            Self::GfSing => "GF Sing",
            Self::NoAnim => "No Animation",
            Self::Custom(s) => s,
        }
    }

    /// Whether this note type damages the player when hit (like Hurt Note).
    /// Checks built-in types first, then falls back to the runtime registry.
    pub fn is_harmful(&self) -> bool {
        match self {
            Self::Hurt => true,
            Self::Custom(name) => is_note_type_harmful(name),
            _ => false,
        }
    }

    /// Whether missing this note should NOT penalize the player.
    /// Hurt notes are meant to be dodged, so missing them is fine.
    pub fn should_ignore_miss(&self) -> bool {
        match self {
            Self::Hurt => true,
            Self::Custom(name) => get_note_type_config(name).map_or(false, |c| c.ignore_miss),
            _ => false,
        }
    }

    /// Health damage when this note is hit (only relevant for harmful notes).
    /// Psych Engine default Hurt Note damage is 0.276 on the 0–2 health scale.
    pub fn hit_damage(&self) -> f32 {
        match self {
            Self::Hurt => 0.276,
            Self::Custom(name) => get_note_type_config(name).map_or(0.0, |c| c.hit_damage),
            _ => 0.0,
        }
    }

    /// Get the full NoteTypeConfig for custom types (returns None for built-in types
    /// that aren't separately registered).
    pub fn custom_config(&self) -> Option<NoteTypeConfig> {
        match self {
            Self::Custom(name) => get_note_type_config(name),
            _ => None,
        }
    }

    /// Parse from the optional 4th element of sectionNotes.
    pub fn from_chart_value(value: Option<&serde_json::Value>) -> Self {
        match value {
            None => Self::Normal,
            Some(v) => {
                if let Some(s) = v.as_str() {
                    match s {
                        "" => Self::Normal,
                        "Alt Animation" => Self::Alt,
                        "Hey!" => Self::Hey,
                        "Hurt Note" => Self::Hurt,
                        "GF Sing" => Self::GfSing,
                        "No Animation" => Self::NoAnim,
                        other => Self::Custom(other.to_string()),
                    }
                } else if let Some(n) = v.as_i64() {
                    match n {
                        0 => Self::Normal,
                        _ => Self::Custom(n.to_string()),
                    }
                } else {
                    Self::Normal
                }
            }
        }
    }
}

/// A note parsed from a chart, with all gameplay-relevant fields.
#[derive(Debug, Clone)]
pub struct NoteData {
    /// Strum time in milliseconds.
    pub strum_time: f64,
    /// Lane index (0-3: Left, Down, Up, Right).
    pub lane: usize,
    /// Sustain/hold length in milliseconds. 0 = tap note.
    pub sustain_length: f64,
    /// Whether this note must be hit by the player.
    pub must_press: bool,
    /// Note type.
    pub kind: NoteKind,

    // === Runtime state (not from chart) ===
    pub was_good_hit: bool,
    pub too_late: bool,
    pub hold_released: bool,
    pub hold_progress: f64,
    pub rating: Option<String>,
    pub rating_mod: f64,
    pub rating_disabled: bool,
    pub gf_note: bool,
    pub alt_note: bool,

    // === Visual overrides (set by Lua modcharts) ===
    pub visible: bool,
    pub alpha: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub angle: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub flip_y: bool,
    pub correction_offset: f32,
    pub is_reversing_scroll: bool,
    pub color_r_offset: f32,
    pub color_g_offset: f32,
    pub color_b_offset: f32,
}

impl NoteData {
    pub fn new(
        strum_time: f64,
        lane: usize,
        sustain_length: f64,
        must_press: bool,
        kind: NoteKind,
    ) -> Self {
        Self {
            strum_time,
            lane,
            sustain_length,
            must_press,
            kind,
            was_good_hit: false,
            too_late: false,
            hold_released: false,
            hold_progress: 0.0,
            rating: None,
            rating_mod: 0.0,
            rating_disabled: false,
            gf_note: false,
            alt_note: false,
            visible: true,
            alpha: 1.0,
            scale_x: 0.7,
            scale_y: 0.7,
            angle: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            flip_y: false,
            correction_offset: 0.0,
            is_reversing_scroll: false,
            color_r_offset: 0.0,
            color_g_offset: 0.0,
            color_b_offset: 0.0,
        }
    }

    pub fn is_sustain(&self) -> bool {
        self.sustain_length > 0.0
    }

    pub fn is_active(&self) -> bool {
        !self.was_good_hit && !self.too_late
    }
}

/// Configuration for a custom note type — gameplay behavior and asset overrides.
/// Note types are registered at runtime (typically from Lua scripts) via `register_note_type`.
#[derive(Debug, Clone)]
pub struct NoteTypeConfig {
    /// If true, hitting this note damages the player instead of scoring.
    pub hit_causes_miss: bool,
    /// Health penalty when hit (0.0–1.0, as fraction of max health).
    pub hit_damage: f32,
    /// If true, letting this note pass (missing) does NOT penalize the player.
    pub ignore_miss: bool,
    /// Custom note skin atlas path (relative to images dir), e.g. "notes/Scythe_Note_Assets".
    pub note_skin: Option<String>,
    /// Custom note animation names for the 4 directions [Left, Down, Up, Right].
    pub note_anims: Option<[String; 4]>,
    /// Custom strum static animation names for the 4 directions.
    pub strum_anims: Option<[String; 4]>,
    /// Custom strum confirm (hit) animation names for the 4 directions.
    pub confirm_anims: Option<[String; 4]>,
    /// SFX to play when this note type is hit (path relative to sounds dir, without extension).
    pub hit_sfx: Option<String>,
    /// If > 0.0, health drains by this fraction over time instead of instantly.
    /// E.g. 0.5 means health slides down by 50% of max (1.0) over ~0.5s.
    pub health_drain_pct: f32,
    /// If true, the health drain cannot kill — it stops just above the death threshold
    /// on the first hit. A second hit while already near death IS lethal.
    pub drain_death_safe: bool,
}

impl Default for NoteTypeConfig {
    fn default() -> Self {
        Self {
            hit_causes_miss: false,
            hit_damage: 0.0,
            ignore_miss: false,
            note_skin: None,
            note_anims: None,
            strum_anims: None,
            confirm_anims: None,
            hit_sfx: None,
            health_drain_pct: 0.0,
            drain_death_safe: false,
        }
    }
}

use std::sync::RwLock;

static NOTE_TYPE_REGISTRY: RwLock<Option<HashMap<String, NoteTypeConfig>>> = RwLock::new(None);

fn ensure_registry() {
    let mut reg = NOTE_TYPE_REGISTRY.write().unwrap();
    if reg.is_none() {
        *reg = Some(HashMap::new());
    }
}

/// Register a note type config. Called from Lua scripts or engine init.
pub fn register_note_type(name: &str, config: NoteTypeConfig) {
    ensure_registry();
    let mut reg = NOTE_TYPE_REGISTRY.write().unwrap();
    if let Some(map) = reg.as_mut() {
        map.insert(name.to_string(), config);
    }
}

/// Look up config for a registered note type name. Returns None for unknown types.
pub fn get_note_type_config(name: &str) -> Option<NoteTypeConfig> {
    let reg = NOTE_TYPE_REGISTRY.read().unwrap();
    reg.as_ref().and_then(|map| map.get(name).cloned())
}

/// Convenience: check if a note type name is a registered harmful type.
pub fn is_note_type_harmful(name: &str) -> bool {
    get_note_type_config(name).map_or(false, |c| c.hit_causes_miss)
}

/// An event note parsed from the chart events array.
#[derive(Debug, Clone)]
pub struct EventNote {
    pub strum_time: f64,
    pub name: String,
    pub value1: String,
    pub value2: String,
    pub fired: bool,
}

impl EventNote {
    pub fn new(strum_time: f64, name: String, value1: String, value2: String) -> Self {
        Self {
            strum_time,
            name,
            value1,
            value2,
            fired: false,
        }
    }
}
