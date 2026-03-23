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
use super::characters::{CharacterSprite, StageBgSprite};

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

/// Death screen state (visual layer).
pub(super) struct DeathState {
    pub character: CharacterSprite,
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
    pub(super) note_assets: Option<NoteAssets>,
    pub(super) rating_assets: Option<RatingAssets>,
    pub(super) splash_atlas: Option<NoteAssets>,
    pub(super) icon_bf: Option<GpuTexture>,
    pub(super) icon_dad: Option<GpuTexture>,
    pub(super) countdown_ready: Option<GpuTexture>,
    pub(super) countdown_set: Option<GpuTexture>,
    pub(super) countdown_go: Option<GpuTexture>,

    // Characters & Stage
    pub(super) char_bf: Option<CharacterSprite>,
    pub(super) char_dad: Option<CharacterSprite>,
    pub(super) char_gf: Option<CharacterSprite>,
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

    // Death
    pub(super) death: Option<DeathState>,
    pub(super) death_char_preloaded: Option<CharacterSprite>,

    // Lua scripting
    pub(super) scripts: ScriptManager,
    pub(super) lua_textures: HashMap<String, GpuTexture>,
    pub(super) lua_atlases: HashMap<String, SpriteAtlas>,
    pub(super) lua_behind: Vec<String>,  // sprite tags drawn behind characters
    pub(super) lua_front: Vec<String>,   // sprite tags drawn in front of characters
    pub(super) paths: AssetPaths,

    // Pause
    pub(super) paused: bool,
    pub(super) pause_selection: usize,
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
            rating_assets: None,
            splash_atlas: None,
            icon_bf: None,
            icon_dad: None,
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
            death: None,
            death_char_preloaded: None,
            scripts: ScriptManager::new(),
            lua_textures: HashMap::new(),
            lua_atlases: HashMap::new(),
            lua_behind: Vec::new(),
            lua_front: Vec::new(),
            paths: AssetPaths::psych_default(),
            paused: false,
            pause_selection: 0,
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

    /// Get strum position/alpha from modchart state. Falls back to defaults.
    pub(super) fn strum_pos(&self, lane: usize, player: bool) -> (f32, f32, f32) {
        let idx = if player { lane + 4 } else { lane };
        let sp = &self.scripts.state.strum_props[idx];
        if sp.custom {
            (sp.x, sp.y, sp.alpha)
        } else {
            (Self::strum_x(lane, player), STRUM_Y, 1.0)
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
                _ => {
                    log::debug!("Unhandled property write: {} = {:?}", prop, val);
                }
            }
        }
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
