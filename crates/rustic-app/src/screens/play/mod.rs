mod init;
mod input;
mod update;
mod draw;

use std::collections::HashMap;

use winit::event::TouchPhase;
use winit::keyboard::KeyCode;

use rustic_audio::AudioEngine;
use rustic_core::prefs::Preferences;
use rustic_core::paths::AssetPaths;
use rustic_core::highscore::HighscoreStore;
use rustic_gameplay::play_state::PlayState;
use rustic_render::camera::GameCamera;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::SpriteAtlas;
use rustic_scripting::{ScriptManager, LuaSpriteKind};

use crate::screen::Screen;
use super::characters::{Character, StageBgSprite};
use super::video::VideoPlayer;

// === Psych Engine constants ===
pub const GAME_W: f32 = 1280.0;
pub const GAME_H: f32 = 720.0;
pub(super) const STRUM_Y: f32 = 50.0;
pub(super) const STRUM_Y_DOWN: f32 = 570.0;
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

/// Animated health drain state (e.g. for scythe notes).
/// Health slides from `start` down to `target` over `duration` seconds.
pub(super) struct HealthDrain {
    pub start: f32,
    pub target: f32,
    pub elapsed: f32,
    pub duration: f32,
    pub death_safe: bool,
}

/// Draw order layer — determines what gets drawn when.
/// Built from stage `objects` array or hardcoded fallback.
#[derive(Debug, Clone)]
pub(super) enum DrawLayer {
    StageBg(usize),
    Gf,
    Dad,
    Bf,
}

/// Generic stage overlay: split left/right color tint system.
/// Controlled via setStageColor/swapStageColors Lua API functions.
pub(super) struct StageOverlay {
    pub color_left: [f32; 4],
    pub color_right: [f32; 4],
    pub tween_left_active: bool,
    pub tween_left_start: [f32; 4],
    pub tween_left_target: [f32; 4],
    pub tween_left_elapsed: f32,
    pub tween_left_duration: f32,
    pub tween_right_active: bool,
    pub tween_right_start: [f32; 4],
    pub tween_right_target: [f32; 4],
    pub tween_right_elapsed: f32,
    pub tween_right_duration: f32,
    /// Whether the overlay is enabled (any non-transparent color activates it).
    pub lights_on: bool,
}

impl Default for StageOverlay {
    fn default() -> Self {
        Self {
            color_left: [0.0; 4],
            color_right: [0.0; 4],
            tween_left_active: false,
            tween_left_start: [0.0; 4],
            tween_left_target: [0.0; 4],
            tween_left_elapsed: 0.0,
            tween_left_duration: 1.0,
            tween_right_active: false,
            tween_right_start: [0.0; 4],
            tween_right_target: [0.0; 4],
            tween_right_elapsed: 0.0,
            tween_right_duration: 1.0,
            lights_on: true,
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

pub(super) enum CutsceneState {
    Video {
        player: VideoPlayer,
        skippable: bool,
        wall_clock_ms: f64,
    },
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

    // Health drain (animated slide for harmful notes)
    pub(super) health_drain: Option<HealthDrain>,

    // Lua scripting
    pub(super) scripts: ScriptManager,
    pub(super) lua_textures: HashMap<String, GpuTexture>,
    pub(super) lua_atlases: HashMap<String, SpriteAtlas>,
    pub(super) lua_behind: Vec<String>,  // sprite tags drawn behind characters
    pub(super) lua_front: Vec<String>,   // sprite tags drawn in front of characters
    pub(super) paths: AssetPaths,
    /// Per-note-type custom skin assets, keyed by type string (e.g. "scytheNote").
    pub(super) custom_note_assets: HashMap<String, NoteAssets>,
    /// Pending note skin loads: (note_type_name, skin_path, custom_anims). Processed in draw phase with GPU.
    pub(super) pending_note_skin_loads: Vec<(String, String, Option<[String; 4]>, Option<[String; 4]>, Option<[String; 4]>)>,
    /// Whether the character camera layer is visible (toggled by camCharacters.visible).
    pub(super) cam_characters_visible: bool,
    /// Whether character reflections are drawn (flipY copies below characters).
    pub(super) reflections_enabled: bool,
    /// Reflection alpha (0.35 in V-Slice ReflectShader).
    pub(super) reflection_alpha: f32,
    /// Reflection Y offset from bottom of character (-30 in V-Slice).
    pub(super) reflection_dist_y: f32,
    /// Generic stage overlay: split left/right color tint drawn over the game world.
    /// Controlled via setStageColor() Lua API. Any stage can use this.
    pub(super) stage_overlay: StageOverlay,

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
    pub(super) completed_song: bool,
    pub(super) score_saved: bool,

    /// Active gameplay-blocking cutscene.
    pub(super) cutscene: Option<CutsceneState>,
}

impl PlayScreen {
    pub fn new(song_name: &str, difficulty: &str, play_as_opponent: bool) -> Self {
        let mut game = PlayState::new(100.0);
        game.play_as_opponent = play_as_opponent;
        let prefs = Preferences::load();
        
        Self {
            game,
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
            health_drain: None,
            scripts: ScriptManager::new(),
            lua_textures: HashMap::new(),
            lua_atlases: HashMap::new(),
            lua_behind: Vec::new(),
            lua_front: Vec::new(),
            paths: AssetPaths::platform_default(),
            custom_note_assets: HashMap::new(),
            pending_note_skin_loads: Vec::new(),
            cam_characters_visible: true,
            reflections_enabled: false,
            reflection_alpha: 0.35,
            reflection_dist_y: -30.0,
            stage_overlay: StageOverlay::default(),
            char_change_requests: Vec::new(),
            stage_pos_bf: [0.0; 2],
            stage_pos_dad: [0.0; 2],
            stage_pos_gf: [0.0; 2],
            stage_name: String::new(),
            last_dt: 1.0 / 60.0,
            downscroll: prefs.downscroll,
            paused: false,
            pause_selection: 0,
            skip_target_ms: 0.0,
            wants_restart: false,
            completed_song: false,
            score_saved: false,
            cutscene: None,
        }
    }

    pub(super) fn start_video_cutscene(&mut self, player: VideoPlayer, skippable: bool) {
        if let Some(audio) = &mut self.audio {
            if self.game.song_started {
                audio.pause();
            }
        }
        self.cutscene = Some(CutsceneState::Video { player, skippable, wall_clock_ms: 0.0 });
    }

    pub(super) fn finish_cutscene(&mut self) {
        let Some(CutsceneState::Video { mut player, .. }) = self.cutscene.take() else { return; };
        if let Some(cb) = player.take_on_finish() {
            if self.scripts.has_scripts() {
                self.scripts.call_lua_function(&cb, "");
            }
        }
        if let Some(audio) = &mut self.audio {
            if self.game.song_started && !self.game.song_ended {
                audio.play();
            }
        }
    }

    pub(super) fn skip_cutscene(&mut self) {
        if let Some(CutsceneState::Video { player, .. }) = &mut self.cutscene {
            player.stop();
        }
        self.finish_cutscene();
    }

    pub(super) fn key_to_lane(key: KeyCode) -> Option<usize> {
        for (lane, binds) in LANE_KEYS.iter().enumerate() {
            if binds.contains(&key) {
                return Some(lane);
            }
        }
        None
    }

    pub(super) fn strum_x(lane: usize, player: bool, _play_as_opponent: bool) -> f32 {
        let base = STRUM_X + 50.0 + NOTE_WIDTH * lane as f32;
        if player { base + GAME_W / 2.0 } else { base }
    }

    // === Generic stage overlay methods ===

    /// Tween the left stage overlay color.
    pub(super) fn stage_color_left(&mut self, color: [f32; 4], dur: f32) {
        self.stage_overlay.tween_left_start = self.stage_overlay.color_left;
        self.stage_overlay.tween_left_target = color;
        self.stage_overlay.tween_left_elapsed = 0.0;
        self.stage_overlay.tween_left_duration = dur;
        self.stage_overlay.tween_left_active = true;
    }

    /// Tween the right stage overlay color.
    pub(super) fn stage_color_right(&mut self, color: [f32; 4], dur: f32) {
        self.stage_overlay.tween_right_start = self.stage_overlay.color_right;
        self.stage_overlay.tween_right_target = color;
        self.stage_overlay.tween_right_elapsed = 0.0;
        self.stage_overlay.tween_right_duration = dur;
        self.stage_overlay.tween_right_active = true;
    }

    /// Tween both stage overlay colors to the same target.
    pub(super) fn stage_color_both(&mut self, color: [f32; 4], dur: f32) {
        self.stage_color_left(color, dur);
        self.stage_color_right(color, dur);
    }

    /// Update stage overlay color tweens.
    pub(super) fn update_stage_overlay(&mut self, dt: f32) {
        if self.stage_overlay.tween_left_active {
            self.stage_overlay.tween_left_elapsed += dt;
            let t = (self.stage_overlay.tween_left_elapsed / self.stage_overlay.tween_left_duration).min(1.0);
            for i in 0..4 {
                self.stage_overlay.color_left[i] = self.stage_overlay.tween_left_start[i]
                    + (self.stage_overlay.tween_left_target[i] - self.stage_overlay.tween_left_start[i]) * t;
            }
            if t >= 1.0 { self.stage_overlay.tween_left_active = false; }
        }
        if self.stage_overlay.tween_right_active {
            self.stage_overlay.tween_right_elapsed += dt;
            let t = (self.stage_overlay.tween_right_elapsed / self.stage_overlay.tween_right_duration).min(1.0);
            for i in 0..4 {
                self.stage_overlay.color_right[i] = self.stage_overlay.tween_right_start[i]
                    + (self.stage_overlay.tween_right_target[i] - self.stage_overlay.tween_right_start[i]) * t;
            }
            if t >= 1.0 { self.stage_overlay.tween_right_active = false; }
        }
    }

    /// Process Lua extension requests (stage color, post-processing, health bar, etc.).
    pub(super) fn process_lua_extensions(&mut self) {
        let srgb = |s: f32| -> f32 {
            if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };

        // Stage color requests (collect first to avoid borrow conflict)
        let color_reqs: Vec<_> = self.scripts.state.stage_color_requests.drain(..).collect();
        for (side, r, g, b, a, dur) in color_reqs {
            let color = [srgb(r), srgb(g), srgb(b), a];
            match side.as_str() {
                "left" => self.stage_color_left(color, dur),
                "right" => self.stage_color_right(color, dur),
                _ => self.stage_color_both(color, dur),
            }
        }

        // Stage color swap requests
        let swap_reqs: Vec<_> = self.scripts.state.stage_color_swap_requests.drain(..).collect();
        for dur in swap_reqs {
            let old_left = self.stage_overlay.color_left;
            let old_right = self.stage_overlay.color_right;
            self.stage_color_left(old_right, dur);
            self.stage_color_right(old_left, dur);
        }

        // Stage lights toggle
        if let Some(on) = self.scripts.state.stage_lights_request.take() {
            self.stage_overlay.lights_on = on;
        }

        // Reflections toggle
        if let Some(enabled) = self.scripts.state.reflections_request.take() {
            self.reflections_enabled = enabled;
        }

        // Custom health bar color requests
        // Accumulate targets so multiple requests in the same frame don't overwrite each other
        let hb_reqs: Vec<_> = self.scripts.state.healthbar_color_requests.drain(..).collect();
        if !hb_reqs.is_empty() {
            if let Some(chb) = &mut self.custom_healthbar {
                let mut new_left: Option<[f32; 4]> = None;
                let mut new_right: Option<[f32; 4]> = None;
                let mut dur = 1.0f32;
                for (side, r, g, b, a, d) in hb_reqs {
                    let color = [srgb(r), srgb(g), srgb(b), a];
                    dur = d;
                    match side.as_str() {
                        "left" => new_left = Some(color),
                        "right" => new_right = Some(color),
                        _ => { new_left = Some(color); new_right = Some(color); }
                    }
                }
                let left = new_left.unwrap_or(chb.left_color);
                let right = new_right.unwrap_or(chb.right_color);
                chb.tween_colors(left, Some(right), dur);
            }
        }

        // Post-processing requests
        // postprocess_requests (enable/disable) is handled in draw where gpu is available
        // postprocess_param_requests (individual params) is also handled in draw
    }

    /// Whether a given strum lane is in downscroll mode.
    /// Per-strum `down_scroll` overrides the global `self.downscroll`.
    pub(super) fn is_strum_downscroll(&self, lane: usize, player: bool) -> bool {
        let idx = if player { lane + 4 } else { lane };
        let sp = &self.scripts.state.strum_props[idx];
        sp.down_scroll.unwrap_or(self.downscroll)
    }

    /// Get strum position/alpha/angle/scale from modchart state. Falls back to defaults.
    /// Returns (x, y, alpha, angle_degrees, scale).
    pub(super) fn strum_pos(&self, lane: usize, player: bool) -> (f32, f32, f32, f32, f32) {
        let idx = if player { lane + 4 } else { lane };
        let sp = &self.scripts.state.strum_props[idx];
        if sp.custom {
            (sp.x, sp.y, sp.alpha, sp.angle, sp.scale_x)
        } else {
            let default_y = if self.is_strum_downscroll(lane, player) { STRUM_Y_DOWN } else { STRUM_Y };
            (Self::strum_x(lane, player, self.game.play_as_opponent), default_y, 1.0, 0.0, NOTE_SCALE)
        }
    }

    /// Load a custom note skin (PNG + XML) and register standard animations.
    pub(super) fn load_note_skin(
        &self,
        gpu: &rustic_render::gpu::GpuState,
        paths: &rustic_core::paths::AssetPaths,
        skin_path: &str,
        custom_note_anims: Option<&[String; 4]>,
        custom_strum_anims: Option<&[String; 4]>,
        custom_confirm_anims: Option<&[String; 4]>,
    ) -> Option<NoteAssets> {
        let png = paths.image(skin_path)?;
        let xml_path = paths.image_xml(skin_path)?;
        let xml_str = std::fs::read_to_string(&xml_path).ok()?;
        let tex = gpu.load_texture_from_path(&png);
        let mut atlas = rustic_render::sprites::SpriteAtlas::from_xml(&xml_str);

        // Try standard Psych Engine naming first (purple0, arrowLEFT, left confirm, etc.)
        for (anim, prefix) in NOTE_ANIMS.iter().zip(NOTE_PREFIXES.iter()) {
            atlas.add_by_prefix(anim, prefix);
        }
        for prefix in STRUM_ANIMS.iter().chain(PRESS_ANIMS.iter())
            .chain(CONFIRM_ANIMS.iter()).chain(HOLD_PIECE_ANIMS.iter())
            .chain(HOLD_END_ANIMS.iter())
        {
            atlas.add_by_prefix(prefix, prefix);
        }

        // If standard naming didn't find scroll notes, try direction-based naming
        // (used by VS Retrospecter custom note skins: Left, Down, Up, Right, static Left, confirm Left, etc.)
        let dir_names = ["Left", "Down", "Up", "Right"];
        if atlas.get_frame(NOTE_ANIMS[0], 0).is_none() {
            for (i, dir) in dir_names.iter().enumerate() {
                atlas.add_by_prefix(NOTE_ANIMS[i], dir);
            }
        }
        // Direction-based strum names: "static Left" etc.
        if atlas.get_frame(STRUM_ANIMS[0], 0).is_none() {
            for (i, dir) in dir_names.iter().enumerate() {
                atlas.add_by_prefix(STRUM_ANIMS[i], &format!("static {}", dir));
            }
        }
        // Direction-based confirm names: "confirm Left" etc.
        if atlas.get_frame(CONFIRM_ANIMS[0], 0).is_none() {
            for (i, dir) in dir_names.iter().enumerate() {
                atlas.add_by_prefix(CONFIRM_ANIMS[i], &format!("confirm {}", dir));
            }
        }
        // Direction-based press names: "press Left" etc.
        if atlas.get_frame(PRESS_ANIMS[0], 0).is_none() {
            for (i, dir) in dir_names.iter().enumerate() {
                atlas.add_by_prefix(PRESS_ANIMS[i], &format!("press {}", dir));
            }
        }
        // Shared hold pieces: "hold_piece" / "hold_end" (not per-lane)
        if atlas.get_frame(HOLD_PIECE_ANIMS[0], 0).is_none() {
            for i in 0..4 {
                atlas.add_by_prefix(HOLD_PIECE_ANIMS[i], "hold_piece");
                atlas.add_by_prefix(HOLD_END_ANIMS[i], "hold_end");
            }
        }
        // Fix known atlas typos: VS Retrospecter has "pruple end hold" instead of "purple hold end"
        if atlas.get_frame(HOLD_END_ANIMS[0], 0).is_none() {
            atlas.add_by_prefix(HOLD_END_ANIMS[0], "pruple end hold");
        }

        // Register custom animation names from Lua registerNoteType (e.g. "Scythe_Note_Left")
        if let Some(anims) = custom_note_anims {
            for anim in anims {
                atlas.add_by_prefix(anim, anim);
            }
        }
        if let Some(anims) = custom_strum_anims {
            for anim in anims {
                atlas.add_by_prefix(anim, anim);
            }
        }
        if let Some(anims) = custom_confirm_anims {
            for anim in anims {
                atlas.add_by_prefix(anim, anim);
            }
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

        // Process pending note type registrations from Lua scripts
        let note_regs: Vec<_> = self.scripts.state.note_type_registrations.drain(..).collect();
        for reg in note_regs {
            // Queue skin load if a custom skin path was specified
            if let Some(ref skin) = reg.note_skin {
                if !skin.is_empty() && !self.custom_note_assets.contains_key(&reg.name) {
                    self.pending_note_skin_loads.push((
                        reg.name.clone(), skin.clone(),
                        reg.note_anims.clone(), reg.strum_anims.clone(), reg.confirm_anims.clone(),
                    ));
                }
            }
            rustic_core::note::register_note_type(&reg.name, rustic_core::note::NoteTypeConfig {
                hit_causes_miss: reg.hit_causes_miss,
                hit_damage: reg.hit_damage,
                ignore_miss: reg.ignore_miss,
                note_skin: reg.note_skin,
                note_anims: reg.note_anims,
                strum_anims: reg.strum_anims,
                confirm_anims: reg.confirm_anims,
                hit_sfx: reg.hit_sfx,
                health_drain_pct: reg.health_drain_pct,
                drain_death_safe: reg.drain_death_safe,
            });
            log::info!("Registered note type '{}' from Lua", reg.name);
        }

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
                        if sprite.antialiasing {
                            gpu.load_texture_from_path(&png)
                        } else {
                            gpu.load_texture_from_path_nearest(&png)
                        }
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
                        if sprite.antialiasing {
                            gpu.load_texture_from_path(&png)
                        } else {
                            gpu.load_texture_from_path_nearest(&png)
                        }
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

    fn handle_touch(&mut self, _id: u64, phase: TouchPhase, x: f64, y: f64) {
        let (x, y) = (x as f32, y as f32);

        if let Some(CutsceneState::Video { skippable, .. }) = &self.cutscene {
            if *skippable && phase == TouchPhase::Started {
                self.skip_cutscene();
            }
            return;
        }

        // Pause menu / death screen
        if self.paused || self.death.is_some() {
            if phase == TouchPhase::Started {
                if y < GAME_H * 0.33 {
                    self.handle_key_inner(KeyCode::ArrowUp);
                } else if y > GAME_H * 0.67 {
                    self.handle_key_inner(KeyCode::ArrowDown);
                } else {
                    self.handle_key_inner(KeyCode::Enter);
                }
            }
            return;
        }

        // Pause: top center strip (top 12%, middle 50% of width)
        if phase == TouchPhase::Started
            && y < GAME_H * 0.12
            && x > GAME_W * 0.25
            && x < GAME_W * 0.75
        {
            self.handle_key_inner(KeyCode::Escape);
            return;
        }

        // Gameplay: full screen divided into 4 equal lane columns
        let lane = ((x / GAME_W) * 4.0) as usize;
        let lane = lane.min(3);
        match phase {
            TouchPhase::Started => self.game.key_press(lane),
            TouchPhase::Ended | TouchPhase::Cancelled => self.game.key_release(lane),
            _ => {}
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
            Some(Box::new(PlayScreen::new(&self.song_name, &self.difficulty, self.game.play_as_opponent)))
        } else if self.game.song_ended {
            if self.completed_song && !self.score_saved {
                let mut store = HighscoreStore::load();
                store.save_score(
                    &self.song_name,
                    &self.difficulty,
                    self.game.score.score,
                    self.game.score.accuracy() as f32,
                    self.game.score.misses == 0,
                );
                store.save();
                self.score_saved = true;
            }
            self.game.song_ended = false;
            Some(Box::new(super::freeplay::FreeplayScreen::new()))
        } else {
            None
        }
    }
}
