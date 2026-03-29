mod init;
mod input;
mod update;
mod draw;

use std::collections::HashMap;

use winit::keyboard::KeyCode;

use rustic_audio::AudioEngine;
use rustic_core::paths::AssetPaths;
use rustic_gameplay::play_state::PlayState;
use rustic_render::camera::GameCamera;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::SpriteAtlas;
use rustic_scripting::{ScriptManager, LuaSpriteKind};

use crate::screen::Screen;
use super::characters::{Character, StageBgSprite};

// === Psych Engine constants ===
pub const GAME_W: f32 = 1280.0;
pub const GAME_H: f32 = 720.0;
pub(super) const STRUM_Y: f32 = 50.0;
const STRUM_X: f32 = 42.0;
pub(super) const NOTE_WIDTH: f32 = 112.0; // 160 * 0.7
pub(super) const NOTE_SCALE: f32 = 0.7;

const LANE_KEYS: [[KeyCode; 2]; 4] = [
    [KeyCode::KeyD, KeyCode::ArrowLeft],
    [KeyCode::KeyF, KeyCode::ArrowDown],
    [KeyCode::KeyJ, KeyCode::ArrowUp],
    [KeyCode::KeyK, KeyCode::ArrowRight],
];

// Health bar layout
const HEALTH_BAR_W: f32 = 600.0;
const HEALTH_BAR_H: f32 = 18.0;
pub(super) const HEALTH_BAR_Y: f32 = GAME_H - 80.0;
pub(super) const HEALTH_BAR_X: f32 = (GAME_W - HEALTH_BAR_W) / 2.0;

// Note atlas animation names per lane (left/down/up/right).
pub(super) const NOTE_ANIMS: [&str; 4] = ["purpleScroll", "blueScroll", "greenScroll", "redScroll"];
pub(super) const NOTE_PREFIXES: [&str; 4] = ["purple0", "blue0", "green0", "red0"];
pub(super) const STRUM_ANIMS: [&str; 4] = ["arrowLEFT", "arrowDOWN", "arrowUP", "arrowRIGHT"];
pub(super) const PRESS_ANIMS: [&str; 4] = ["left press", "down press", "up press", "right press"];
pub(super) const CONFIRM_ANIMS: [&str; 4] = ["left confirm", "down confirm", "up confirm", "right confirm"];
pub(super) const HOLD_PIECE_ANIMS: [&str; 4] = ["purple hold piece", "blue hold piece", "green hold piece", "red hold piece"];
pub(super) const HOLD_END_ANIMS: [&str; 4] = ["purple hold end", "blue hold end", "green hold end", "red hold end"];

/// A rating popup (visual only, Psych Engine physics).
pub(super) struct RatingPopup {
    pub rating_name: String,
    pub combo: i32,
    pub y: f32,
    pub vel_y: f32,
    pub age_ms: f64,
    pub fade_delay: f64,
    pub alpha: f32,
}

pub(super) const RATING_SCALE: f32 = 0.7;
pub(super) const RATING_ACCEL: f32 = 550.0;
pub(super) const RATING_VEL_Y: f32 = -160.0;
pub(super) const RATING_FADE_SECS: f32 = 0.2;

/// Loaded note sprite assets.
pub(super) struct NoteAssets {
    pub texture: GpuTexture,
    pub atlas: SpriteAtlas,
    pub tex_w: f32,
    pub tex_h: f32,
}

/// Rating/combo sprite textures.
pub(super) struct RatingAssets {
    pub sick: GpuTexture,
    pub good: GpuTexture,
    pub bad: GpuTexture,
    pub shit: GpuTexture,
    pub nums: [GpuTexture; 10],
}

/// Active note splash animation (visual only).
pub(super) struct NoteSplash {
    pub lane: usize,
    pub player: bool,
    pub frame: usize,
    pub timer: f64,
}

pub(super) const SPLASH_FPS: f64 = 24.0;
pub(super) const SPLASH_FRAMES: usize = 4;
pub(super) const SPLASH_PREFIXES: [&str; 4] = [
    "note splash purple 1", "note splash blue 1",
    "note splash green 1", "note splash red 1",
];

/// Draw order layer — determines what gets drawn when.
/// Built from stage `objects` array or hardcoded fallback.
#[derive(Debug, Clone)]
pub(super) enum DrawLayer {
    StageBg(usize),
    Gf,
    Dad,
    Bf,
}

/// 80sNightflaid visual phase state.
pub(super) struct NightflaidState {
    /// Whether the 80s phase is currently active.
    pub active: bool,
    /// unbeatableBG texture and position.
    pub bg_texture: Option<GpuTexture>,
    pub bg_x: f32,
    pub bg_y: f32,
    pub bg_angle: f32,
    pub bg_alpha: f32,
    /// Lightning bolt textures (animated sparrow).
    pub lightning_atlas: Option<SpriteAtlas>,
    pub lightning_texture: Option<GpuTexture>,
    pub lightning_tex_w: f32,
    pub lightning_tex_h: f32,
    pub lightning_left_x: f32,
    pub lightning_right_x: f32,
    pub lightning_frame: usize,
    pub lightning_timer: f32,
    /// Needle textures (animated sparrow).
    pub needle_atlas: Option<SpriteAtlas>,
    pub needle_texture: Option<GpuTexture>,
    pub needle_tex_w: f32,
    pub needle_tex_h: f32,
    pub needle_left_y: [f32; 3],
    pub needle_right_y: [f32; 3],
    pub needles_active: bool,
    /// unbeatableBG drift tween state.
    pub bg_tween_target_x: f32,
    pub bg_tween_target_angle: f32,
    pub bg_tween_duration: f32,
    pub bg_tween_elapsed: f32,
    pub bg_tween_start_x: f32,
    pub bg_tween_start_angle: f32,
    /// GF visible in 80s mode.
    pub gf_visible_80s: bool,
    /// GF position for 80s mode (slides up from below).
    pub gf_80s_y: f32,
    pub gf_80s_target_y: f32,
    pub gf_80s_tween_elapsed: f32,
    pub gf_80s_tween_duration: f32,
    /// VCR shader parameters being tweened.
    pub vcr_tween_elapsed: f32,
    pub vcr_tween_duration: f32,
    pub vcr_tween_active: bool,
    pub vcr_target_enabled: bool,

    // Stage background color overlay (left/right halves, like NightflaidShaderHandler).
    /// Current left-side stage background color [R,G,B,A].
    pub stage_color_left: [f32; 4],
    /// Current right-side stage background color [R,G,B,A].
    pub stage_color_right: [f32; 4],
    /// Color tween state for left side.
    pub color_left_tween_active: bool,
    pub color_left_start: [f32; 4],
    pub color_left_target: [f32; 4],
    pub color_left_elapsed: f32,
    pub color_left_duration: f32,
    /// Color tween state for right side.
    pub color_right_tween_active: bool,
    pub color_right_start: [f32; 4],
    pub color_right_target: [f32; 4],
    pub color_right_elapsed: f32,
    pub color_right_duration: f32,
    /// Whether stage lights are visible.
    pub lights_on: bool,
    /// Song-specific accent color (red for extirpatient, green for hexerpatient, cyan for extiraging).
    pub song_color: [f32; 4],
    /// Dark color constant.
    pub dark_color: [f32; 4],
    /// Whether the side-based color swap section is active (steps 1664-2944).
    pub side_swap_active: bool,
    /// Light ray blinking state.
    pub light_left_blink_time: f32,
    pub light_left_blink_count: f32,
    pub light_left_visible: bool,
    pub light_right_blink_time: f32,
    pub light_right_blink_count: f32,
    pub light_right_visible: bool,
    /// Red light pulse timer.
    pub red_light_time: f32,
    /// Red light pulse alpha.
    pub red_light_alpha: f32,
}

impl Default for NightflaidState {
    fn default() -> Self {
        Self {
            active: false,
            bg_texture: None,
            bg_x: -1000.0,
            bg_y: -900.0,
            bg_angle: 0.0,
            bg_alpha: 0.0,
            lightning_atlas: None,
            lightning_texture: None,
            lightning_tex_w: 0.0,
            lightning_tex_h: 0.0,
            lightning_left_x: 110.0 - 500.0,
            lightning_right_x: 1020.0 + 500.0,
            lightning_frame: 0,
            lightning_timer: 0.0,
            needle_atlas: None,
            needle_texture: None,
            needle_tex_w: 0.0,
            needle_tex_h: 0.0,
            needle_left_y: [0.0, 360.0, 720.0],
            needle_right_y: [0.0, 360.0, 720.0],
            needles_active: false,
            bg_tween_target_x: -1000.0,
            bg_tween_target_angle: 0.0,
            bg_tween_duration: 3.0,
            bg_tween_elapsed: 0.0,
            bg_tween_start_x: -1000.0,
            bg_tween_start_angle: 0.0,
            gf_visible_80s: false,
            gf_80s_y: 820.0, // off screen
            gf_80s_target_y: 100.0,
            gf_80s_tween_elapsed: 0.0,
            gf_80s_tween_duration: 0.75,
            vcr_tween_elapsed: 0.0,
            vcr_tween_duration: 1.0,
            vcr_tween_active: false,
            vcr_target_enabled: false,
            // Default stage colors: transparent until a color tween event fires
            stage_color_left: [0.0, 0.0, 0.0, 0.0],
            stage_color_right: [0.0, 0.0, 0.0, 0.0],
            color_left_tween_active: false,
            color_left_start: [0.0, 0.0, 0.0, 0.0],
            color_left_target: [0.0, 0.0, 0.0, 0.0],
            color_left_elapsed: 0.0,
            color_left_duration: 1.0,
            color_right_tween_active: false,
            color_right_start: [0.0, 0.0, 0.0, 0.0],
            color_right_target: [0.0, 0.0, 0.0, 0.0],
            color_right_elapsed: 0.0,
            color_right_duration: 1.0,
            lights_on: true,
            song_color: [0.984, 0.0, 0.176, 1.0], // #fb002d red (default for extirpatient)
            dark_color: [0.051, 0.051, 0.051, 1.0], // #0d0d0d
            side_swap_active: false,
            light_left_blink_time: 0.0,
            light_left_blink_count: 3.0,
            light_left_visible: true,
            light_right_blink_time: 0.0,
            light_right_blink_count: 3.0,
            light_right_visible: true,
            red_light_time: 0.0,
            red_light_alpha: 1.0,
        }
    }
}

/// Custom health bar (overlay + bar sprites, clipRect-style fill, color tweens).
/// Loaded when the opponent character has a `healthBarImg` field pointing to
/// `images/healthBars/<name>/` with bar.png and overlay.png.
pub(super) struct CustomHealthBar {
    pub bar_texture: GpuTexture,
    pub overlay_texture: GpuTexture,
    /// Scale factor (V-Slice uses 0.7).
    pub scale: f32,
    /// Overall alpha (starts at 0, tweens to 1 at beat 16).
    pub alpha: f32,
    /// Smoothed health value (lerped toward actual health each frame).
    pub health_lerp: f32,
    /// Current left bar color (opponent side) — RGBA premultiplied.
    pub left_color: [f32; 4],
    /// Current right bar color (player side) — RGBA premultiplied.
    pub right_color: [f32; 4],
    /// Color tween state.
    pub color_tween_elapsed: f32,
    pub color_tween_duration: f32,
    pub color_tween_active: bool,
    pub color_tween_start_left: [f32; 4],
    pub color_tween_start_right: [f32; 4],
    pub color_tween_target_left: [f32; 4],
    pub color_tween_target_right: [f32; 4],
    /// Saved player color (restored after form changes).
    pub saved_player_color: [f32; 4],
    /// Whether the bar has faded in yet.
    pub visible: bool,
}

impl CustomHealthBar {
    pub fn new(bar_texture: GpuTexture, overlay_texture: GpuTexture) -> Self {
        let default_left = [0.8, 0.0, 0.0, 1.0]; // red opponent
        let default_right = [0.19, 0.69, 0.82, 1.0]; // #31B0D1 BF blue
        Self {
            bar_texture,
            overlay_texture,
            scale: 0.7,
            alpha: 0.0,
            health_lerp: 1.0,
            left_color: default_left,
            right_color: default_right,
            color_tween_elapsed: 0.0,
            color_tween_duration: 1.0,
            color_tween_active: false,
            color_tween_start_left: default_left,
            color_tween_start_right: default_right,
            color_tween_target_left: default_left,
            color_tween_target_right: default_right,
            saved_player_color: default_right,
            visible: false,
        }
    }

    /// Start a color tween to new opponent/player colors.
    pub fn tween_colors(&mut self, left: [f32; 4], right: Option<[f32; 4]>, duration: f32) {
        self.color_tween_start_left = self.left_color;
        self.color_tween_start_right = self.right_color;
        self.color_tween_target_left = left;
        self.color_tween_target_right = right.unwrap_or(self.right_color);
        self.color_tween_duration = duration;
        self.color_tween_elapsed = 0.0;
        self.color_tween_active = true;
    }

    /// Update smoothed health and color tweens.
    pub fn update(&mut self, dt: f32, actual_health: f32) {
        // Health lerp (frame-rate-dependent like V-Slice: 0.15 factor)
        self.health_lerp += (actual_health - self.health_lerp) * 0.15;
        let visual_health = self.health_lerp.clamp(0.0, 1.7);
        let _ = visual_health; // used in draw

        // Color tween
        if self.color_tween_active {
            self.color_tween_elapsed += dt;
            let t = (self.color_tween_elapsed / self.color_tween_duration).min(1.0);
            // circOut ease
            let eased = (1.0 - (1.0 - t) * (1.0 - t)).sqrt();
            for i in 0..4 {
                self.left_color[i] = self.color_tween_start_left[i]
                    + (self.color_tween_target_left[i] - self.color_tween_start_left[i]) * eased;
                self.right_color[i] = self.color_tween_start_right[i]
                    + (self.color_tween_target_right[i] - self.color_tween_start_right[i]) * eased;
            }
            if t >= 1.0 {
                self.color_tween_active = false;
            }
        }
    }

    /// Fade in the bar (called at beat 16).
    pub fn fade_in(&mut self) {
        self.visible = true;
        // Instant-ish fade: set alpha to 1 (V-Slice uses 0.08s circOut, but we can approximate)
        self.alpha = 1.0;
    }
}

/// Death screen state (visual layer).
pub(super) struct DeathState {
    pub character: Character,
    pub phase: DeathPhase,
    pub timer: f64,
    pub fade_alpha: f32,
}

#[derive(PartialEq)]
pub(super) enum DeathPhase {
    FirstDeath,
    Loop,
    Confirm,
}

pub struct PlayScreen {
    // === Game logic (owned by rustic-gameplay) ===
    pub(super) game: PlayState,

    // === Rendering / audio state (owned by this screen) ===
    pub(super) audio: Option<AudioEngine>,
    pub(super) song_name: String,
    pub(super) difficulty: String,

    // Visual effects
    pub(super) rating_popups: Vec<RatingPopup>,
    pub(super) splashes: Vec<NoteSplash>,
    pub(super) countdown_alpha: f32,
    pub(super) countdown_swag: i32,
    pub(super) hud_zoom: f32,
    pub(super) icon_scale_bf: f32,
    pub(super) icon_scale_dad: f32,

    // Sprite assets
    pub(super) note_assets: Option<NoteAssets>,        // default / player note skin
    pub(super) opp_note_assets: Option<NoteAssets>,    // opponent note skin (if different)
    pub(super) rating_assets: Option<RatingAssets>,
    pub(super) splash_atlas: Option<NoteAssets>,
    pub(super) icon_bf: Option<GpuTexture>,
    pub(super) icon_dad: Option<GpuTexture>,
    pub(super) healthbar_tex: Option<GpuTexture>,
    /// Custom health bar (loaded when opponent has healthBarImg).
    pub(super) custom_healthbar: Option<CustomHealthBar>,
    pub(super) countdown_ready: Option<GpuTexture>,
    pub(super) countdown_set: Option<GpuTexture>,
    pub(super) countdown_go: Option<GpuTexture>,

    // Characters & Stage
    pub(super) char_bf: Option<Character>,
    pub(super) char_dad: Option<Character>,
    pub(super) char_gf: Option<Character>,
    pub(super) stage_bg: Vec<StageBgSprite>,
    pub(super) draw_order: Vec<DrawLayer>,
    pub(super) camera: GameCamera,
    pub(super) cam_bf: [f32; 2],
    pub(super) cam_dad: [f32; 2],
    // Camera offsets for dynamic recomputation at section changes
    pub(super) bf_cam_off: [f32; 2],
    pub(super) dad_cam_off: [f32; 2],
    pub(super) stage_cam_bf: [f32; 2],
    pub(super) stage_cam_dad: [f32; 2],
    pub(super) hb_color_bf: [f32; 4],
    pub(super) hb_color_dad: [f32; 4],
    pub(super) default_cam_zoom: f32,
    pub(super) cam_zooming: bool,
    pub(super) disable_zooming: bool,
    /// When true, camera follows camFollow position instead of character targets.
    pub(super) camera_forced_pos: bool,

    /// Chart events (sorted by strum_time, fired as song progresses).
    pub(super) chart_events: Vec<rustic_core::note::EventNote>,
    pub(super) event_index: usize,

    // Death
    pub(super) death: Option<DeathState>,
    pub(super) death_char_preloaded: Option<Character>,

    // Lua scripting
    pub(super) scripts: ScriptManager,
    pub(super) lua_textures: HashMap<String, GpuTexture>,
    pub(super) lua_atlases: HashMap<String, SpriteAtlas>,
    pub(super) lua_behind: Vec<String>,  // sprite tags drawn behind characters
    pub(super) lua_front: Vec<String>,   // sprite tags drawn in front of characters
    pub(super) paths: AssetPaths,
    /// Whether the character camera layer is visible (toggled by camCharacters.visible).
    pub(super) cam_characters_visible: bool,
    /// Whether character reflections are drawn (flipY copies below characters).
    pub(super) reflections_enabled: bool,
    /// Reflection alpha (0.35 in V-Slice ReflectShader).
    pub(super) reflection_alpha: f32,
    /// Reflection Y offset from bottom of character (-30 in V-Slice).
    pub(super) reflection_dist_y: f32,
    /// 80sNightflaid visual phase state (only used for nightflaid stage).
    pub(super) nightflaid: NightflaidState,
    /// Pending flags for 80s activation/deactivation (set in update, consumed in draw where gpu is available).
    pub(super) nightflaid_activate_pending: bool,
    pub(super) nightflaid_deactivate_pending: bool,

    /// Pending character change requests: (target, new_char_name)
    pub(super) char_change_requests: Vec<(String, String)>,
    /// Stage positions for character slots (for Change Character event)
    pub(super) stage_pos_bf: [f64; 2],
    pub(super) stage_pos_dad: [f64; 2],
    pub(super) stage_pos_gf: [f64; 2],
    pub(super) stage_name: String,

    // Frame timing
    pub(super) last_dt: f32,
    /// Downscroll mode: notes scroll down, health bar at top.
    pub(super) downscroll: bool,

    // Pause
    pub(super) paused: bool,
    pub(super) pause_selection: usize,
    pub(super) skip_target_ms: f64,
    pub(super) wants_restart: bool,
}

impl PlayScreen {
    pub fn new(song_name: &str, difficulty: &str) -> Self {
        Self {
            game: PlayState::new(100.0),
            audio: None,
            song_name: song_name.to_string(),
            difficulty: difficulty.to_string(),
            rating_popups: Vec::new(),
            splashes: Vec::new(),
            countdown_alpha: 0.0,
            countdown_swag: -1,
            hud_zoom: 1.0,
            icon_scale_bf: 1.0,
            icon_scale_dad: 1.0,
            note_assets: None,
            opp_note_assets: None,
            rating_assets: None,
            splash_atlas: None,
            icon_bf: None,
            icon_dad: None,
            healthbar_tex: None,
            custom_healthbar: None,
            countdown_ready: None,
            countdown_set: None,
            countdown_go: None,
            char_bf: None,
            char_dad: None,
            char_gf: None,
            stage_bg: Vec::new(),
            draw_order: Vec::new(),
            camera: GameCamera::new(0.9),
            cam_bf: [0.0; 2],
            cam_dad: [0.0; 2],
            bf_cam_off: [0.0; 2],
            dad_cam_off: [0.0; 2],
            stage_cam_bf: [0.0; 2],
            stage_cam_dad: [0.0; 2],
            hb_color_bf: [0.2, 0.8, 0.2, 1.0],
            hb_color_dad: [0.8, 0.1, 0.1, 1.0],
            default_cam_zoom: 0.9,
            cam_zooming: false,
            disable_zooming: false,
            camera_forced_pos: false,
            chart_events: Vec::new(),
            event_index: 0,
            death: None,
            death_char_preloaded: None,
            scripts: ScriptManager::new(),
            lua_textures: HashMap::new(),
            lua_atlases: HashMap::new(),
            lua_behind: Vec::new(),
            lua_front: Vec::new(),
            paths: AssetPaths::psych_default(),
            cam_characters_visible: true,
            reflections_enabled: false,
            reflection_alpha: 0.35,
            reflection_dist_y: -30.0,
            nightflaid: NightflaidState::default(),
            nightflaid_activate_pending: false,
            nightflaid_deactivate_pending: false,
            char_change_requests: Vec::new(),
            stage_pos_bf: [0.0; 2],
            stage_pos_dad: [0.0; 2],
            stage_pos_gf: [0.0; 2],
            stage_name: String::new(),
            last_dt: 1.0 / 60.0,
            downscroll: false,
            paused: false,
            pause_selection: 0,
            skip_target_ms: 0.0,
            wants_restart: false,
        }
    }

    pub(super) fn key_to_lane(key: KeyCode) -> Option<usize> {
        for (lane, binds) in LANE_KEYS.iter().enumerate() {
            if binds.contains(&key) {
                return Some(lane);
            }
        }
        None
    }

    pub(super) fn strum_x(lane: usize, player: bool) -> f32 {
        let base = STRUM_X + 50.0 + NOTE_WIDTH * lane as f32;
        if player { base + GAME_W / 2.0 } else { base }
    }

    /// Load 80sNightflaid visual assets (unbeatableBG, lightning, needles).
    pub(super) fn load_nightflaid_assets(&mut self, gpu: &GpuState, paths: &AssetPaths) {
        // Load unbeatableBG
        if let Some(p) = paths.find("images/nightflaid/unbeatableBG.png") {
            self.nightflaid.bg_texture = Some(gpu.load_texture_from_path(&p));
        }
        // Load lightning sparrow atlas
        if let Some(png_path) = paths.find("images/nightflaid/ycbu_lightning.png") {
            if let Some(xml_path) = paths.find("images/nightflaid/ycbu_lightning.xml") {
                let xml_str = std::fs::read_to_string(&xml_path).ok();
                if let Some(xml) = xml_str {
                    let tex = gpu.load_texture_from_path(&png_path);
                    let atlas = SpriteAtlas::from_xml(&xml);
                    // Register lightning animation
                    self.nightflaid.lightning_tex_w = tex.width as f32;
                    self.nightflaid.lightning_tex_h = tex.height as f32;
                    self.nightflaid.lightning_atlas = Some(atlas);
                    self.nightflaid.lightning_texture = Some(tex);
                }
            }
        }
        // Load needle sparrow atlas
        if let Some(png_path) = paths.find("images/nightflaid/needle.png") {
            if let Some(xml_path) = paths.find("images/nightflaid/needle.xml") {
                let xml_str = std::fs::read_to_string(&xml_path).ok();
                if let Some(xml) = xml_str {
                    let tex = gpu.load_texture_from_path(&png_path);
                    let atlas = SpriteAtlas::from_xml(&xml);
                    self.nightflaid.needle_tex_w = tex.width as f32;
                    self.nightflaid.needle_tex_h = tex.height as f32;
                    self.nightflaid.needle_atlas = Some(atlas);
                    self.nightflaid.needle_texture = Some(tex);
                }
            }
        }
        log::info!("Loaded 80sNightflaid assets: bg={}, lightning={}, needle={}",
            self.nightflaid.bg_texture.is_some(),
            self.nightflaid.lightning_texture.is_some(),
            self.nightflaid.needle_texture.is_some(),
        );
    }

    /// Activate the 80sNightflaid visual phase.
    pub(super) fn activate_80s_nightflaid(&mut self, gpu: &mut GpuState) {
        self.nightflaid.active = true;
        self.nightflaid.bg_alpha = 0.0; // Will tween to 1.0
        self.nightflaid.needles_active = true;
        self.nightflaid.gf_visible_80s = true;
        self.nightflaid.gf_80s_y = GAME_H + 100.0;
        self.nightflaid.gf_80s_target_y = 100.0;
        self.nightflaid.gf_80s_tween_elapsed = 0.0;
        self.nightflaid.gf_80s_tween_duration = 0.75;

        // Slide lightning bolts in from sides
        self.nightflaid.lightning_left_x = 110.0 - 500.0;
        self.nightflaid.lightning_right_x = 1020.0 + 500.0;

        // Start VCR shader tween
        self.nightflaid.vcr_tween_active = true;
        self.nightflaid.vcr_tween_elapsed = 0.0;
        self.nightflaid.vcr_tween_duration = 1.0;
        self.nightflaid.vcr_target_enabled = true;

        // Start unbeatableBG drift
        self.start_unbeatable_tween();

        // Enable post-processing
        gpu.set_postprocess_active(true);
        gpu.postprocess.uniforms.enabled = 1;

        // Hide game camera (characters render in 80s overlay)
        self.cam_characters_visible = false;
    }

    /// Deactivate the 80sNightflaid phase (return to normal stage).
    pub(super) fn deactivate_80s_nightflaid(&mut self, gpu: &mut GpuState) {
        self.nightflaid.active = false;
        self.nightflaid.needles_active = false;
        self.nightflaid.gf_visible_80s = false;
        self.nightflaid.bg_alpha = 0.0;

        // Disable VCR
        self.nightflaid.vcr_tween_active = true;
        self.nightflaid.vcr_tween_elapsed = 0.0;
        self.nightflaid.vcr_tween_duration = 0.5;
        self.nightflaid.vcr_target_enabled = false;

        // Show game camera again
        self.cam_characters_visible = true;
    }

    /// Start a new random drift tween for unbeatableBG.
    fn start_unbeatable_tween(&mut self) {
        self.nightflaid.bg_tween_start_x = self.nightflaid.bg_x;
        self.nightflaid.bg_tween_start_angle = self.nightflaid.bg_angle;
        // Random target (matching V-Slice ranges)
        let seed = (self.game.conductor.song_position * 7.3).sin().abs() as f32;
        self.nightflaid.bg_tween_target_x = -1000.0 + (seed * 2.0 - 1.0) * 400.0;
        let seed2 = (self.game.conductor.song_position * 11.1).cos().abs() as f32;
        self.nightflaid.bg_tween_target_angle = (seed2 * 2.0 - 1.0) * 25.0;
        self.nightflaid.bg_tween_duration = 2.5 + seed * 2.0;
        self.nightflaid.bg_tween_elapsed = 0.0;
    }

    /// Update 80sNightflaid animations each frame.
    pub(super) fn update_nightflaid(&mut self, dt: f32, gpu: &mut GpuState) {
        // Stage color tweens and light effects run regardless of 80s phase
        self.update_nightflaid_colors(dt);
        self.update_nightflaid_lights(dt);

        if !self.nightflaid.active && !self.nightflaid.vcr_tween_active {
            return;
        }

        // VCR shader tween
        if self.nightflaid.vcr_tween_active {
            self.nightflaid.vcr_tween_elapsed += dt;
            let t = (self.nightflaid.vcr_tween_elapsed / self.nightflaid.vcr_tween_duration).min(1.0);
            // quadOut ease
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            if self.nightflaid.vcr_target_enabled {
                gpu.postprocess.uniforms.scanline_intensity = eased;
                gpu.postprocess.uniforms.distortion_mult = eased;
                gpu.postprocess.uniforms.chromatic_aberration = eased;
                gpu.postprocess.uniforms.vignette_intensity = eased * 0.5;
            } else {
                gpu.postprocess.uniforms.scanline_intensity = 1.0 - eased;
                gpu.postprocess.uniforms.distortion_mult = 1.0 - eased;
                gpu.postprocess.uniforms.chromatic_aberration = 1.0 - eased;
                gpu.postprocess.uniforms.vignette_intensity = (1.0 - eased) * 0.5;
            }
            if t >= 1.0 {
                self.nightflaid.vcr_tween_active = false;
                if !self.nightflaid.vcr_target_enabled {
                    gpu.postprocess.uniforms.enabled = 0;
                    gpu.set_postprocess_active(false);
                }
            }
        }

        if !self.nightflaid.active {
            return;
        }

        // Update time for shader
        gpu.postprocess.uniforms.time += dt;

        // unbeatableBG alpha fade in
        if self.nightflaid.bg_alpha < 1.0 {
            self.nightflaid.bg_alpha = (self.nightflaid.bg_alpha + dt / 0.75).min(1.0);
        }

        // unbeatableBG drift tween
        self.nightflaid.bg_tween_elapsed += dt;
        let t = (self.nightflaid.bg_tween_elapsed / self.nightflaid.bg_tween_duration).min(1.0);
        self.nightflaid.bg_x = self.nightflaid.bg_tween_start_x
            + (self.nightflaid.bg_tween_target_x - self.nightflaid.bg_tween_start_x) * t;
        self.nightflaid.bg_angle = self.nightflaid.bg_tween_start_angle
            + (self.nightflaid.bg_tween_target_angle - self.nightflaid.bg_tween_start_angle) * t;
        if t >= 1.0 {
            self.start_unbeatable_tween();
        }

        // Lightning animation (24 fps)
        self.nightflaid.lightning_timer += dt;
        if self.nightflaid.lightning_timer >= 1.0 / 24.0 {
            self.nightflaid.lightning_timer -= 1.0 / 24.0;
            self.nightflaid.lightning_frame += 1;
        }

        // Lightning slide in (from offscreen to visible)
        let lightning_target_left = 110.0;
        let lightning_target_right = 1020.0;
        if self.nightflaid.lightning_left_x < lightning_target_left {
            // quadOut ease towards target
            self.nightflaid.lightning_left_x += (lightning_target_left - self.nightflaid.lightning_left_x) * dt * 3.0;
        }
        if self.nightflaid.lightning_right_x > lightning_target_right {
            self.nightflaid.lightning_right_x += (lightning_target_right - self.nightflaid.lightning_right_x) * dt * 3.0;
        }

        // Needle scrolling
        if self.nightflaid.needles_active {
            for y in &mut self.nightflaid.needle_left_y {
                *y -= 360.0 * dt;
                if *y < -200.0 {
                    *y += (720.0 / 2.0) * 3.0;
                }
            }
            for y in &mut self.nightflaid.needle_right_y {
                *y += 360.0 * dt;
                if *y > GAME_H {
                    *y -= (720.0 / 2.0) * 3.0;
                }
            }
        }

        // GF slide-up tween
        if self.nightflaid.gf_visible_80s && self.nightflaid.gf_80s_tween_elapsed < self.nightflaid.gf_80s_tween_duration {
            self.nightflaid.gf_80s_tween_elapsed += dt;
            let t = (self.nightflaid.gf_80s_tween_elapsed / self.nightflaid.gf_80s_tween_duration).min(1.0);
            // circOut ease
            let eased = (1.0 - (1.0 - t) * (1.0 - t)).sqrt();
            let start_y = GAME_H + 100.0;
            self.nightflaid.gf_80s_y = start_y + (self.nightflaid.gf_80s_target_y - start_y) * eased;
        }
    }

    /// Update nightflaid stage background color tweens.
    fn update_nightflaid_colors(&mut self, dt: f32) {
        if self.nightflaid.color_left_tween_active {
            self.nightflaid.color_left_elapsed += dt;
            let t = (self.nightflaid.color_left_elapsed / self.nightflaid.color_left_duration).min(1.0);
            for i in 0..4 {
                self.nightflaid.stage_color_left[i] = self.nightflaid.color_left_start[i]
                    + (self.nightflaid.color_left_target[i] - self.nightflaid.color_left_start[i]) * t;
            }
            if t >= 1.0 { self.nightflaid.color_left_tween_active = false; }
        }
        if self.nightflaid.color_right_tween_active {
            self.nightflaid.color_right_elapsed += dt;
            let t = (self.nightflaid.color_right_elapsed / self.nightflaid.color_right_duration).min(1.0);
            for i in 0..4 {
                self.nightflaid.stage_color_right[i] = self.nightflaid.color_right_start[i]
                    + (self.nightflaid.color_right_target[i] - self.nightflaid.color_right_start[i]) * t;
            }
            if t >= 1.0 { self.nightflaid.color_right_tween_active = false; }
        }
    }

    /// Tween the left stage color.
    pub(super) fn nightflaid_color_tween_left(&mut self, color: [f32; 4], dur: f32) {
        self.nightflaid.color_left_start = self.nightflaid.stage_color_left;
        self.nightflaid.color_left_target = color;
        self.nightflaid.color_left_elapsed = 0.0;
        self.nightflaid.color_left_duration = dur;
        self.nightflaid.color_left_tween_active = true;
    }

    /// Tween the right stage color.
    pub(super) fn nightflaid_color_tween_right(&mut self, color: [f32; 4], dur: f32) {
        self.nightflaid.color_right_start = self.nightflaid.stage_color_right;
        self.nightflaid.color_right_target = color;
        self.nightflaid.color_right_elapsed = 0.0;
        self.nightflaid.color_right_duration = dur;
        self.nightflaid.color_right_tween_active = true;
    }

    /// Tween both stage colors to the same target.
    pub(super) fn nightflaid_color_tween_both(&mut self, color: [f32; 4], dur: f32) {
        self.nightflaid_color_tween_left(color, dur);
        self.nightflaid_color_tween_right(color, dur);
    }

    /// Update nightflaid light blinking and pulsing effects.
    fn update_nightflaid_lights(&mut self, dt: f32) {
        if !self.nightflaid.lights_on { return; }

        // Left light ray blinking
        self.nightflaid.light_left_blink_time -= dt * 5.0;
        if self.nightflaid.light_left_blink_time < 0.0 {
            self.nightflaid.light_left_blink_count -= 1.0;
            if self.nightflaid.light_left_blink_count <= 0.0 {
                // Reset: random wait before next blink cycle
                self.nightflaid.light_left_blink_count =
                    2.0 + (self.nightflaid.red_light_time * 3.7).sin().abs() * 2.0;
                self.nightflaid.light_left_blink_time =
                    12.0 + (self.nightflaid.red_light_time * 5.1).sin().abs() * 8.0;
                self.nightflaid.light_left_visible = true;
            } else {
                self.nightflaid.light_left_blink_time = 1.0;
                self.nightflaid.light_left_visible = !self.nightflaid.light_left_visible;
            }
        }

        // Right light ray blinking
        self.nightflaid.light_right_blink_time -= dt * 5.0;
        if self.nightflaid.light_right_blink_time < 0.0 {
            self.nightflaid.light_right_blink_count -= 1.0;
            if self.nightflaid.light_right_blink_count <= 0.0 {
                self.nightflaid.light_right_blink_count =
                    2.0 + (self.nightflaid.red_light_time * 4.3).sin().abs() * 2.0;
                self.nightflaid.light_right_blink_time =
                    12.0 + (self.nightflaid.red_light_time * 6.7).sin().abs() * 8.0;
                self.nightflaid.light_right_visible = true;
            } else {
                self.nightflaid.light_right_blink_time = 1.0;
                self.nightflaid.light_right_visible = !self.nightflaid.light_right_visible;
            }
        }

        // Red light pulse
        self.nightflaid.red_light_time += dt;
        self.nightflaid.red_light_alpha =
            (std::f32::consts::PI * self.nightflaid.red_light_time).sin() * 0.25 + 0.85;
    }

    /// Get strum position/alpha/angle/scale from modchart state. Falls back to defaults.
    /// Returns (x, y, alpha, angle_degrees, scale).
    pub(super) fn strum_pos(&self, lane: usize, player: bool) -> (f32, f32, f32, f32, f32) {
        let idx = if player { lane + 4 } else { lane };
        let sp = &self.scripts.state.strum_props[idx];
        if sp.custom {
            (sp.x, sp.y, sp.alpha, sp.angle, sp.scale_x)
        } else {
            (Self::strum_x(lane, player), STRUM_Y, 1.0, 0.0, NOTE_SCALE)
        }
    }

    /// Load a custom note skin (PNG + XML) and register standard animations.
    pub(super) fn load_note_skin(
        &self,
        gpu: &rustic_render::gpu::GpuState,
        paths: &rustic_core::paths::AssetPaths,
        skin_path: &str,
    ) -> Option<NoteAssets> {
        let png = paths.image(skin_path)?;
        let xml_path = paths.image_xml(skin_path)?;
        let xml_str = std::fs::read_to_string(&xml_path).ok()?;
        let tex = gpu.load_texture_from_path(&png);
        let mut atlas = rustic_render::sprites::SpriteAtlas::from_xml(&xml_str);
        for (anim, prefix) in NOTE_ANIMS.iter().zip(NOTE_PREFIXES.iter()) {
            atlas.add_by_prefix(anim, prefix);
        }
        for prefix in STRUM_ANIMS.iter().chain(PRESS_ANIMS.iter())
            .chain(CONFIRM_ANIMS.iter()).chain(HOLD_PIECE_ANIMS.iter())
            .chain(HOLD_END_ANIMS.iter())
        {
            atlas.add_by_prefix(prefix, prefix);
        }
        Some(NoteAssets {
            tex_w: tex.width as f32,
            tex_h: tex.height as f32,
            texture: tex,
            atlas,
        })
    }

    /// Process queued character change requests (needs GPU for texture loading).
    pub(super) fn process_char_changes(&mut self, gpu: &GpuState) {
        use rustic_core::character::CharacterFile;
        use super::characters::{AtlasCharacterSprite, CharacterSprite};

        let requests: Vec<(String, String)> = self.char_change_requests.drain(..).collect();
        for (target, char_name) in requests {
            let json_path = match self.paths.character_json(&char_name) {
                Some(p) => p,
                None => {
                    log::warn!("Change Character: can't find {}.json", char_name);
                    continue;
                }
            };
            let json_str = match std::fs::read_to_string(&json_path) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("Change Character: can't read {:?}: {}", json_path, e);
                    continue;
                }
            };
            let char_def = match CharacterFile::from_json(&json_str) {
                Ok(d) => d,
                Err(e) => {
                    log::warn!("Change Character: can't parse {:?}: {}", json_path, e);
                    continue;
                }
            };

            let effective_image = char_def.effective_image().to_string();
            let is_player = matches!(target.as_str(), "bf" | "boyfriend" | "0");

            // Use the stored stage positions for the target slot
            let (stage_x, stage_y) = match target.as_str() {
                "bf" | "boyfriend" | "0" => (self.stage_pos_bf[0], self.stage_pos_bf[1]),
                "gf" | "girlfriend" | "2" => (self.stage_pos_gf[0], self.stage_pos_gf[1]),
                _ => (self.stage_pos_dad[0], self.stage_pos_dad[1]),
            };

            // Load the new character (atlas or sparrow)
            let new_char = if let Some(animate_dir) = self.paths.character_animate_dir(&effective_image) {
                log::info!("Change Character: loading atlas {} from {:?}", char_name, animate_dir);
                let mut sprite = AtlasCharacterSprite::load(gpu, &char_def, &animate_dir, stage_x, stage_y, is_player);
                if let Some(&s) = char_def.stage_scale.get(&self.stage_name) {
                    sprite.scale = s as f32;
                }
                Some(Character::Atlas(sprite))
            } else if let Some(atlas_dir) = self.paths.character_atlas_dir(&effective_image) {
                log::info!("Change Character: loading sparrow {} from {:?}", char_name, atlas_dir);
                let mut sprite = CharacterSprite::load(gpu, &json_path, &atlas_dir, stage_x, stage_y, is_player);
                if let Some(&s) = char_def.stage_scale.get(&self.stage_name) {
                    sprite.scale = s as f32;
                }
                Some(Character::Sparrow(sprite))
            } else {
                log::warn!("Change Character: can't find atlas for image '{}'", effective_image);
                None
            };

            if let Some(ch) = new_char {
                match target.as_str() {
                    "bf" | "boyfriend" | "0" => self.char_bf = Some(ch),
                    "gf" | "girlfriend" | "2" => self.char_gf = Some(ch),
                    _ => self.char_dad = Some(ch),
                }
                // Recompute camera targets with new character
                self.recompute_camera_targets();
            }
        }
    }

    /// Recompute camera targets from current character positions (called at section changes).
    /// Matches Psych Engine's moveCamera() which reads character midpoints dynamically.
    pub(super) fn recompute_camera_targets(&mut self) {
        if let Some(bf) = &self.char_bf {
            let (mx, my) = bf.midpoint();
            self.cam_bf = [
                mx - 100.0 - self.bf_cam_off[0] + self.stage_cam_bf[0],
                my - 100.0 + self.bf_cam_off[1] + self.stage_cam_bf[1],
            ];
        }
        if let Some(dad) = &self.char_dad {
            let (mx, my) = dad.midpoint();
            self.cam_dad = [
                mx + 150.0 + self.dad_cam_off[0] + self.stage_cam_dad[0],
                my - 100.0 + self.dad_cam_off[1] + self.stage_cam_dad[1],
            ];
        }
    }

    /// Process game-level property writes from Lua (defaultCamZoom, cameraSpeed, etc.).
    pub(super) fn process_property_writes(&mut self) {
        use rustic_scripting::LuaValue;
        let writes: Vec<(String, LuaValue)> = self.scripts.state.property_writes.drain(..).collect();
        for (prop, val) in writes {
            let as_f32 = match &val {
                LuaValue::Float(f) => Some(*f as f32),
                LuaValue::Int(i) => Some(*i as f32),
                _ => None,
            };
            match prop.as_str() {
                "defaultCamZoom" => {
                    if let Some(v) = as_f32 {
                        self.default_cam_zoom = v;
                        self.camera.target_zoom = v;
                    }
                }
                "cameraSpeed" => {
                    if let Some(v) = as_f32 {
                        self.camera.camera_speed = v;
                    }
                }
                "camera.zoom" => {
                    if let Some(v) = as_f32 {
                        self.camera.zoom = v;
                    }
                }
                "health" => {
                    if let Some(v) = as_f32 {
                        self.game.score.health = v.clamp(0.0, 2.0);
                    }
                }
                "isCameraOnForcedPos" => {
                    if let LuaValue::Bool(b) = &val {
                        self.camera_forced_pos = *b;
                    }
                }
                "camFollow.x" | "camFollowPos.x" => {
                    if let Some(v) = as_f32 {
                        if self.camera_forced_pos {
                            self.camera.follow(v, self.camera.y);
                        }
                    }
                }
                "camFollow.y" | "camFollowPos.y" => {
                    if let Some(v) = as_f32 {
                        if self.camera_forced_pos {
                            self.camera.follow(self.camera.x, v);
                        }
                    }
                }
                "camZooming" => {
                    if let LuaValue::Bool(b) = &val {
                        self.cam_zooming = *b;
                    }
                }
                "camGame.zoom" => {
                    if let Some(v) = as_f32 {
                        self.camera.zoom = v;
                    }
                }
                "camHUD.zoom" => {
                    if let Some(v) = as_f32 {
                        self.hud_zoom = v;
                    }
                }
                "camGame.visible" => {
                    // TODO: toggle game camera visibility
                }
                "__charPlayAnim.dad" | "__charPlayAnim.opponent" => {
                    if let LuaValue::String(anim) = &val {
                        if let Some(dad) = &mut self.char_dad {
                            dad.play_anim(anim, true);
                        }
                    }
                }
                "__charPlayAnim.bf" | "__charPlayAnim.boyfriend" => {
                    if let LuaValue::String(anim) = &val {
                        if let Some(bf) = &mut self.char_bf {
                            bf.play_anim(anim, true);
                        }
                    }
                }
                "__charPlayAnim.gf" | "__charPlayAnim.girlfriend" => {
                    if let LuaValue::String(anim) = &val {
                        if let Some(gf) = &mut self.char_gf {
                            gf.play_anim(anim, true);
                        }
                    }
                }
                "__charDance.dad" | "__charDance.opponent" => {
                    if let Some(dad) = &mut self.char_dad {
                        dad.dance();
                    }
                }
                "__charDance.bf" | "__charDance.boyfriend" => {
                    if let Some(bf) = &mut self.char_bf {
                        bf.dance();
                    }
                }
                "__charDance.gf" | "__charDance.girlfriend" => {
                    if let Some(gf) = &mut self.char_gf {
                        gf.dance();
                    }
                }
                "opponentCameraOffset.x" => {
                    if let Some(v) = as_f32 {
                        self.scripts.state.opponent_camera_offset.0 = v;
                    }
                }
                "opponentCameraOffset.y" => {
                    if let Some(v) = as_f32 {
                        self.scripts.state.opponent_camera_offset.1 = v;
                    }
                }
                "boyfriendCameraOffset.x" => {
                    if let Some(v) = as_f32 {
                        self.scripts.state.bf_camera_offset.0 = v;
                    }
                }
                "boyfriendCameraOffset.y" => {
                    if let Some(v) = as_f32 {
                        self.scripts.state.bf_camera_offset.1 = v;
                    }
                }
                "__camCharactersVisible" => {
                    if let LuaValue::Bool(b) = &val {
                        self.cam_characters_visible = *b;
                    }
                }
                "dad.animationSuffix" | "opponent.animationSuffix" => {
                    if let LuaValue::String(s) = &val {
                        if let Some(dad) = &mut self.char_dad {
                            dad.set_anim_suffix(s);
                        }
                    }
                }
                "boyfriend.animationSuffix" | "bf.animationSuffix" => {
                    if let LuaValue::String(s) = &val {
                        if let Some(bf) = &mut self.char_bf {
                            bf.set_anim_suffix(s);
                        }
                    }
                }
                "gf.animationSuffix" | "girlfriend.animationSuffix" => {
                    if let LuaValue::String(s) = &val {
                        if let Some(gf) = &mut self.char_gf {
                            gf.set_anim_suffix(s);
                        }
                    }
                }
                "dad.idleSuffix" | "opponent.idleSuffix" => {
                    if let LuaValue::String(s) = &val {
                        if let Some(dad) = &mut self.char_dad {
                            dad.set_idle_suffix(s);
                        }
                    }
                }
                "boyfriend.idleSuffix" | "bf.idleSuffix" => {
                    if let LuaValue::String(s) = &val {
                        if let Some(bf) = &mut self.char_bf {
                            bf.set_idle_suffix(s);
                        }
                    }
                }
                "gf.idleSuffix" | "girlfriend.idleSuffix" => {
                    if let LuaValue::String(s) = &val {
                        if let Some(gf) = &mut self.char_gf {
                            gf.set_idle_suffix(s);
                        }
                    }
                }
                // Character position/alpha/visibility
                "dad.x" | "dadGroup.x" => {
                    if let Some(v) = as_f32 { if let Some(c) = &mut self.char_dad { c.set_x(v); } }
                }
                "dad.y" | "dadGroup.y" => {
                    if let Some(v) = as_f32 { if let Some(c) = &mut self.char_dad { c.set_y(v); } }
                }
                "boyfriend.x" | "bf.x" | "boyfriendGroup.x" => {
                    if let Some(v) = as_f32 { if let Some(c) = &mut self.char_bf { c.set_x(v); } }
                }
                "boyfriend.y" | "bf.y" | "boyfriendGroup.y" => {
                    if let Some(v) = as_f32 { if let Some(c) = &mut self.char_bf { c.set_y(v); } }
                }
                "gf.x" | "girlfriend.x" | "gfGroup.x" => {
                    if let Some(v) = as_f32 { if let Some(c) = &mut self.char_gf { c.set_x(v); } }
                }
                "gf.y" | "girlfriend.y" | "gfGroup.y" => {
                    if let Some(v) = as_f32 { if let Some(c) = &mut self.char_gf { c.set_y(v); } }
                }
                "dad.alpha" | "dadGroup.alpha" => {
                    if let Some(v) = as_f32 { if let Some(c) = &mut self.char_dad { c.set_alpha(v); } }
                }
                "boyfriend.alpha" | "bf.alpha" | "boyfriendGroup.alpha" => {
                    if let Some(v) = as_f32 { if let Some(c) = &mut self.char_bf { c.set_alpha(v); } }
                }
                "gf.alpha" | "girlfriend.alpha" | "gfGroup.alpha" => {
                    if let Some(v) = as_f32 { if let Some(c) = &mut self.char_gf { c.set_alpha(v); } }
                }
                "hud.zoom" => {
                    if let Some(v) = as_f32 {
                        self.hud_zoom = v;
                    }
                }
                _ => {
                    log::debug!("Unhandled property write: {} = {:?}", prop, val);
                }
            }
        }

        // Sync note overrides from scripting state to NoteData
        if !self.scripts.state.note_overrides.is_empty() {
            let overrides = std::mem::take(&mut self.scripts.state.note_overrides);
            for (idx, fields) in overrides {
                if idx >= self.game.notes.len() { continue; }
                let note = &mut self.game.notes[idx];
                for (field, val) in &fields {
                    match field.as_str() {
                        "visible" => note.visible = *val != 0.0,
                        "alpha" => note.alpha = *val as f32,
                        "scale.x" => note.scale_x = *val as f32,
                        "scale.y" => note.scale_y = *val as f32,
                        "angle" => note.angle = *val as f32,
                        "flipY" => note.flip_y = *val != 0.0,
                        "correctionOffset" => note.correction_offset = *val as f32,
                        "isReversingScroll" => note.is_reversing_scroll = *val != 0.0,
                        "offsetX" | "offset.x" => note.offset_x = *val as f32,
                        "offsetY" | "offset.y" => note.offset_y = *val as f32,
                        "colorTransform.redOffset" => note.color_r_offset = *val as f32,
                        "colorTransform.greenOffset" => note.color_g_offset = *val as f32,
                        "colorTransform.blueOffset" => note.color_b_offset = *val as f32,
                        _ => {}
                    }
                }
            }
        }
    }

    /// Process pending character position adjustments from runHaxeCode.
    pub(super) fn process_char_positions(&mut self) {
        let adjustments: Vec<(String, String, f64)> =
            self.scripts.state.char_position_adjustments.drain(..).collect();

        let mut i = 0;
        while i < adjustments.len() {
            let (ref char_name, ref field, value) = adjustments[i];

            // NaN is a marker that the next entry is an absolute set
            if value.is_nan() && i + 1 < adjustments.len() {
                let abs_val = adjustments[i + 1].2 as f32;
                self.apply_char_pos(char_name, field, abs_val, false);
                i += 2;
                continue;
            }

            // Otherwise it's a delta
            self.apply_char_pos(char_name, field, value as f32, true);
            i += 1;
        }
    }

    fn apply_char_pos(&mut self, char_name: &str, field: &str, value: f32, is_delta: bool) {
        let char = match char_name {
            "boyfriend" => self.char_bf.as_mut(),
            "dad" => self.char_dad.as_mut(),
            "gf" => self.char_gf.as_mut(),
            _ => return,
        };
        let Some(ch) = char else { return };
        match (field, is_delta) {
            ("x", true) => ch.set_x(ch.x() + value),
            ("y", true) => ch.set_y(ch.y() + value),
            ("x", false) => ch.set_x(value),
            ("y", false) => ch.set_y(value),
            _ => {}
        }
        log::debug!("char position: {}.{} {} {}", char_name, field, if is_delta { "+=" } else { "=" }, value);
    }

    /// Process pending Lua sprite additions: load textures and add to draw lists.
    pub(super) fn process_lua_sprites(&mut self, gpu: &GpuState) {
        let adds: Vec<(String, bool)> = self.scripts.state.sprites_to_add.drain(..).collect();
        for (tag, in_front) in adds {
            // Skip if already added
            if self.lua_textures.contains_key(&tag) { continue; }

            // Load texture based on sprite kind
            let sprite = match self.scripts.state.lua_sprites.get(&tag) {
                Some(s) => s,
                None => continue,
            };
            // Hide large white makeGraphic sprites when shaders are disabled —
            // these are shader fill layers (e.g., N_clr in nightflaid) that render
            // as opaque white rectangles without their intended shader.
            let hide_shader_sprite = matches!(&sprite.kind,
                LuaSpriteKind::Graphic { width, height, color }
                if (color == "FFFFFF" || color == "ffffff" || color == "#FFFFFF")
                    && (*width as i64) * (*height as i64) > 500_000
            );

            let tex = match &sprite.kind {
                LuaSpriteKind::Image(image) => {
                    if let Some(png) = self.paths.image(image) {
                        gpu.load_texture_from_path(&png)
                    } else {
                        log::warn!("Lua sprite '{}': image '{}' not found", tag, image);
                        continue;
                    }
                }
                LuaSpriteKind::Graphic { width, height, color } => {
                    gpu.create_solid_texture(*width as u32, *height as u32, color)
                }
                LuaSpriteKind::Animated(image) => {
                    if let Some(png) = self.paths.image(image) {
                        // Load XML atlas alongside the PNG
                        if let Some(xml_path) = self.paths.image_xml(image) {
                            if let Ok(xml_str) = std::fs::read_to_string(&xml_path) {
                                let mut atlas = SpriteAtlas::from_xml(&xml_str);
                                // Register all animations defined by Lua scripts
                                if let Some(spr) = self.scripts.state.lua_sprites.get(&tag) {
                                    for (anim_name, def) in &spr.animations {
                                        if def.indices.is_empty() {
                                            atlas.add_by_prefix(anim_name, &def.prefix);
                                        } else {
                                            atlas.add_by_indices(anim_name, &def.prefix, &def.indices);
                                        }
                                    }
                                }
                                self.lua_atlases.insert(tag.clone(), atlas);
                            }
                        }
                        gpu.load_texture_from_path(&png)
                    } else {
                        log::warn!("Lua sprite '{}': animated image '{}' not found", tag, image);
                        continue;
                    }
                }
            };

            // Store texture dimensions in sprite so getProperty can return width/height
            let tex_w = tex.width as f32;
            let tex_h = tex.height as f32;
            if let Some(sprite) = self.scripts.state.lua_sprites.get_mut(&tag) {
                sprite.tex_w = tex_w;
                sprite.tex_h = tex_h;
                if hide_shader_sprite {
                    sprite.visible = false;
                }
            }

            self.lua_textures.insert(tag.clone(), tex);
            if in_front {
                self.lua_front.push(tag);
            } else {
                self.lua_behind.push(tag);
            }
        }

        // Process sprite removals
        let removes: Vec<String> = self.scripts.state.sprites_to_remove.drain(..).collect();
        for tag in &removes {
            self.lua_textures.remove(tag);
            self.lua_atlases.remove(tag);
            self.lua_behind.retain(|t| t != tag);
            self.lua_front.retain(|t| t != tag);
            self.scripts.state.lua_sprites.remove(tag);
        }

        // Register any new animations on existing atlases (for late addAnimationByPrefix calls)
        for (tag, sprite) in &self.scripts.state.lua_sprites {
            if let Some(atlas) = self.lua_atlases.get_mut(tag) {
                for (anim_name, def) in &sprite.animations {
                    if !atlas.has_anim(anim_name) {
                        if def.indices.is_empty() {
                            atlas.add_by_prefix(anim_name, &def.prefix);
                        } else {
                            atlas.add_by_indices(anim_name, &def.prefix, &def.indices);
                        }
                    }
                }
            }
        }
    }
}

impl Screen for PlayScreen {
    fn init(&mut self, gpu: &GpuState) {
        self.init_inner(gpu);
    }

    fn handle_key(&mut self, key: KeyCode) {
        self.handle_key_inner(key);
    }

    fn handle_key_release(&mut self, key: KeyCode) {
        if let Some(lane) = Self::key_to_lane(key) {
            self.game.key_release(lane);
        }
    }

    fn update(&mut self, dt: f32) {
        self.update_inner(dt);
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        self.draw_inner(gpu);
    }

    fn next_screen(&mut self) -> Option<Box<dyn Screen>> {
        if self.wants_restart {
            self.wants_restart = false;
            Some(Box::new(PlayScreen::new(&self.song_name, &self.difficulty)))
        } else if self.game.song_ended {
            self.game.song_ended = false;
            Some(Box::new(super::freeplay::FreeplayScreen::new()))
        } else {
            None
        }
    }
}
