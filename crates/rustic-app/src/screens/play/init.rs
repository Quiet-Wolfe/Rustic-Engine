use rustic_audio::AudioEngine;
use rustic_core::character::CharacterFile;
use rustic_core::chart;
use rustic_core::paths::AssetPaths;
use rustic_core::stage::StageFile;
use rustic_gameplay::play_state::{PlayState, SectionInfo};
use rustic_render::gpu::GpuState;
use rustic_render::sprites::SpriteAtlas;

use super::{
    PlayScreen, NoteAssets, RatingAssets, DrawLayer, STRUM_Y,
    NOTE_ANIMS, NOTE_PREFIXES, STRUM_ANIMS, PRESS_ANIMS, CONFIRM_ANIMS,
    HOLD_PIECE_ANIMS, HOLD_END_ANIMS, SPLASH_PREFIXES,
};
use super::super::characters::{CharacterSprite, StageBgSprite};

impl PlayScreen {
    pub(super) fn init_inner(&mut self, gpu: &GpuState) {
        let paths = AssetPaths::psych_default();

        // Load note atlas
        let note_png = paths.image("noteSkins/NOTE_assets")
            .expect("NOTE_assets.png not found");
        let note_xml_path = paths.image_xml("noteSkins/NOTE_assets")
            .expect("NOTE_assets.xml not found");
        let note_xml = std::fs::read_to_string(&note_xml_path)
            .expect("Failed to read NOTE_assets.xml");

        let note_tex = gpu.load_texture_from_path(&note_png);
        let mut atlas = SpriteAtlas::from_xml(&note_xml);

        for (anim, prefix) in NOTE_ANIMS.iter().zip(NOTE_PREFIXES.iter()) {
            atlas.add_by_prefix(anim, prefix);
        }
        for prefix in STRUM_ANIMS.iter().chain(PRESS_ANIMS.iter())
            .chain(CONFIRM_ANIMS.iter()).chain(HOLD_PIECE_ANIMS.iter())
            .chain(HOLD_END_ANIMS.iter())
        {
            atlas.add_by_prefix(prefix, prefix);
        }

        self.note_assets = Some(NoteAssets {
            tex_w: note_tex.width as f32,
            tex_h: note_tex.height as f32,
            texture: note_tex,
            atlas,
        });

        // Load note splash atlas
        if let (Some(png), Some(xml_path)) = (
            paths.image("noteSplashes/noteSplashes"),
            paths.image_xml("noteSplashes/noteSplashes"),
        ) {
            let splash_xml = std::fs::read_to_string(&xml_path).unwrap();
            let splash_tex = gpu.load_texture_from_path(&png);
            let mut splash_atlas = SpriteAtlas::from_xml(&splash_xml);
            for prefix in SPLASH_PREFIXES {
                splash_atlas.add_by_prefix(prefix, prefix);
            }
            self.splash_atlas = Some(NoteAssets {
                tex_w: splash_tex.width as f32,
                tex_h: splash_tex.height as f32,
                texture: splash_tex,
                atlas: splash_atlas,
            });
        }

        // Load rating/combo sprites
        let load_rating_tex = |name: &str| -> Option<_> {
            let p = paths.image(name)?;
            Some(gpu.load_texture_from_path(&p))
        };
        if let (Some(sick), Some(good), Some(bad), Some(shit)) = (
            load_rating_tex("sick"), load_rating_tex("good"),
            load_rating_tex("bad"), load_rating_tex("shit"),
        ) {
            let nums = std::array::from_fn(|i| {
                load_rating_tex(&format!("num{i}")).expect("Missing combo number sprite")
            });
            self.rating_assets = Some(RatingAssets { sick, good, bad, shit, nums });
        }

        // Load countdown sprites
        self.countdown_ready = paths.image("ready").map(|p| gpu.load_texture_from_path(&p));
        self.countdown_set = paths.image("set").map(|p| gpu.load_texture_from_path(&p));
        self.countdown_go = paths.image("go").map(|p| gpu.load_texture_from_path(&p));

        // Load chart
        let chart_file = paths.chart(&self.song_name, &self.difficulty)
            .unwrap_or_else(|| panic!("Chart not found: {} ({})", self.song_name, self.difficulty));

        let chart_json = std::fs::read_to_string(&chart_file)
            .unwrap_or_else(|e| panic!("Failed to read chart {:?}: {}", chart_file, e));
        let parsed = chart::parse_chart(&chart_json).expect("Failed to parse chart");

        // Initialize PlayState with chart data
        self.game = PlayState::new(parsed.song.bpm);
        self.game.song_speed = parsed.song.speed;

        let sections: Vec<(bool, f64, f64)> = parsed.song.notes.iter()
            .map(|s| {
                let bpm = if s.change_bpm && s.bpm > 0.0 { s.bpm } else { parsed.song.bpm };
                (s.change_bpm, bpm, s.section_beats)
            })
            .collect();
        self.game.conductor.map_bpm_changes(parsed.song.bpm, sections);

        let mut notes = parsed.notes;
        notes.sort_by(|a, b| a.strum_time.partial_cmp(&b.strum_time).unwrap());
        self.game.notes = notes;

        // Build section timing data
        {
            let mut section_time = 0.0;
            let mut cur_bpm = parsed.song.bpm;
            for s in &parsed.song.notes {
                if s.change_bpm && s.bpm > 0.0 { cur_bpm = s.bpm; }
                self.game.sections.push(SectionInfo {
                    must_hit: s.must_hit_section,
                    start_time: section_time,
                });
                let step_crochet = ((60.0 / cur_bpm) * 1000.0) / 4.0;
                section_time += step_crochet * s.section_beats * 4.0;
            }
        }

        // Load stage
        let stage_name = &parsed.song.stage;
        let stage = if let Some(p) = paths.stage_json(stage_name) {
            let json_str = std::fs::read_to_string(&p).unwrap();
            StageFile::from_json(&json_str).unwrap_or_else(|_| StageFile::default_stage())
        } else {
            StageFile::default_stage()
        };

        self.default_cam_zoom = stage.default_zoom as f32;
        self.camera = rustic_render::camera::GameCamera::new(self.default_cam_zoom);
        self.camera.camera_speed = stage.camera_speed as f32;

        // Load stage background sprites — data-driven from objects array,
        // with hardcoded fallback for legacy stages without objects.
        let load_stage_sprite = |gpu: &GpuState, paths: &AssetPaths, image: &str, stage_dir: &str, x: f32, y: f32, scale: f32, scroll_x: f32, scroll_y: f32, flip_x: bool| -> Option<StageBgSprite> {
            let png = paths.stage_image(image, stage_dir)?;
            let tex = gpu.load_texture_from_path(&png);
            Some(StageBgSprite::new(tex, x, y, scale, scroll_x, scroll_y, flip_x))
        };

        let stage_dir = &stage.directory;

        if !stage.objects.is_empty() {
            // Data-driven: load sprites and character markers from objects array
            for obj in &stage.objects {
                match obj.obj_type.as_str() {
                    "gf" | "gfGroup" => self.draw_order.push(DrawLayer::Gf),
                    "dad" | "dadGroup" => self.draw_order.push(DrawLayer::Dad),
                    "boyfriend" | "boyfriendGroup" => self.draw_order.push(DrawLayer::Bf),
                    "sprite" | "animatedSprite" => {
                        if let Some(bg) = load_stage_sprite(
                            gpu, &paths, &obj.image, stage_dir,
                            obj.x as f32, obj.y as f32,
                            obj.scale[0] as f32,
                            obj.scroll[0] as f32, obj.scroll[1] as f32,
                            obj.flip_x,
                        ) {
                            let idx = self.stage_bg.len();
                            self.stage_bg.push(bg);
                            self.draw_order.push(DrawLayer::StageBg(idx));
                        }
                    }
                    _ => {}
                }
            }
        } else {
            // Legacy fallback: hardcoded sprites for known stages
            let hardcoded: &[(&str, f32, f32, f32, f32, f32)] = match stage_name.as_str() {
                "stage" | "" => &[
                    ("stageback",    -600.0, -200.0, 1.0, 0.9, 0.9),
                    ("stagefront",   -650.0,  600.0, 1.1, 0.9, 0.9),
                    ("stagecurtains",-500.0, -300.0, 0.9, 1.3, 1.3),
                ],
                _ => &[],
            };
            for &(image, x, y, scale, sx, sy) in hardcoded {
                if let Some(bg) = load_stage_sprite(gpu, &paths, image, stage_dir, x, y, scale, sx, sy, false) {
                    let idx = self.stage_bg.len();
                    self.stage_bg.push(bg);
                    self.draw_order.push(DrawLayer::StageBg(idx));
                }
            }
            // Default character order for legacy stages
            self.draw_order.push(DrawLayer::Gf);
            self.draw_order.push(DrawLayer::Dad);
            self.draw_order.push(DrawLayer::Bf);
        }

        // If no draw order was established (e.g., Lua-only stage), use default
        if self.draw_order.is_empty() {
            self.draw_order.push(DrawLayer::Gf);
            self.draw_order.push(DrawLayer::Dad);
            self.draw_order.push(DrawLayer::Bf);
        }

        // Load Lua scripts
        self.scripts.set_image_roots(paths.roots().to_vec());
        self.scripts.set_globals(&self.song_name, false);

        // Stage Lua script (loaded first — builds stage visuals)
        if let Some(lua_path) = paths.stage_lua(stage_name) {
            self.scripts.load_script(&lua_path);
        }

        // Song Lua scripts (script.lua, modchart.lua, etc.)
        for script_path in paths.song_scripts(&self.song_name) {
            self.scripts.load_script(&script_path);
        }

        // Call onCreate on all loaded scripts
        if self.scripts.has_scripts() {
            self.scripts.call("onCreate");
            self.process_lua_sprites(gpu);
            self.process_property_writes();
        }

        // Helper: parse character JSON and extract metadata
        let parse_char = |paths: &AssetPaths, name: &str| -> Option<(std::path::PathBuf, CharacterFile)> {
            let json_path = paths.character_json(name)?;
            let json_str = std::fs::read_to_string(&json_path).ok()?;
            let char_def = CharacterFile::from_json(&json_str).ok()?;
            Some((json_path, char_def))
        };

        // Helper: try to load character sprite (may fail for unsupported atlas formats)
        let load_char_sprite = |paths: &AssetPaths, gpu: &GpuState, json_path: &std::path::Path, char_def: &CharacterFile, stage_x: f64, stage_y: f64, is_player: bool, stage_name: &str| -> Option<CharacterSprite> {
            let effective_image = char_def.effective_image();
            if effective_image.is_empty() {
                log::warn!("Character has empty image field, skipping sprite");
                return None;
            }
            let atlas_dir = paths.character_atlas_dir(effective_image)?;
            let mut sprite = CharacterSprite::load(gpu, json_path, &atlas_dir, stage_x, stage_y, is_player);
            // Apply per-stage scale override
            if let Some(&stage_scale) = char_def.stage_scale.get(stage_name) {
                sprite.scale = stage_scale as f32;
            }
            Some(sprite)
        };

        let srgb_to_linear = |s: f32| -> f32 {
            if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };

        // Load BF
        let bf_def = parse_char(&paths, &parsed.song.player1);
        if let Some((json_path, char_def)) = &bf_def {
            self.char_bf = load_char_sprite(&paths, gpu, json_path, char_def, stage.boyfriend[0], stage.boyfriend[1], true, stage_name);
            // Extract health info even if sprite failed to load
            let c = char_def.healthbar_colors;
            self.hb_color_bf = [
                srgb_to_linear(c[0] as f32 / 255.0),
                srgb_to_linear(c[1] as f32 / 255.0),
                srgb_to_linear(c[2] as f32 / 255.0),
                1.0,
            ];
            self.icon_bf = paths.health_icon(&char_def.healthicon)
                .map(|p| gpu.load_texture_from_path(&p));
        }
        if self.icon_bf.is_none() {
            self.icon_bf = paths.health_icon("bf")
                .map(|p| gpu.load_texture_from_path(&p));
        }

        // Load Dad
        let dad_def = parse_char(&paths, &parsed.song.player2);
        if let Some((json_path, char_def)) = &dad_def {
            self.char_dad = load_char_sprite(&paths, gpu, json_path, char_def, stage.opponent[0], stage.opponent[1], false, stage_name);
            let c = char_def.healthbar_colors;
            self.hb_color_dad = [
                srgb_to_linear(c[0] as f32 / 255.0),
                srgb_to_linear(c[1] as f32 / 255.0),
                srgb_to_linear(c[2] as f32 / 255.0),
                1.0,
            ];
            self.icon_dad = paths.health_icon(&char_def.healthicon)
                .map(|p| gpu.load_texture_from_path(&p));
        }
        if self.icon_dad.is_none() {
            self.icon_dad = paths.health_icon("dad")
                .map(|p| gpu.load_texture_from_path(&p));
        }

        // Load GF
        if !stage.hide_girlfriend {
            if let Some((json_path, char_def)) = parse_char(&paths, &parsed.song.gf_version) {
                self.char_gf = load_char_sprite(&paths, gpu, &json_path, &char_def, stage.girlfriend[0], stage.girlfriend[1], false, stage_name);
            }
        }

        // Store camera offsets for dynamic recomputation at section changes
        self.bf_cam_off = if let Some((_, char_def)) = &bf_def {
            let off = char_def.stage_camera.get(stage_name).copied()
                .unwrap_or(char_def.camera_position);
            [off[0] as f32, off[1] as f32]
        } else {
            [0.0, 0.0]
        };
        self.dad_cam_off = if let Some((_, char_def)) = &dad_def {
            let off = char_def.stage_camera.get(stage_name).copied()
                .unwrap_or(char_def.camera_position);
            [off[0] as f32, off[1] as f32]
        } else {
            [0.0, 0.0]
        };
        self.stage_cam_bf = [stage.camera_boyfriend[0] as f32, stage.camera_boyfriend[1] as f32];
        self.stage_cam_dad = [stage.camera_opponent[0] as f32, stage.camera_opponent[1] as f32];

        // Camera targets — compute from current character midpoints
        self.recompute_camera_targets();

        // Preload bf-dead character for death screen
        if let Some((json_path, char_def)) = parse_char(&paths, "bf-dead") {
            self.death_char_preloaded = load_char_sprite(
                &paths, gpu, &json_path, &char_def,
                stage.boyfriend[0], stage.boyfriend[1], true, stage_name,
            );
        }

        // Start camera — use stage camera_start if specified, otherwise opponent position
        if let Some(start) = stage.camera_start {
            self.camera.snap_to(start[0] as f32, start[1] as f32);
        } else {
            self.camera.snap_to(self.cam_dad[0], self.cam_dad[1]);
        }

        // Disable beat zooming if stage specifies it
        self.disable_zooming = stage.disable_zooming;

        // Load audio
        let mut audio = AudioEngine::new();
        if let Some(inst) = paths.song_audio(&self.song_name, "Inst.ogg") {
            audio.load_inst(&inst);
        }
        // Try split vocals first (Psych Engine format), then single Voices.ogg
        if let Some(vp) = paths.song_audio(&self.song_name, "Voices-Player.ogg") {
            audio.load_vocals(&vp);
        } else if let Some(vp) = paths.song_audio(&self.song_name, "VoicesPlayable.ogg") {
            audio.load_vocals(&vp);
        } else if let Some(v) = paths.song_audio(&self.song_name, "Voices.ogg") {
            audio.load_vocals(&v);
        }
        if let Some(vo) = paths.song_audio(&self.song_name, "Voices-Opponent.ogg") {
            audio.load_opponent_vocals(&vo);
        } else if let Some(v) = paths.song_audio(&self.song_name, "Voices.ogg") {
            // If no split opponent vocals, use combined Voices.ogg as opponent too
            audio.load_opponent_vocals(&v);
        }
        // Miss sounds from shared sounds dir
        if let Some(sounds_dir) = paths.find_dir("sounds") {
            audio.load_miss_sounds(&sounds_dir);
        }
        self.audio = Some(audio);

        // Initialize strum default positions for modchart property access
        for lane in 0..4 {
            let opp_x = Self::strum_x(lane, false);
            let plr_x = Self::strum_x(lane, true);
            self.scripts.state.strum_props[lane].x = opp_x;
            self.scripts.state.strum_props[lane].y = STRUM_Y;
            self.scripts.state.strum_props[lane + 4].x = plr_x;
            self.scripts.state.strum_props[lane + 4].y = STRUM_Y;
        }

        // Set default strum position globals on all scripts (Psych Engine's setOnScripts)
        // These are used everywhere in modcharts: _G['defaultPlayerStrumX0'], etc.
        for lane in 0..4usize {
            let opp_x = Self::strum_x(lane, false) as f64;
            let plr_x = Self::strum_x(lane, true) as f64;
            self.scripts.set_on_all(&format!("defaultOpponentStrumX{lane}"), opp_x);
            self.scripts.set_on_all(&format!("defaultOpponentStrumY{lane}"), STRUM_Y as f64);
            self.scripts.set_on_all(&format!("defaultPlayerStrumX{lane}"), plr_x);
            self.scripts.set_on_all(&format!("defaultPlayerStrumY{lane}"), STRUM_Y as f64);
        }
        // Also set the non-indexed variants (defaultOpponentStrumY0 style is already covered,
        // but some scripts use just defaultOpponentStrumY0 / defaultPlayerStrumY0)
        self.scripts.set_on_all("defaultOpponentStrumY0", STRUM_Y as f64);
        self.scripts.set_on_all("defaultPlayerStrumY0", STRUM_Y as f64);

        // Set crochet/stepCrochet globals
        self.scripts.set_on_all("crochet", self.game.conductor.crochet);
        self.scripts.set_on_all("stepCrochet", self.game.conductor.step_crochet);

        // Set misc globals scripts expect
        self.scripts.set_bool_on_all("flashingLights", true);
        self.scripts.set_bool_on_all("modcharts", true);
        self.scripts.set_bool_on_all("mustHitSection", false);

        // Initialize countdown
        self.game.conductor.song_position = -self.game.conductor.crochet * 5.0;
        self.game.countdown_timer = self.game.conductor.crochet * 5.0;

        log::info!(
            "PlayScreen: {} ({}) - {} notes, speed {:.1}, BPM {:.0}, stage '{}', scripts: {}",
            self.song_name, self.difficulty, self.game.notes.len(),
            self.game.song_speed, self.game.conductor.bpm, stage_name,
            if self.scripts.has_scripts() { "yes" } else { "none" },
        );

        // Call onCreatePost after everything is initialized
        if self.scripts.has_scripts() {
            self.scripts.call("onCreatePost");
            self.process_lua_sprites(gpu);
            self.process_property_writes();
        }
    }
}
