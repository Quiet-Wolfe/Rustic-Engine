use std::path::{Path, PathBuf};

use winit::keyboard::KeyCode;

use rustic_audio::AudioEngine;
use rustic_core::character::CharacterFile;
use rustic_core::chart;
use rustic_core::conductor::Conductor;
use rustic_core::note::NoteData;
use rustic_core::rating::{self, Rating};
use rustic_core::scoring::{self, ScoreState};
use rustic_core::stage::StageFile;
use rustic_render::camera::GameCamera;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::SpriteAtlas;

use crate::screen::Screen;
use super::characters::{CharacterSprite, StageBgSprite};

// === Psych Engine constants ===
pub const GAME_W: f32 = 1280.0;
pub const GAME_H: f32 = 720.0;
const STRUM_Y: f32 = 50.0;
const STRUM_X: f32 = 42.0;
const NOTE_WIDTH: f32 = 112.0; // 160 * 0.7
const NOTE_SCALE: f32 = 0.7;
const KILL_OFFSET_MS: f64 = 350.0;
const SCROLL_SPEED_FACTOR: f64 = 0.45;

const LANE_KEYS: [[KeyCode; 2]; 4] = [
    [KeyCode::KeyD, KeyCode::ArrowLeft],
    [KeyCode::KeyF, KeyCode::ArrowDown],
    [KeyCode::KeyJ, KeyCode::ArrowUp],
    [KeyCode::KeyK, KeyCode::ArrowRight],
];

// Health bar layout
const HEALTH_BAR_W: f32 = 600.0;
const HEALTH_BAR_H: f32 = 18.0;
const HEALTH_BAR_Y: f32 = GAME_H - 80.0;
const HEALTH_BAR_X: f32 = (GAME_W - HEALTH_BAR_W) / 2.0;
const ICON_SIZE: f32 = 75.0;

// Note atlas animation names per lane (left/down/up/right).
// Scrolling notes use color frames matching Note.hx colArray + '0' prefix.
const NOTE_ANIMS: [&str; 4] = ["purpleScroll", "blueScroll", "greenScroll", "redScroll"];
const NOTE_PREFIXES: [&str; 4] = ["purple0", "blue0", "green0", "red0"];
const STRUM_ANIMS: [&str; 4] = ["arrowLEFT", "arrowDOWN", "arrowUP", "arrowRIGHT"];
const PRESS_ANIMS: [&str; 4] = ["left press", "down press", "up press", "right press"];
const CONFIRM_ANIMS: [&str; 4] = ["left confirm", "down confirm", "up confirm", "right confirm"];
const HOLD_PIECE_ANIMS: [&str; 4] = ["purple hold piece", "blue hold piece", "green hold piece", "red hold piece"];
const HOLD_END_ANIMS: [&str; 4] = ["purple hold end", "blue hold end", "green hold end", "red hold end"];

/// Runtime note with rendering state.
struct PlayNote {
    data: NoteData,
    y_pos: f32,
}

/// A rating popup that fades out.
struct RatingPopup {
    text: String,
    combo: i32,
    timer: f64,
}

const RATING_DURATION: f64 = 800.0;

/// Loaded note sprite assets.
struct NoteAssets {
    texture: GpuTexture,
    atlas: SpriteAtlas,
    tex_w: f32,
    tex_h: f32,
}

/// Section info for camera targeting.
struct SectionInfo {
    must_hit: bool,
    start_time: f64,
}

pub struct PlayScreen {
    notes: Vec<PlayNote>,
    conductor: Conductor,
    audio: Option<AudioEngine>,
    song_started: bool,
    song_ended: bool,
    countdown_timer: f64,
    countdown_beat: i32,
    score: ScoreState,
    ratings: Vec<Rating>,
    keys_held: [bool; 4],
    player_confirm: [f64; 4],
    opponent_confirm: [f64; 4],
    song_speed: f64,
    song_name: String,
    difficulty: String,
    rating_popup: Option<RatingPopup>,
    note_assets: Option<NoteAssets>,
    icon_bf: Option<GpuTexture>,
    icon_dad: Option<GpuTexture>,
    // Phase 3: Characters & Stage
    char_bf: Option<CharacterSprite>,
    char_dad: Option<CharacterSprite>,
    char_gf: Option<CharacterSprite>,
    stage_bg: Vec<StageBgSprite>,
    camera: GameCamera,
    sections: Vec<SectionInfo>,
    cur_section: usize,
    /// Camera target positions for BF and opponent (from stage + character JSON).
    cam_bf: [f32; 2],
    cam_dad: [f32; 2],
    last_beat: i32,
    /// Health bar colors [r, g, b] normalized 0-1 for player and opponent.
    hb_color_bf: [f32; 4],
    hb_color_dad: [f32; 4],
}

impl PlayScreen {
    pub fn new(song_name: &str, difficulty: &str) -> Self {
        Self {
            notes: Vec::new(),
            conductor: Conductor::new(100.0),
            audio: None,
            song_started: false,
            song_ended: false,
            countdown_timer: 0.0,
            countdown_beat: -5,
            score: ScoreState::new(),
            ratings: Rating::load_default(),
            keys_held: [false; 4],
            player_confirm: [0.0; 4],
            opponent_confirm: [0.0; 4],
            song_speed: 1.0,
            song_name: song_name.to_string(),
            difficulty: difficulty.to_string(),
            rating_popup: None,
            note_assets: None,
            icon_bf: None,
            icon_dad: None,
            char_bf: None,
            char_dad: None,
            char_gf: None,
            stage_bg: Vec::new(),
            camera: GameCamera::new(0.9),
            sections: Vec::new(),
            cur_section: 0,
            cam_bf: [0.0; 2],
            cam_dad: [0.0; 2],
            last_beat: -999,
            hb_color_bf: [0.2, 0.8, 0.2, 1.0],
            hb_color_dad: [0.8, 0.1, 0.1, 1.0],
        }
    }

    fn key_to_lane(key: KeyCode) -> Option<usize> {
        for (lane, binds) in LANE_KEYS.iter().enumerate() {
            if binds.contains(&key) {
                return Some(lane);
            }
        }
        None
    }

    fn strum_x(lane: usize, player: bool) -> f32 {
        let base = STRUM_X + 50.0 + NOTE_WIDTH * lane as f32;
        if player { base + GAME_W / 2.0 } else { base }
    }

    fn try_hit_note(&mut self, lane: usize) {
        let mut best_idx = None;
        let mut best_time = f64::MAX;
        let max_window = 166.0;

        for (i, pn) in self.notes.iter().enumerate() {
            if !pn.data.must_press || pn.data.lane != lane || pn.data.was_good_hit || pn.data.too_late {
                continue;
            }
            let diff = (pn.data.strum_time - self.conductor.song_position).abs();
            if diff <= max_window && pn.data.strum_time < best_time {
                best_time = pn.data.strum_time;
                best_idx = Some(i);
            }
        }

        if let Some(idx) = best_idx {
            let diff = (self.notes[idx].data.strum_time - self.conductor.song_position).abs();
            if let Some(judgment) = rating::judge_note(&self.ratings, diff) {
                self.notes[idx].data.was_good_hit = true;
                self.score.note_hit(
                    judgment.score, judgment.rating_mod,
                    judgment.health_gain, &judgment.name,
                );
                self.rating_popup = Some(RatingPopup {
                    text: judgment.name.to_uppercase(),
                    combo: self.score.combo,
                    timer: RATING_DURATION,
                });
                // Flash confirm on strum receptor (elapsed time, counts up)
                self.player_confirm[lane] = f64::MIN_POSITIVE;
                if let Some(audio) = &mut self.audio {
                    audio.unmute_player_vocals();
                }
                // Trigger BF sing animation
                if let Some(bf) = &mut self.char_bf {
                    bf.play_sing(lane);
                }
            }
        }
    }

    fn draw_note_sprite(&self, gpu: &mut GpuState, anim: &str, x: f32, y: f32, scale: f32) {
        if let Some(assets) = &self.note_assets {
            if let Some(frame) = assets.atlas.get_frame(anim, 0) {
                gpu.draw_sprite_frame(
                    frame, assets.tex_w, assets.tex_h,
                    x, y, scale, false, [1.0, 1.0, 1.0, 1.0],
                );
            }
        }
    }

    fn draw_strum(&self, gpu: &mut GpuState, lane: usize, player: bool) {
        let x = Self::strum_x(lane, player);
        let elapsed = if player { self.player_confirm[lane] } else { self.opponent_confirm[lane] };
        let (anim, frame_idx) = if elapsed > 0.0 {
            let idx = (elapsed / (1000.0 / 24.0)) as usize;
            (CONFIRM_ANIMS[lane], idx)
        } else if player && self.keys_held[lane] {
            (PRESS_ANIMS[lane], 0)
        } else {
            (STRUM_ANIMS[lane], 0)
        };
        if let Some(assets) = &self.note_assets {
            // Fixed pivot from the static strum frame so animations don't jump
            let static_frame = assets.atlas.get_frame(STRUM_ANIMS[lane], 0);
            let (ref_w, ref_h) = static_frame
                .map(|f| (f.frame_w, f.frame_h))
                .unwrap_or((NOTE_WIDTH / NOTE_SCALE, NOTE_WIDTH / NOTE_SCALE));
            let cx = x + ref_w * NOTE_SCALE / 2.0;
            let cy = STRUM_Y + ref_h * NOTE_SCALE / 2.0;

            let count = assets.atlas.frame_count(anim);
            let clamped = if count > 0 { frame_idx.min(count - 1) } else { 0 };
            if let Some(frame) = assets.atlas.get_frame(anim, clamped) {
                let draw_x = cx - frame.frame_w * NOTE_SCALE / 2.0;
                let draw_y = cy - frame.frame_h * NOTE_SCALE / 2.0;
                gpu.draw_sprite_frame(
                    frame, assets.tex_w, assets.tex_h,
                    draw_x, draw_y, NOTE_SCALE, false, [1.0, 1.0, 1.0, 1.0],
                );
            }
        }
    }

    fn draw_hold_tail(&self, gpu: &mut GpuState, pn: &PlayNote) {
        let assets = match &self.note_assets { Some(a) => a, None => return };
        let lane = pn.data.lane;

        let piece = match assets.atlas.get_frame(HOLD_PIECE_ANIMS[lane], 0) {
            Some(f) => f.clone(), None => return,
        };
        let end = match assets.atlas.get_frame(HOLD_END_ANIMS[lane], 0) {
            Some(f) => f.clone(), None => return,
        };

        let tw = assets.tex_w;
        let th = assets.tex_h;
        let x = Self::strum_x(lane, pn.data.must_press);
        let white = [1.0, 1.0, 1.0, 1.0];

        // Scaled dimensions
        let pw = piece.src.w * NOTE_SCALE;
        let ph = piece.src.h * NOTE_SCALE;
        let ew = end.src.w * NOTE_SCALE;
        let eh = end.src.h * NOTE_SCALE;

        // Total hold height in pixels
        let hold_h = (SCROLL_SPEED_FACTOR * pn.data.sustain_length * self.song_speed) as f32;

        // Hold starts overlapping the note head (hidden behind it in draw order)
        let hold_top = pn.y_pos + NOTE_WIDTH * 0.5;
        // Clip at strum receptor bottom for hit notes
        let clip_y = if pn.data.was_good_hit { STRUM_Y + NOTE_WIDTH * 0.5 } else { -999.0 };

        // Center hold in lane
        let px = x + (NOTE_WIDTH - pw) / 2.0;
        let ex = x + (NOTE_WIDTH - ew) / 2.0;

        // Hold end position
        let end_y = hold_top + hold_h - eh;

        // Tile hold pieces from hold_top to end_y
        let mut cy = hold_top;
        while cy < end_y {
            let tile_h = ph.min(end_y - cy);
            let vis_top = cy.max(clip_y);
            let vis_h = (cy + tile_h) - vis_top;

            if vis_h > 0.5 && vis_top < GAME_H + 100.0 {
                let clip_frac = if vis_top > cy { (vis_top - cy) / tile_h } else { 0.0 };
                gpu.push_texture_region(
                    tw, th,
                    piece.src.x, piece.src.y + piece.src.h * clip_frac,
                    piece.src.w, piece.src.h * (vis_h / tile_h),
                    px, vis_top, pw, vis_h,
                    false, white,
                );
            }
            cy += ph;
        }

        // Draw hold end
        if end_y + eh > clip_y && end_y < GAME_H + 100.0 {
            let vis_top = end_y.max(clip_y);
            let vis_h = (end_y + eh) - vis_top;
            if vis_h > 0.5 {
                let clip_frac = if vis_top > end_y { (vis_top - end_y) / eh } else { 0.0 };
                gpu.push_texture_region(
                    tw, th,
                    end.src.x, end.src.y + end.src.h * clip_frac,
                    end.src.w, end.src.h * (vis_h / eh),
                    ex, vis_top, ew, vis_h,
                    false, white,
                );
            }
        }
    }
}

impl Screen for PlayScreen {
    fn init(&mut self, gpu: &GpuState) {
        // Load note atlas
        let note_png = Path::new("references/FNF-PsychEngine/assets/shared/images/noteSkins/NOTE_assets.png");
        let note_xml = std::fs::read_to_string(
            "references/FNF-PsychEngine/assets/shared/images/noteSkins/NOTE_assets.xml"
        ).expect("Failed to read NOTE_assets.xml");

        let note_tex = gpu.load_texture_from_path(note_png);
        let mut atlas = SpriteAtlas::from_xml(&note_xml);

        // Register note scroll animations with specific prefix (purple0, blue0, etc.)
        for (anim, prefix) in NOTE_ANIMS.iter().zip(NOTE_PREFIXES.iter()) {
            atlas.add_by_prefix(anim, prefix);
        }
        // Register strum, press, confirm, and hold animations
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

        // Load health bar icons (loaded after characters below)

        // Load chart
        let chart_dir = format!(
            "references/FNF-PsychEngine/assets/base_game/shared/data/{}",
            self.song_name
        );
        let chart_file = if self.difficulty == "normal" {
            format!("{}/{}.json", chart_dir, self.song_name)
        } else {
            format!("{}/{}-{}.json", chart_dir, self.song_name, self.difficulty)
        };

        let chart_json = std::fs::read_to_string(&chart_file)
            .unwrap_or_else(|e| panic!("Failed to read chart {:?}: {}", chart_file, e));
        let parsed = chart::parse_chart(&chart_json).expect("Failed to parse chart");

        self.song_speed = parsed.song.speed;
        self.conductor.set_bpm(parsed.song.bpm);

        let sections: Vec<(bool, f64, f64)> = parsed.song.notes.iter()
            .map(|s| {
                let bpm = if s.change_bpm && s.bpm > 0.0 { s.bpm } else { parsed.song.bpm };
                (s.change_bpm, bpm, s.section_beats)
            })
            .collect();
        self.conductor.map_bpm_changes(parsed.song.bpm, sections);

        let mut notes: Vec<PlayNote> = parsed.notes.into_iter()
            .map(|nd| PlayNote { data: nd, y_pos: 0.0 })
            .collect();
        notes.sort_by(|a, b| a.data.strum_time.partial_cmp(&b.data.strum_time).unwrap());
        self.notes = notes;

        // Build section timing data for camera targeting
        {
            let mut section_time = 0.0;
            let mut cur_bpm = parsed.song.bpm;
            for s in &parsed.song.notes {
                if s.change_bpm && s.bpm > 0.0 { cur_bpm = s.bpm; }
                self.sections.push(SectionInfo {
                    must_hit: s.must_hit_section,
                    start_time: section_time,
                });
                let step_crochet = ((60.0 / cur_bpm) * 1000.0) / 4.0;
                section_time += step_crochet * s.section_beats * 4.0;
            }
        }

        // Load stage
        let stage_name = &parsed.song.stage;
        let stage_json_path = format!(
            "references/FNF-PsychEngine/assets/base_game/shared/stages/{}.json", stage_name
        );
        let stage = if let Ok(json_str) = std::fs::read_to_string(&stage_json_path) {
            StageFile::from_json(&json_str).unwrap_or_else(|_| StageFile::default_stage())
        } else {
            StageFile::default_stage()
        };

        self.camera = GameCamera::new(stage.default_zoom as f32);
        self.camera.camera_speed = stage.camera_speed as f32;

        // Load stage background sprites (default stage: week1)
        let stage_img_dir = if stage.directory.is_empty() {
            "references/FNF-PsychEngine/assets/base_game/week1/images".to_string()
        } else {
            format!("references/FNF-PsychEngine/assets/base_game/{}/images", stage.directory)
        };

        let load_bg = |gpu: &GpuState, name: &str, x: f32, y: f32, scale: f32, scroll_x: f32, scroll_y: f32, flip_x: bool| -> Option<StageBgSprite> {
            let path = PathBuf::from(&stage_img_dir).join(format!("{}.png", name));
            if !path.exists() { return None; }
            let tex = gpu.load_texture_from_path(&path);
            Some(StageBgSprite::new(tex, x, y, scale, scroll_x, scroll_y, flip_x))
        };

        // Default stage sprite positions (from StageWeek1.hx)
        if let Some(bg) = load_bg(gpu, "stageback", -600.0, -200.0, 1.0, 0.9, 0.9, false) {
            self.stage_bg.push(bg);
        }
        if let Some(bg) = load_bg(gpu, "stagefront", -650.0, 600.0, 1.1, 0.9, 0.9, false) {
            self.stage_bg.push(bg);
        }
        if let Some(bg) = load_bg(gpu, "stagecurtains", -500.0, -300.0, 0.9, 1.3, 1.3, false) {
            self.stage_bg.push(bg);
        }

        // Load characters
        let shared_path = Path::new("references/FNF-PsychEngine/assets/shared");
        let base_shared = Path::new("references/FNF-PsychEngine/assets/base_game/shared");
        let img_root = shared_path.join("images");
        let base_img_root = base_shared.join("images");

        // Helper: find character JSON in shared or base_game/shared
        let find_char_json = |name: &str| -> PathBuf {
            let shared = shared_path.join(format!("characters/{}.json", name));
            if shared.exists() { return shared; }
            base_shared.join(format!("characters/{}.json", name))
        };

        // Helper: find atlas directory (images may be in shared or base_game/shared)
        let find_atlas_dir = |image_field: &str| -> PathBuf {
            // image field is like "characters/BOYFRIEND" — we need the parent dir
            let shared_check = img_root.join(format!("{}.png", image_field));
            if shared_check.exists() { return img_root.clone(); }
            base_img_root.clone()
        };

        // Helper: peek at character JSON image field for atlas directory
        let peek_image_field = |path: &Path| -> String {
            let s = std::fs::read_to_string(path).unwrap();
            CharacterFile::from_json(&s).unwrap().image
        };

        // Load BF
        let bf_json = find_char_json(&parsed.song.player1);
        if bf_json.exists() {
            let atlas_dir = find_atlas_dir(&peek_image_field(&bf_json));
            self.char_bf = Some(CharacterSprite::load(
                gpu, &bf_json, &atlas_dir,
                stage.boyfriend[0], stage.boyfriend[1], true,
            ));
        }

        // Load Dad
        let dad_json = find_char_json(&parsed.song.player2);
        if dad_json.exists() {
            let atlas_dir = find_atlas_dir(&peek_image_field(&dad_json));
            self.char_dad = Some(CharacterSprite::load(
                gpu, &dad_json, &atlas_dir,
                stage.opponent[0], stage.opponent[1], false,
            ));
        }

        // Load GF
        if !stage.hide_girlfriend {
            let gf_json = find_char_json(&parsed.song.gf_version);
            if gf_json.exists() {
                let atlas_dir = find_atlas_dir(&peek_image_field(&gf_json));
                self.char_gf = Some(CharacterSprite::load(
                    gpu, &gf_json, &atlas_dir,
                    stage.girlfriend[0], stage.girlfriend[1], false,
                ));
            }
        }

        // Camera targets: Psych Engine uses getMidpoint() + offset + cameraPosition
        // Opponent: midpoint.x + 150, midpoint.y - 100
        // Player:   midpoint.x - 100, midpoint.y - 100
        if let Some(bf) = &self.char_bf {
            let (mx, my) = bf.midpoint();
            self.cam_bf = [
                mx - 100.0 + stage.camera_boyfriend[0] as f32,
                my - 100.0 + stage.camera_boyfriend[1] as f32,
            ];
            let c = bf.healthbar_colors;
            self.hb_color_bf = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0];
        }
        if let Some(dad) = &self.char_dad {
            let (mx, my) = dad.midpoint();
            self.cam_dad = [
                mx + 150.0 + stage.camera_opponent[0] as f32,
                my - 100.0 + stage.camera_opponent[1] as f32,
            ];
            let c = dad.healthbar_colors;
            self.hb_color_dad = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0];
        }

        // Load health bar icons based on character healthicon field
        let icon_path = Path::new("references/FNF-PsychEngine/assets/shared/images/icons");
        if let Some(bf) = &self.char_bf {
            let p = icon_path.join(format!("icon-{}.png", bf.healthicon));
            if p.exists() { self.icon_bf = Some(gpu.load_texture_from_path(&p)); }
        }
        if self.icon_bf.is_none() {
            self.icon_bf = Some(gpu.load_texture_from_path(&icon_path.join("icon-bf.png")));
        }
        if let Some(dad) = &self.char_dad {
            let p = icon_path.join(format!("icon-{}.png", dad.healthicon));
            if p.exists() { self.icon_dad = Some(gpu.load_texture_from_path(&p)); }
        }
        if self.icon_dad.is_none() {
            self.icon_dad = Some(gpu.load_texture_from_path(&icon_path.join("icon-dad.png")));
        }

        // Start camera on opponent
        self.camera.snap_to(self.cam_dad[0], self.cam_dad[1]);

        // Load audio
        let song_dir = format!(
            "references/FNF-PsychEngine/assets/base_game/songs/{}",
            self.song_name
        );
        let mut audio = AudioEngine::new();
        audio.load_inst(&PathBuf::from(format!("{}/Inst.ogg", song_dir)));
        audio.load_vocals(&PathBuf::from(format!("{}/Voices-Player.ogg", song_dir)));
        audio.load_opponent_vocals(&PathBuf::from(format!("{}/Voices-Opponent.ogg", song_dir)));
        audio.load_miss_sounds(Path::new("references/FNF-PsychEngine/assets/shared/sounds"));
        self.audio = Some(audio);

        self.conductor.song_position = -self.conductor.crochet * 5.0;
        self.countdown_timer = self.conductor.crochet * 5.0;

        log::info!(
            "PlayScreen: {} ({}) - {} notes, speed {:.1}, BPM {:.0}",
            self.song_name, self.difficulty, self.notes.len(),
            self.song_speed, self.conductor.bpm,
        );
    }

    fn handle_key(&mut self, key: KeyCode) {
        if let Some(lane) = Self::key_to_lane(key) {
            if !self.keys_held[lane] {
                self.keys_held[lane] = true;
                self.try_hit_note(lane);
            }
        }
    }

    fn handle_key_release(&mut self, key: KeyCode) {
        if let Some(lane) = Self::key_to_lane(key) {
            self.keys_held[lane] = false;
        }
    }

    fn update(&mut self, dt: f32) {
        let dt_ms = dt as f64 * 1000.0;

        // Advance strum confirm elapsed timers
        let confirm_dur = self.conductor.crochet / 4.0 * 1.25;
        for i in 0..4 {
            if self.player_confirm[i] > 0.0 {
                self.player_confirm[i] += dt_ms;
                // Keep confirm on last frame while key is held; reset only on release
                if self.player_confirm[i] > confirm_dur && !self.keys_held[i] {
                    self.player_confirm[i] = 0.0;
                }
            }
            if self.opponent_confirm[i] > 0.0 {
                self.opponent_confirm[i] += dt_ms;
                if self.opponent_confirm[i] > confirm_dur { self.opponent_confirm[i] = 0.0; }
            }
        }

        if let Some(popup) = &mut self.rating_popup {
            popup.timer -= dt_ms;
            if popup.timer <= 0.0 {
                self.rating_popup = None;
            }
        }

        if !self.song_started {
            self.conductor.song_position += dt_ms;
            self.countdown_timer -= dt_ms;
            let beat = (self.conductor.song_position / self.conductor.crochet).floor() as i32;
            if beat != self.countdown_beat {
                self.countdown_beat = beat;
            }
            if self.countdown_timer <= 0.0 {
                if let Some(audio) = &mut self.audio {
                    audio.play();
                }
                self.song_started = true;
                self.conductor.song_position = 0.0;
            }
        } else if let Some(audio) = &self.audio {
            let audio_pos = audio.position_ms();
            let diff = audio_pos - self.conductor.song_position;
            if diff.abs() > 50.0 {
                self.conductor.song_position = audio_pos;
            } else {
                self.conductor.song_position += dt_ms + diff * 0.02;
            }
        }

        for pn in &mut self.notes {
            if pn.data.was_good_hit || pn.data.too_late {
                if pn.data.sustain_length > 0.0 {
                    pn.y_pos = STRUM_Y
                        - (SCROLL_SPEED_FACTOR
                            * (self.conductor.song_position - pn.data.strum_time)
                            * self.song_speed) as f32;
                }
                continue;
            }

            pn.y_pos = STRUM_Y
                - (SCROLL_SPEED_FACTOR
                    * (self.conductor.song_position - pn.data.strum_time)
                    * self.song_speed) as f32;

            // Opponent notes: auto-hit at strum time, flash confirm + sing animation
            if !pn.data.must_press && self.conductor.song_position >= pn.data.strum_time {
                pn.data.was_good_hit = true;
                self.opponent_confirm[pn.data.lane] = f64::MIN_POSITIVE;
                if let Some(dad) = &mut self.char_dad {
                    dad.play_sing(pn.data.lane);
                }
            }

            if pn.data.must_press
                && self.conductor.song_position - pn.data.strum_time > KILL_OFFSET_MS
            {
                pn.data.too_late = true;
                if !pn.data.is_sustain() {
                    self.score.note_miss(scoring::HEALTH_MISS);
                    self.rating_popup = Some(RatingPopup {
                        text: "MISS".into(), combo: 0, timer: RATING_DURATION,
                    });
                    if let Some(audio) = &mut self.audio {
                        audio.mute_player_vocals();
                        audio.play_miss_sound();
                    }
                    // BF miss animation
                    if let Some(bf) = &mut self.char_bf {
                        bf.play_miss(pn.data.lane);
                    }
                }
            }
        }

        // Hold notes: health gain/drain for player, loop confirm for both sides
        let step_ms = self.conductor.crochet / 4.0;
        let confirm_cycle_ms = 4.0 * (1000.0 / 24.0); // 4 frames at 24fps
        for pn in &self.notes {
            if pn.data.sustain_length <= 0.0 { continue; }
            let end_time = pn.data.strum_time + pn.data.sustain_length;
            if self.conductor.song_position > end_time { continue; }

            if pn.data.was_good_hit {
                if pn.data.must_press && self.keys_held[pn.data.lane] {
                    // Player holding: health gain + loop confirm
                    let ticks = dt_ms / step_ms;
                    self.score.change_health(scoring::HEALTH_HOLD_TICK * ticks as f32);
                    if self.player_confirm[pn.data.lane] <= 0.0
                        || self.player_confirm[pn.data.lane] >= confirm_cycle_ms
                    {
                        self.player_confirm[pn.data.lane] = f64::MIN_POSITIVE;
                    }
                } else if pn.data.must_press && !self.keys_held[pn.data.lane] {
                    // Player released hold: drain health per tick
                    let ticks = dt_ms / step_ms;
                    self.score.change_health(-scoring::HEALTH_MISS * ticks as f32);
                } else if !pn.data.must_press {
                    // Opponent hold: loop confirm
                    if self.opponent_confirm[pn.data.lane] <= 0.0
                        || self.opponent_confirm[pn.data.lane] >= confirm_cycle_ms
                    {
                        self.opponent_confirm[pn.data.lane] = f64::MIN_POSITIVE;
                    }
                }
            } else if pn.data.too_late && pn.data.must_press {
                // Missed hold note: drain health per tick
                let ticks = dt_ms / step_ms;
                self.score.change_health(-scoring::HEALTH_MISS * ticks as f32);
            }
        }

        // Update character animations
        let dt_secs = dt;

        // Opponent sing→idle transition based on singDuration
        if let Some(dad) = &mut self.char_dad {
            if dad.anim.current_anim.starts_with("sing") {
                dad.hold_timer += dt_ms;
                let step_crochet = self.conductor.step_crochet;
                let threshold = step_crochet * dad.sing_duration * 1.1;
                if dad.hold_timer >= threshold {
                    dad.dance();
                    dad.hold_timer = 0.0;
                }
            }
            dad.update(dt_secs);
        }

        // BF sing→idle: reset on non-sing anims (player controls when to idle)
        if let Some(bf) = &mut self.char_bf {
            if bf.anim.current_anim.starts_with("sing") {
                bf.hold_timer += dt_ms;
                // Player: return to idle after sing duration if no keys held
                let step_crochet = self.conductor.step_crochet;
                let threshold = step_crochet * bf.sing_duration * 1.1;
                if bf.hold_timer >= threshold && !self.keys_held.iter().any(|&k| k) {
                    bf.dance();
                    bf.hold_timer = 0.0;
                }
            }
            bf.update(dt_secs);
        }

        // Beat-based idle dance for all characters
        let beat = self.conductor.cur_beat();
        if beat != self.last_beat {
            // Dad dances on every beat when not singing
            if let Some(dad) = &mut self.char_dad {
                if !dad.anim.current_anim.starts_with("sing") {
                    dad.dance();
                }
            }
            // BF dances on every beat when not singing
            if let Some(bf) = &mut self.char_bf {
                if !bf.anim.current_anim.starts_with("sing") {
                    bf.dance();
                }
            }
            // GF dances every 2 beats
            if let Some(gf) = &mut self.char_gf {
                if beat % 2 == 0 {
                    gf.dance();
                }
            }
        }
        self.last_beat = beat;

        if let Some(gf) = &mut self.char_gf {
            gf.update(dt_secs);
        }

        // Camera: follow current section's singer
        if !self.sections.is_empty() {
            // Find current section
            let mut sec_idx = self.cur_section;
            while sec_idx + 1 < self.sections.len()
                && self.conductor.song_position >= self.sections[sec_idx + 1].start_time
            {
                sec_idx += 1;
            }
            if sec_idx != self.cur_section {
                self.cur_section = sec_idx;
                let target = if self.sections[sec_idx].must_hit {
                    self.cam_bf
                } else {
                    self.cam_dad
                };
                self.camera.follow(target[0], target[1]);
            }
        }
        self.camera.update(dt_secs);

        if self.song_started && !self.song_ended {
            if let Some(audio) = &self.audio {
                if audio.is_finished() {
                    self.song_ended = true;
                }
            }
        }
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        let white = [1.0, 1.0, 1.0, 1.0];

        if !gpu.begin_frame() {
            return;
        }

        // === Stage background sprites ===
        for bg in self.stage_bg.iter() {
            bg.draw(gpu, &self.camera);
            gpu.draw_batch(Some(&bg.texture));
        }

        // === Characters (drawn between stage bg and HUD) ===
        if let Some(gf) = &self.char_gf {
            gf.draw(gpu, &self.camera);
            gpu.draw_batch(Some(&gf.texture));
        }
        if let Some(dad) = &self.char_dad {
            dad.draw(gpu, &self.camera);
            gpu.draw_batch(Some(&dad.texture));
        }
        if let Some(bf) = &self.char_bf {
            bf.draw(gpu, &self.camera);
            gpu.draw_batch(Some(&bf.texture));
        }

        // === Note atlas batch (hold tails → strum receptors → note heads) ===
        // Draw hold tails first (behind everything)
        for pn in &self.notes {
            if pn.data.sustain_length > 0.0 && !pn.data.too_late {
                self.draw_hold_tail(gpu, pn);
            }
        }

        // Strum receptors
        for player in [false, true] {
            for lane in 0..4 {
                self.draw_strum(gpu, lane, player);
            }
        }

        // Note heads
        for pn in &self.notes {
            if pn.data.was_good_hit || pn.data.too_late {
                continue;
            }
            if pn.y_pos < -NOTE_WIDTH || pn.y_pos > GAME_H + NOTE_WIDTH {
                continue;
            }
            let x = Self::strum_x(pn.data.lane, pn.data.must_press);
            self.draw_note_sprite(gpu, NOTE_ANIMS[pn.data.lane], x, pn.y_pos, NOTE_SCALE);
        }

        if let Some(assets) = &self.note_assets {
            gpu.draw_batch(Some(&assets.texture));
        } else {
            gpu.draw_batch(None);
        }

        // === Batch 2: Health bar (colored quads) ===
        let health_pct = self.score.health_percent();
        gpu.push_colored_quad(
            HEALTH_BAR_X - 2.0, HEALTH_BAR_Y - 2.0,
            HEALTH_BAR_W + 4.0, HEALTH_BAR_H + 4.0,
            [0.0, 0.0, 0.0, 1.0],
        );
        gpu.push_colored_quad(
            HEALTH_BAR_X, HEALTH_BAR_Y, HEALTH_BAR_W, HEALTH_BAR_H,
            self.hb_color_dad,
        );
        let player_w = HEALTH_BAR_W * health_pct;
        gpu.push_colored_quad(
            HEALTH_BAR_X + HEALTH_BAR_W - player_w, HEALTH_BAR_Y,
            player_w, HEALTH_BAR_H,
            self.hb_color_bf,
        );

        // Results overlay background
        if self.song_ended {
            gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.0, 0.6]);
        }

        gpu.draw_batch(None); // white texture for colored quads

        // === Batch 3+4: Health bar icons ===
        // Psych Engine: opponent icon on LEFT, player icon on RIGHT, both at divider
        // BF icon is flipped horizontally (faces left toward opponent)
        let divider_x = HEALTH_BAR_X + HEALTH_BAR_W * (1.0 - health_pct);
        let icon_y = HEALTH_BAR_Y + HEALTH_BAR_H / 2.0 - ICON_SIZE / 2.0;
        let bf_losing = health_pct < 0.2;
        let dad_losing = health_pct > 0.8;

        // Dad icon (opponent, left of divider)
        if let Some(icon) = &self.icon_dad {
            let src_x = if dad_losing { 150.0 } else { 0.0 };
            gpu.push_texture_region(
                icon.width as f32, icon.height as f32,
                src_x, 0.0, 150.0, 150.0,
                divider_x - ICON_SIZE + 15.0, icon_y, ICON_SIZE, ICON_SIZE,
                false, white,
            );
            gpu.draw_batch(Some(icon));
        }

        // BF icon (player, right of divider, flipped)
        if let Some(icon) = &self.icon_bf {
            let src_x = if bf_losing { 150.0 } else { 0.0 };
            gpu.push_texture_region(
                icon.width as f32, icon.height as f32,
                src_x, 0.0, 150.0, 150.0,
                divider_x - 15.0, icon_y, ICON_SIZE, ICON_SIZE,
                true, white,
            );
            gpu.draw_batch(Some(icon));
        }

        // === Text layer (handled by end_frame) ===
        let grade = self.score.grade();
        let score_text = format!(
            "Score: {} | Misses: {} | Acc: {:.2}% [{}]",
            self.score.score, self.score.misses, self.score.accuracy(), grade,
        );
        gpu.draw_text(&score_text, HEALTH_BAR_X, HEALTH_BAR_Y + HEALTH_BAR_H + 6.0, 16.0, white);

        if let Some(popup) = &self.rating_popup {
            let alpha = (popup.timer / RATING_DURATION) as f32;
            let popup_text = format!("{}\n{}", popup.text, popup.combo);
            gpu.draw_text(&popup_text, GAME_W / 2.0 - 40.0, GAME_H / 2.0 - 20.0, 32.0,
                [1.0, 1.0, 1.0, alpha]);
        }

        if !self.song_started {
            let countdown_text = match self.countdown_beat {
                -4 => Some("3"), -3 => Some("2"), -2 => Some("1"), -1 => Some("Go!"),
                _ => None,
            };
            if let Some(text) = countdown_text {
                gpu.draw_text(text, GAME_W / 2.0 - 20.0, GAME_H / 2.0 - 30.0, 48.0, white);
            }
        }

        if self.song_ended {
            let fc = rating::classify_fc(
                self.score.sicks, self.score.goods,
                self.score.bads, self.score.shits, self.score.misses,
            );
            let fc_str = match fc {
                rating::FcClassification::Sfc => " [SFC]",
                rating::FcClassification::Gfc => " [GFC]",
                rating::FcClassification::Fc => " [FC]",
                rating::FcClassification::Sdcb => " [SDCB]",
                rating::FcClassification::Clear => "",
            };
            let results = format!(
                "SONG COMPLETE!\n\nScore: {}\nAccuracy: {:.2}%{}\nGrade: {}\n\n\
                 Sicks: {} | Goods: {} | Bads: {} | Shits: {}\nCombo: {} | Misses: {}",
                self.score.score, self.score.accuracy(), fc_str, self.score.grade(),
                self.score.sicks, self.score.goods, self.score.bads, self.score.shits,
                self.score.max_combo, self.score.misses,
            );
            gpu.draw_text(&results, GAME_W / 2.0 - 180.0, 200.0, 24.0, white);
        }

        gpu.end_frame();
    }
}
