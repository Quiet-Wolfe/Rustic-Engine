use std::path::{Path, PathBuf};

use winit::keyboard::KeyCode;

use rustic_audio::AudioEngine;
use rustic_core::chart;
use rustic_core::conductor::Conductor;
use rustic_core::note::NoteData;
use rustic_core::rating::{self, Rating};
use rustic_core::scoring::{self, ScoreState};
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::SpriteAtlas;

use crate::screen::Screen;

// === Psych Engine constants ===
const GAME_W: f32 = 1280.0;
const GAME_H: f32 = 720.0;
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

// Note atlas animation name prefixes per lane (left/down/up/right)
// Use the fully-colored strum arrow frames for scrolling notes too,
// since the purple/blue/green/red frames are designed for RGB recoloring shader.
const NOTE_ANIMS: [&str; 4] = ["arrowLEFT", "arrowDOWN", "arrowUP", "arrowRIGHT"];
const STRUM_ANIMS: [&str; 4] = ["arrowLEFT", "arrowDOWN", "arrowUP", "arrowRIGHT"];
const PRESS_ANIMS: [&str; 4] = ["left press", "down press", "up press", "right press"];
const CONFIRM_ANIMS: [&str; 4] = ["left confirm", "down confirm", "up confirm", "right confirm"];

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
    song_speed: f64,
    song_name: String,
    difficulty: String,
    rating_popup: Option<RatingPopup>,
    note_assets: Option<NoteAssets>,
    icon_bf: Option<GpuTexture>,
    icon_dad: Option<GpuTexture>,
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
            song_speed: 1.0,
            song_name: song_name.to_string(),
            difficulty: difficulty.to_string(),
            rating_popup: None,
            note_assets: None,
            icon_bf: None,
            icon_dad: None,
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
                if let Some(audio) = &mut self.audio {
                    audio.unmute_player_vocals();
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
        let pressed = player && self.keys_held[lane];
        let anim = if pressed { PRESS_ANIMS[lane] } else { STRUM_ANIMS[lane] };
        self.draw_note_sprite(gpu, anim, x, STRUM_Y, NOTE_SCALE);
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

        // Register note animations
        for prefix in NOTE_ANIMS.iter().chain(STRUM_ANIMS.iter())
            .chain(PRESS_ANIMS.iter()).chain(CONFIRM_ANIMS.iter())
        {
            atlas.add_by_prefix(prefix, prefix);
        }

        self.note_assets = Some(NoteAssets {
            tex_w: note_tex.width as f32,
            tex_h: note_tex.height as f32,
            texture: note_tex,
            atlas,
        });

        // Load health bar icons
        let icon_path = Path::new("references/FNF-PsychEngine/assets/shared/images/icons");
        self.icon_bf = Some(gpu.load_texture_from_path(&icon_path.join("icon-bf.png")));
        self.icon_dad = Some(gpu.load_texture_from_path(&icon_path.join("icon-dad.png")));

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

            if !pn.data.must_press && self.conductor.song_position >= pn.data.strum_time {
                pn.data.was_good_hit = true;
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
                }
            }
        }

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

        // === Batch 1: Hold tails (colored quads, behind everything) ===
        let note_size = NOTE_WIDTH - 4.0;
        let lane_colors: [[f32; 4]; 4] = [
            [0.7, 0.2, 0.8, 0.6],
            [0.2, 0.4, 0.9, 0.6],
            [0.2, 0.8, 0.2, 0.6],
            [0.8, 0.2, 0.2, 0.6],
        ];

        for pn in &self.notes {
            if pn.data.sustain_length > 0.0 {
                let x = Self::strum_x(pn.data.lane, pn.data.must_press);
                let tail_full_h =
                    (SCROLL_SPEED_FACTOR * pn.data.sustain_length * self.song_speed) as f32;
                let tail_top = pn.y_pos + note_size;
                let tail_bot = tail_top + tail_full_h;
                let clip_y = STRUM_Y + note_size;
                let visible_top = tail_top.max(clip_y);
                let visible_h = tail_bot - visible_top;

                if visible_h > 1.0 && visible_top < GAME_H + tail_full_h {
                    let tail_x = x + NOTE_WIDTH * 0.35;
                    let tail_w = NOTE_WIDTH * 0.3;
                    gpu.push_colored_quad(tail_x, visible_top, tail_w, visible_h, lane_colors[pn.data.lane]);
                }
            }
        }
        gpu.draw_batch(None); // white texture for colored quads

        // === Batch 2: Note atlas sprites (strum receptors + note heads) ===
        for player in [false, true] {
            for lane in 0..4 {
                self.draw_strum(gpu, lane, player);
            }
        }

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
            [0.8, 0.1, 0.1, 1.0],
        );
        let player_w = HEALTH_BAR_W * health_pct;
        gpu.push_colored_quad(
            HEALTH_BAR_X + HEALTH_BAR_W - player_w, HEALTH_BAR_Y,
            player_w, HEALTH_BAR_H,
            [0.2, 0.8, 0.2, 1.0],
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
