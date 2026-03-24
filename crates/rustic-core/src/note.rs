use serde::{Deserialize, Serialize};

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
