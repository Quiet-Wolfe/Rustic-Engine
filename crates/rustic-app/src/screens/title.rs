use winit::event::TouchPhase;
use winit::keyboard::KeyCode;

use rustic_audio::AudioEngine;
use rustic_core::paths::AssetPaths;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::{AnimationController, SpriteAtlas};

use crate::screen::Screen;
use super::main_menu::MainMenuScreen;

const GAME_W: f32 = 1280.0;
const GAME_H: f32 = 720.0;

pub struct TitleScreen {
    audio: Option<AudioEngine>,

    // Logo
    logo_tex: Option<GpuTexture>,
    logo_atlas: SpriteAtlas,
    logo_anim: AnimationController,

    // GF
    gf_tex: Option<GpuTexture>,
    gf_atlas: SpriteAtlas,
    gf_anim: AnimationController,
    /// Matches Psych Engine's `danceLeft` variable (starts false).
    /// When true → plays 'danceRight', when false → plays 'danceLeft'.
    dance_left: bool,

    // Title Enter text
    enter_tex: Option<GpuTexture>,
    enter_atlas: SpriteAtlas,
    enter_anim: AnimationController,

    // Positions (from gfDanceTitle.json)
    title_x: f32,
    title_y: f32,
    gf_x: f32,
    gf_y: f32,
    start_x: f32,
    start_y: f32,

    // Timing
    bpm: f64,
    song_position: f64,
    last_beat: i32,
    /// Psych Engine's sickBeats — counts actual beat events, never skipped.
    sick_beats: i32,

    // Intro sequence
    skipped_intro: bool,
    intro_texts: Vec<String>,

    // Interactive state
    confirmed: bool,
    confirm_timer: f32,
    /// 0→2 cycling timer for "Press Enter" color animation.
    title_timer: f32,
    /// White flash alpha (fades from 1.0 to 0 over ~4 seconds like Psych's camera.flash(WHITE, 4)).
    flash_alpha: f32,

    // Transition
    next: Option<Box<dyn Screen>>,
}

impl TitleScreen {
    pub fn new() -> Self {
        Self {
            audio: None,
            logo_tex: None,
            logo_atlas: SpriteAtlas::from_xml(""),
            logo_anim: AnimationController::new(),
            gf_tex: None,
            gf_atlas: SpriteAtlas::from_xml(""),
            gf_anim: AnimationController::new(),
            dance_left: false, // Matches Psych Engine default
            enter_tex: None,
            enter_atlas: SpriteAtlas::from_xml(""),
            enter_anim: AnimationController::new(),
            title_x: -150.0,
            title_y: -100.0,
            gf_x: 512.0,
            gf_y: 40.0,
            start_x: 100.0,
            start_y: 576.0,
            bpm: 102.0,
            song_position: 0.0,
            last_beat: -1,
            sick_beats: 0,
            skipped_intro: false,
            intro_texts: Vec::new(),
            confirmed: false,
            confirm_timer: 0.0,
            title_timer: 0.0,
            flash_alpha: 0.0,
            next: None,
        }
    }

    fn beat_hit(&mut self) {
        // Logo bump every beat (force restart like Psych's play('bump', true))
        self.logo_anim.force_play("bump", 24.0, false);

        // GF dance — Psych Engine toggles danceLeft first, then:
        //   if(danceLeft) play('danceRight') else play('danceLeft')
        self.dance_left = !self.dance_left;
        if self.dance_left {
            self.gf_anim.play("danceRight", 24.0, false);
        } else {
            self.gf_anim.play("danceLeft", 24.0, false);
        }

        // Intro sequence — matches Psych Engine's sickBeats switch exactly
        self.sick_beats += 1;
        if !self.skipped_intro {
            match self.sick_beats {
                1 => {
                    // Music already playing from init
                }
                2 => {
                    self.intro_texts = vec!["Psych Engine by".into()];
                }
                4 => {
                    self.intro_texts.push("Shadow Mario".into());
                    self.intro_texts.push("Riveren".into());
                }
                5 => { self.intro_texts.clear(); }
                6 => {
                    self.intro_texts = vec!["Not associated".into(), "with".into()];
                }
                8 => {
                    self.intro_texts.push("newgrounds".into());
                }
                9 => { self.intro_texts.clear(); }
                10 => {
                    // Random wacky message — use a fixed one for now
                    self.intro_texts = vec!["this is a god damn prototype".into()];
                }
                12 => {
                    self.intro_texts.push("we workin on it okay".into());
                }
                13 => { self.intro_texts.clear(); }
                14 => {
                    self.intro_texts = vec!["Friday".into()];
                }
                15 => {
                    self.intro_texts.push("Night".into());
                }
                16 => {
                    self.intro_texts.push("Funkin".into());
                }
                17 => {
                    self.skip_intro();
                }
                _ => {}
            }
        }
    }

    fn skip_intro(&mut self) {
        if !self.skipped_intro {
            self.skipped_intro = true;
            self.intro_texts.clear();
            self.flash_alpha = 1.0; // camera.flash(WHITE, 4)
        }
    }
}

impl Screen for TitleScreen {
    fn init(&mut self, gpu: &GpuState) {
        let paths = AssetPaths::platform_default();

        // Parse GF config JSON for positions and BPM
        if let Some(gf_config_path) = paths.find("images/gfDanceTitle.json") {
            if let Ok(json_str) = std::fs::read_to_string(&gf_config_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    self.title_x = val["titlex"].as_f64().unwrap_or(-150.0) as f32;
                    self.title_y = val["titley"].as_f64().unwrap_or(-100.0) as f32;
                    self.gf_x = val["gfx"].as_f64().unwrap_or(512.0) as f32;
                    self.gf_y = val["gfy"].as_f64().unwrap_or(40.0) as f32;
                    self.start_x = val["startx"].as_f64().unwrap_or(100.0) as f32;
                    self.start_y = val["starty"].as_f64().unwrap_or(576.0) as f32;
                    self.bpm = val["bpm"].as_f64().unwrap_or(102.0);

                    // GF dance animations from config indices
                    if let Some(gf_xml_path) = paths.image_xml("gfDanceTitle") {
                        let gf_xml = std::fs::read_to_string(gf_xml_path).unwrap_or_default();
                        self.gf_atlas = SpriteAtlas::from_xml(&gf_xml);
                    }

                    let prefix = val["animation"].as_str().unwrap_or("gfDance");
                    let parse_indices = |arr: &serde_json::Value| -> Vec<i32> {
                        arr.as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_i64().map(|n| n as i32))
                                    .collect::<Vec<i32>>()
                            })
                            .unwrap_or_default()
                    };
                    let left_indices = parse_indices(&val["dance_left"]);
                    let right_indices = parse_indices(&val["dance_right"]);
                    self.gf_atlas.add_by_indices("danceLeft", prefix, &left_indices);
                    self.gf_atlas.add_by_indices("danceRight", prefix, &right_indices);
                }
            }
        }
        if let Some(gf_tex_path) = paths.image("gfDanceTitle") {
            self.gf_tex = Some(gpu.load_texture_from_path(&gf_tex_path));
        }

        // Logo
        if let Some(logo_xml_path) = paths.image_xml("logoBumpin") {
            let logo_xml = std::fs::read_to_string(logo_xml_path).unwrap_or_default();
            self.logo_atlas = SpriteAtlas::from_xml(&logo_xml);
        }
        self.logo_atlas.add_by_prefix("bump", "logo bumpin");
        if let Some(logo_tex_path) = paths.image("logoBumpin") {
            self.logo_tex = Some(gpu.load_texture_from_path(&logo_tex_path));
        }

        // Title Enter text
        if let Some(enter_xml_path) = paths.image_xml("titleEnter") {
            let enter_xml = std::fs::read_to_string(enter_xml_path).unwrap_or_default();
            self.enter_atlas = SpriteAtlas::from_xml(&enter_xml);
        }
        self.enter_atlas.add_by_prefix("idle", "ENTER IDLE");
        self.enter_atlas.add_by_prefix("press", "ENTER PRESSED");
        if let Some(enter_tex_path) = paths.image("titleEnter") {
            self.enter_tex = Some(gpu.load_texture_from_path(&enter_tex_path));
        }
        self.enter_anim.play("idle", 24.0, true);

        // Audio — play freakyMenu at 0.7 volume (skip if already passed from previous screen)
        if self.audio.is_none() {
            if let Some(music) = paths.music("freakyMenu") {
                let mut audio = AudioEngine::new();
                audio.play_loop_music_vol(&music, 0.7);
                self.audio = Some(audio);
            }
        }

        // Initial animations — match Psych Engine's startIntro()
        self.logo_anim.play("bump", 24.0, false);
        // Psych: gfDance.animation.play('danceRight') is the initial anim
        self.gf_anim.play("danceRight", 24.0, false);
    }

    fn handle_key(&mut self, key: KeyCode) {
        if key == KeyCode::Enter || key == KeyCode::Space {
            if !self.skipped_intro {
                self.skip_intro();
                return;
            }
            if !self.confirmed {
                self.confirmed = true;
                self.enter_anim.play("press", 24.0, false);
                if let Some(audio) = &mut self.audio {
                    let paths = AssetPaths::platform_default();
                    if let Some(sfx) = paths.sound("confirmMenu") {
                        audio.play_sound(&sfx, 0.7);
                    }
                }
            }
        }
    }

    fn handle_touch(&mut self, _id: u64, phase: TouchPhase, _x: f64, _y: f64) {
        if phase == TouchPhase::Started {
            self.handle_key(KeyCode::Enter);
        }
    }

    fn update(&mut self, dt: f32) {
        let dt_ms = dt as f64 * 1000.0;
        self.song_position += dt_ms;

        // Beat detection
        let crochet = 60000.0 / self.bpm;
        let cur_beat = (self.song_position / crochet).floor() as i32;
        if cur_beat > self.last_beat {
            for _ in (self.last_beat + 1)..=cur_beat {
                self.beat_hit();
            }
            self.last_beat = cur_beat;
        }

        // Update animations
        let logo_fc = self.logo_atlas.frame_count("bump");
        self.logo_anim.update(dt, logo_fc);

        let gf_anim_name = self.gf_anim.current_anim.clone();
        let gf_fc = self.gf_atlas.frame_count(&gf_anim_name);
        self.gf_anim.update(dt, gf_fc);

        let enter_anim_name = self.enter_anim.current_anim.clone();
        let enter_fc = self.enter_atlas.frame_count(&enter_anim_name);
        self.enter_anim.update(dt, enter_fc);

        // "Press Enter" color cycling — 2-second period, bouncing 0→1→0
        // Matches Psych Engine: titleTimer += bound(elapsed, 0, 1); if > 2 then -= 2;
        if !self.confirmed {
            self.title_timer += dt.min(1.0);
            if self.title_timer > 2.0 {
                self.title_timer -= 2.0;
            }
        }

        // Fade white flash (Psych does camera.flash(WHITE, 4) but that's too slow — use ~1.5s)
        if self.flash_alpha > 0.0 {
            self.flash_alpha = (self.flash_alpha - dt / 1.5).max(0.0);
        }

        // Confirm transition — 1 second delay like Psych Engine
        if self.confirmed {
            self.confirm_timer += dt;
            if self.confirm_timer >= 1.0 {
                self.next = Some(Box::new(MainMenuScreen::new()));
            }
        }
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        if !gpu.begin_frame() { return; }

        if !self.skipped_intro {
            // Intro: black background with centered text
            gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.0, 1.0]);
            gpu.draw_batch(None);

            // Psych uses Alphabet at y = (i * 60) + 200
            let text_size = 32.0;
            let line_h = 60.0;
            for (i, text) in self.intro_texts.iter().enumerate() {
                let text_w = text.len() as f32 * text_size * 0.5;
                let x = (GAME_W - text_w) / 2.0;
                let y = 200.0 + i as f32 * line_h;
                gpu.draw_text(text, x, y, text_size, [1.0, 1.0, 1.0, 1.0]);
            }
        } else {
            // Interactive state: GF, logo, press enter

            // GF sprite
            if let (Some(tex), Some(frame)) = (
                &self.gf_tex,
                self.gf_atlas.get_frame(&self.gf_anim.current_anim, self.gf_anim.frame_index),
            ) {
                gpu.draw_sprite_frame(
                    frame, tex.width as f32, tex.height as f32,
                    self.gf_x, self.gf_y, 1.0, false,
                    [1.0, 1.0, 1.0, 1.0],
                );
                gpu.draw_batch(Some(tex));
            }

            // Logo sprite
            if let (Some(tex), Some(frame)) = (
                &self.logo_tex,
                self.logo_atlas.get_frame("bump", self.logo_anim.frame_index),
            ) {
                gpu.draw_sprite_frame(
                    frame, tex.width as f32, tex.height as f32,
                    self.title_x, self.title_y, 1.0, false,
                    [1.0, 1.0, 1.0, 1.0],
                );
                gpu.draw_batch(Some(tex));
            }

            // "Press Enter" text with color cycling
            if let (Some(tex), Some(frame)) = (
                &self.enter_tex,
                self.enter_atlas.get_frame(&self.enter_anim.current_anim, self.enter_anim.frame_index),
            ) {
                // Psych Engine color cycling:
                // timer bounces: if >= 1 then timer = (-timer) + 2, so 0→1→0
                // Then quadInOut ease is applied
                let mut timer = self.title_timer;
                if timer >= 1.0 { timer = -timer + 2.0; }
                // quadInOut: t < 0.5 → 2t², else → 1 - (-2t+2)²/2
                let ease = if timer < 0.5 {
                    2.0 * timer * timer
                } else {
                    1.0 - (-2.0 * timer + 2.0).powi(2) / 2.0
                };

                // Interpolate: 0xFF33FFFF (cyan) → 0xFF3333CC (dark blue)
                let r = 0.2 + ease * (0.2 - 0.2);   // 0x33→0x33 = 0.2→0.2
                let g = 1.0 + ease * (0.2 - 1.0);    // 0xFF→0x33 = 1.0→0.2
                let b = 1.0 + ease * (0.8 - 1.0);    // 0xFF→0xCC = 1.0→0.8
                let a = 1.0 + ease * (0.64 - 1.0);   // 1.0→0.64

                // Premultiply alpha for the shader
                let color = [r * a, g * a, b * a, a];

                // Position: use startx/starty from JSON
                let enter_x = self.start_x;
                let enter_y = self.start_y;

                gpu.draw_sprite_frame(
                    frame, tex.width as f32, tex.height as f32,
                    enter_x, enter_y, 1.0, false,
                    color,
                );
                gpu.draw_batch(Some(tex));
            }
        }

        // White flash overlay (fades out after intro skip)
        if self.flash_alpha > 0.0 {
            gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [self.flash_alpha; 4]);
            gpu.draw_batch(None);
        }

        crate::debug_overlay::finish_frame(gpu);
    }

    fn next_screen(&mut self) -> Option<Box<dyn Screen>> {
        self.next.take()
    }

    fn take_audio(&mut self) -> Option<AudioEngine> {
        self.audio.take()
    }

    fn set_audio(&mut self, audio: AudioEngine) {
        self.audio = Some(audio);
    }
}
