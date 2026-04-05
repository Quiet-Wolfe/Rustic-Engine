use std::collections::HashMap;
use std::path::PathBuf;

use crate::tweens::TweenManager;

/// Shared mutable state accessible from Lua scripts.
/// This is the bridge between Lua and the Rust game engine.
pub struct ScriptState {
    // Built-in variables
    pub song_name: String,
    pub is_story_mode: bool,
    pub screen_width: f32,
    pub screen_height: f32,
    pub cur_beat: i32,
    pub cur_step: i32,
    pub cur_section: i32,

    /// Asset search roots (for resolving image paths in Lua functions).
    pub image_roots: Vec<PathBuf>,

    /// Lua-created sprites (tag → sprite data).
    pub lua_sprites: HashMap<String, LuaSprite>,

    /// Sprites added to the scene. (tag, in_front) — in_front means drawn after characters.
    pub sprites_to_add: Vec<(String, bool)>,

    /// Property writes from Lua: (property_path, value).
    /// Consumed by the game engine each frame.
    pub property_writes: Vec<(String, LuaValue)>,

    /// Property reads requested by Lua.
    /// The game engine populates these before calling Lua callbacks.
    pub property_values: HashMap<String, LuaValue>,

    /// Tween and timer manager.
    pub tweens: TweenManager,

    /// Strum note properties: [opponent0..3, player0..3] = 8 strums.
    /// Modcharts manipulate these via setPropertyFromGroup/noteTween*.
    pub strum_props: [StrumProps; 8],

    /// Sprites pending removal (tag list).
    pub sprites_to_remove: Vec<String>,

    /// Current song position in milliseconds (updated each frame by the game).
    pub song_position: f64,

    /// Current camera zoom (updated each frame by the game, used for tween start values).
    pub camera_zoom: f32,

    /// Default camera zoom (target zoom, synced across all scripts).
    pub default_cam_zoom: f32,

    /// Camera speed multiplier (synced across all scripts).
    pub camera_speed: f32,

    /// Current health value (0..2, synced from game).
    pub health: f32,

    /// Pending camera target changes: "dad"/"bf"/"gf".
    pub camera_target_requests: Vec<String>,

    /// Pending triggered events: (name, v1, v2).
    pub triggered_events: Vec<(String, String, String)>,

    /// Shared custom variables set by scripts (cross-script communication).
    /// When any script calls setProperty('foo', 123), it's stored here so
    /// other scripts can read it back via getProperty('foo').
    pub custom_vars: HashMap<String, LuaValue>,

    /// Per-note visual overrides set by Lua modcharts.
    /// Key is note index, value maps field name to override value.
    pub note_overrides: HashMap<usize, HashMap<String, f64>>,

    /// Total note count, so Lua can query unspawnNotes.length.
    pub note_count: usize,

    /// Basic note data for Lua reads: (strum_time, lane, must_press, sustain_length).
    pub note_read_data: Vec<(f64, usize, bool, f64)>,

    /// Pending moveCameraSection requests: section indices to look up in chart data.
    pub camera_section_requests: Vec<i32>,

    /// Lua-created text objects (tag → text data).
    pub lua_texts: HashMap<String, LuaText>,
    /// Text objects pending addition to the scene. (tag, in_front)
    pub texts_to_add: Vec<(String, bool)>,

    /// Camera shake requests: (camera_name, intensity, duration_seconds).
    pub camera_shake_requests: Vec<(String, f32, f32)>,
    /// Camera flash requests: (camera_name, color_hex, duration, alpha).
    pub camera_flash_requests: Vec<(String, String, f32, f32)>,
    /// Subtitle display requests: (text, font, color, size, duration, border_color).
    pub subtitle_requests: Vec<(String, String, String, f32, f32, String)>,

    /// Whether camera is locked to its current target position (isCameraOnForcedPos).
    pub camera_forced_pos: bool,
    /// Camera offset when pointing at opponent: (x, y).
    pub opponent_camera_offset: (f32, f32),
    /// Camera offset when pointing at player: (x, y).
    pub bf_camera_offset: (f32, f32),

    /// Character names for runHaxeCode switch resolution.
    pub bf_name: String,
    pub dad_name: String,
    pub gf_name: String,

    /// Current animation names for characters (synced from game each frame).
    pub dad_anim_name: String,
    pub bf_anim_name: String,
    pub gf_anim_name: String,
    /// Current character positions (synced from game each frame).
    pub dad_pos: (f32, f32),
    pub bf_pos: (f32, f32),
    pub gf_pos: (f32, f32),

    /// Pending character position adjustments from runHaxeCode.
    /// Each entry: (character: "boyfriend"/"dad"/"gf", field: "x"/"y", value: f64).
    /// NaN value means the next entry is an absolute set; otherwise it's a delta (+=/-=).
    pub char_position_adjustments: Vec<(String, String, f64)>,

    /// Stage overlay color requests: (side: "left"/"right"/"both", r, g, b, a, duration).
    /// Consumed by the game engine each frame.
    pub stage_color_requests: Vec<(String, f32, f32, f32, f32, f32)>,

    /// Stage overlay swap request: (duration,). Swaps left/right colors.
    pub stage_color_swap_requests: Vec<f32>,

    /// Post-processing toggle requests: (enabled, tween_duration).
    pub postprocess_requests: Vec<(bool, f32)>,
    /// Post-processing parameter requests: (param_name, value).
    pub postprocess_param_requests: Vec<(String, f32)>,

    /// Custom health bar color requests: (side: "left"/"right", r, g, b, a, duration).
    pub healthbar_color_requests: Vec<(String, f32, f32, f32, f32, f32)>,

    /// Stage lights toggle: Some(true/false) when set, consumed each frame.
    pub stage_lights_request: Option<bool>,

    /// Reflections toggle: Some(true/false) when set, consumed each frame.
    pub reflections_request: Option<bool>,

    /// Pending note type registrations from Lua (registerNoteType calls).
    /// Each tuple: (name, hit_causes_miss, hit_damage, ignore_miss, note_skin, hit_sfx, drain_pct, death_safe)
    pub note_type_registrations: Vec<(String, bool, f32, bool, Option<String>, Option<String>, f32, bool)>,

    /// Pending video playback requests: (video_path, on_finish_callback_name).
    /// Consumed by the game engine each frame.
    pub video_requests: Vec<(String, Option<String>)>,
}

/// Per-strum-note visual properties (modchart overrides).
#[derive(Debug, Clone, Copy)]
pub struct StrumProps {
    pub x: f32,
    pub y: f32,
    pub alpha: f32,
    pub angle: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub down_scroll: Option<bool>,
    /// If true, these are custom values; if false, use defaults.
    pub custom: bool,
}

/// Definition of a named animation on a Lua sprite.
#[derive(Debug, Clone)]
pub struct LuaAnimDef {
    pub prefix: String,
    pub fps: f32,
    pub looping: bool,
    pub indices: Vec<i32>,
}

/// A sprite created by Lua via makeLuaSprite or makeAnimatedLuaSprite.
pub struct LuaSprite {
    pub tag: String,
    pub kind: LuaSpriteKind,
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub alpha: f32,
    pub visible: bool,
    pub angle: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub antialiasing: bool,
    pub color: [u8; 3],
    /// Object draw order (lower = drawn first).
    pub order: Option<i32>,
    /// Current animation name (for animated sprites).
    pub current_anim: String,
    /// Actual texture dimensions (set after GPU load).
    pub tex_w: f32,
    pub tex_h: f32,
    /// Animation definitions registered via addAnimationByPrefix etc.
    pub animations: HashMap<String, LuaAnimDef>,
    /// Per-animation offsets registered via addOffset.
    pub anim_offsets: HashMap<String, (f32, f32)>,
    /// Current animation frame index.
    pub anim_frame: usize,
    /// Animation timer (seconds accumulated).
    pub anim_timer: f32,
    /// Current animation FPS.
    pub anim_fps: f32,
    /// Whether current animation loops.
    pub anim_looping: bool,
    /// Whether current animation has finished.
    pub anim_finished: bool,
    /// Render offset (additive displacement, separate from position).
    pub offset_x: f32,
    pub offset_y: f32,
    /// Custom rotation origin. None = use sprite center (default).
    pub origin_x: Option<f32>,
    pub origin_y: Option<f32>,
    /// Which camera layer: "camGame", "camHUD", "camOther".
    pub camera: String,
    /// Additive color transform offsets (-255..255).
    pub color_red_offset: f32,
    pub color_green_offset: f32,
    pub color_blue_offset: f32,
}

/// A text object created by Lua via makeLuaText.
pub struct LuaText {
    pub tag: String,
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub alpha: f32,
    pub visible: bool,
    pub angle: f32,
    pub font: String,
    pub size: f32,
    pub color: String,
    pub border_size: f32,
    pub border_color: String,
    pub alignment: String,
    pub camera: String,
    pub antialiasing: bool,
}

pub enum LuaSpriteKind {
    /// Image sprite: image path relative to images/ dir.
    Image(String),
    /// Solid color graphic: (width, height, color_hex).
    Graphic { width: i32, height: i32, color: String },
    /// Animated sprite: image path for atlas.
    Animated(String),
}

/// A dynamic value that can be passed between Lua and Rust.
#[derive(Debug, Clone)]
pub enum LuaValue {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<LuaValue>),
}

impl LuaSprite {
    pub fn new(tag: &str, kind: LuaSpriteKind, x: f32, y: f32) -> Self {
        Self {
            tag: tag.to_string(),
            kind,
            x,
            y,
            scale_x: 1.0,
            scale_y: 1.0,
            scroll_x: 1.0,
            scroll_y: 1.0,
            alpha: 1.0,
            visible: true,
            angle: 0.0,
            flip_x: false,
            flip_y: false,
            antialiasing: true,
            color: [255, 255, 255],
            order: None,
            current_anim: String::new(),
            tex_w: 0.0,
            tex_h: 0.0,
            animations: HashMap::new(),
            anim_offsets: HashMap::new(),
            anim_frame: 0,
            anim_timer: 0.0,
            anim_fps: 24.0,
            anim_looping: true,
            anim_finished: false,
            offset_x: 0.0,
            offset_y: 0.0,
            origin_x: None,
            origin_y: None,
            camera: "camGame".to_string(),
            color_red_offset: 0.0,
            color_green_offset: 0.0,
            color_blue_offset: 0.0,
        }
    }
}

impl LuaText {
    pub fn new(tag: &str, text: &str, width: f32, x: f32, y: f32) -> Self {
        Self {
            tag: tag.to_string(),
            text: text.to_string(),
            x, y, width,
            alpha: 1.0,
            visible: true,
            angle: 0.0,
            font: String::new(),
            size: 16.0,
            color: "FFFFFF".to_string(),
            border_size: 0.0,
            border_color: "000000".to_string(),
            alignment: "left".to_string(),
            camera: "camGame".to_string(),
            antialiasing: true,
        }
    }
}

impl ScriptState {
    pub fn new() -> Self {
        Self {
            song_name: String::new(),
            is_story_mode: false,
            screen_width: 1280.0,
            screen_height: 720.0,
            cur_beat: 0,
            cur_step: 0,
            cur_section: 0,
            image_roots: Vec::new(),
            lua_sprites: HashMap::new(),
            sprites_to_add: Vec::new(),
            property_writes: Vec::new(),
            property_values: HashMap::new(),
            tweens: TweenManager::new(),
            strum_props: [StrumProps { x: 0.0, y: 0.0, alpha: 1.0, angle: 0.0, scale_x: 0.7, scale_y: 0.7, down_scroll: None, custom: false }; 8],
            sprites_to_remove: Vec::new(),
            song_position: 0.0,
            camera_zoom: 1.0,
            default_cam_zoom: 1.0,
            camera_speed: 1.0,
            health: 1.0,
            camera_target_requests: Vec::new(),
            triggered_events: Vec::new(),
            custom_vars: HashMap::new(),
            note_overrides: HashMap::new(),
            note_count: 0,
            note_read_data: Vec::new(),
            camera_section_requests: Vec::new(),
            lua_texts: HashMap::new(),
            texts_to_add: Vec::new(),
            camera_shake_requests: Vec::new(),
            camera_flash_requests: Vec::new(),
            subtitle_requests: Vec::new(),
            camera_forced_pos: false,
            opponent_camera_offset: (0.0, 0.0),
            bf_camera_offset: (0.0, 0.0),
            bf_name: String::new(),
            dad_name: String::new(),
            gf_name: String::new(),
            dad_anim_name: String::new(),
            bf_anim_name: String::new(),
            gf_anim_name: String::new(),
            dad_pos: (0.0, 0.0),
            bf_pos: (0.0, 0.0),
            gf_pos: (0.0, 0.0),
            char_position_adjustments: Vec::new(),
            stage_color_requests: Vec::new(),
            stage_color_swap_requests: Vec::new(),
            postprocess_requests: Vec::new(),
            postprocess_param_requests: Vec::new(),
            healthbar_color_requests: Vec::new(),
            stage_lights_request: None,
            reflections_request: None,
            note_type_registrations: Vec::new(),
            video_requests: Vec::new(),
        }
    }
}

impl Default for ScriptState {
    fn default() -> Self {
        Self::new()
    }
}
