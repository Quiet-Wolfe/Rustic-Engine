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

    /// Pending camera target changes: "dad"/"bf"/"gf".
    pub camera_target_requests: Vec<String>,

    /// Pending triggered events: (name, v1, v2).
    pub triggered_events: Vec<(String, String, String)>,
}

/// Per-strum-note visual properties (modchart overrides).
#[derive(Debug, Clone, Copy)]
pub struct StrumProps {
    pub x: f32,
    pub y: f32,
    pub alpha: f32,
    pub angle: f32,
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
            strum_props: [StrumProps { x: 0.0, y: 0.0, alpha: 1.0, angle: 0.0, custom: false }; 8],
            sprites_to_remove: Vec::new(),
            song_position: 0.0,
            camera_zoom: 1.0,
            default_cam_zoom: 1.0,
            camera_speed: 1.0,
            camera_target_requests: Vec::new(),
            triggered_events: Vec::new(),
        }
    }
}

impl Default for ScriptState {
    fn default() -> Self {
        Self::new()
    }
}
