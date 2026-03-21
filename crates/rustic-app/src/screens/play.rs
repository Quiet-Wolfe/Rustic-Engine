use std::path::PathBuf;

use winit::keyboard::KeyCode;

use rustic_audio::AudioEngine;
use rustic_core::chart;
use rustic_core::conductor::Conductor;
use rustic_core::note::NoteData;
use rustic_core::rating::{self, Rating};
use rustic_core::scoring::{self, ScoreState};
use rustic_render::gpu::GpuState;

use crate::screen::Screen;

// === Psych Engine constants ===
const GAME_W: f32 = 1280.0;
const GAME_H: f32 = 720.0;
const STRUM_Y: f32 = 50.0;
const STRUM_X: f32 = 42.0;
const NOTE_WIDTH: f32 = 112.0; // 160 * 0.7
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

const RATING_DURATION: f64 = 800.0; // ms

pub struct PlayScreen {
    notes: Vec<PlayNote>,
    conductor: Conductor,
    audio: Option<AudioEngine>,
    song_started: bool,
    countdown_timer: f64,
    countdown_beat: i32,
    score: ScoreState,
    ratings: Vec<Rating>,
    keys_held: [bool; 4],
    song_speed: f64,
    song_name: String,
    difficulty: String,
    rating_popup: Option<RatingPopup>,
}

impl PlayScreen {
    pub fn new(song_name: &str, difficulty: &str) -> Self {
        Self {
            notes: Vec::new(),
            conductor: Conductor::new(100.0),
            audio: None,
            song_started: false,
            countdown_timer: 0.0,
            countdown_beat: -5,
            score: ScoreState::new(),
            ratings: Rating::load_default(),
            keys_held: [false; 4],
            song_speed: 1.0,
            song_name: song_name.to_string(),
            difficulty: difficulty.to_string(),
            rating_popup: None,
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
        if player {
            base + GAME_W / 2.0
        } else {
            base
        }
    }

    fn try_hit_note(&mut self, lane: usize) {
        let mut best_idx = None;
        let mut best_time = f64::MAX;
        let max_window = 166.0; // shit window

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
                    judgment.score,
                    judgment.rating_mod,
                    judgment.health_gain,
                    &judgment.name,
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

}

impl Screen for PlayScreen {
    fn init(&mut self, _gpu: &GpuState) {
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

        let sections: Vec<(bool, f64, f64)> = parsed
            .song
            .notes
            .iter()
            .map(|s| {
                let bpm = if s.change_bpm && s.bpm > 0.0 { s.bpm } else { parsed.song.bpm };
                (s.change_bpm, bpm, s.section_beats)
            })
            .collect();
        self.conductor.map_bpm_changes(parsed.song.bpm, sections);

        let mut notes: Vec<PlayNote> = parsed
            .notes
            .into_iter()
            .map(|nd| PlayNote { data: nd, y_pos: 0.0 })
            .collect();
        notes.sort_by(|a, b| a.data.strum_time.partial_cmp(&b.data.strum_time).unwrap());
        self.notes = notes;

        let song_dir = format!(
            "references/FNF-PsychEngine/assets/base_game/songs/{}",
            self.song_name
        );
        let mut audio = AudioEngine::new();
        audio.load_inst(&PathBuf::from(format!("{}/Inst.ogg", song_dir)));
        audio.load_vocals(&PathBuf::from(format!("{}/Voices-Player.ogg", song_dir)));
        audio.load_opponent_vocals(&PathBuf::from(format!("{}/Voices-Opponent.ogg", song_dir)));
        self.audio = Some(audio);

        self.conductor.song_position = -self.conductor.crochet * 5.0;
        self.countdown_timer = self.conductor.crochet * 5.0;

        log::info!(
            "PlayScreen: {} ({}) - {} notes, speed {:.1}, BPM {:.0}",
            self.song_name,
            self.difficulty,
            self.notes.len(),
            self.song_speed,
            self.conductor.bpm,
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

        // Tick rating popup
        if let Some(popup) = &mut self.rating_popup {
            popup.timer -= dt_ms;
            if popup.timer <= 0.0 {
                self.rating_popup = None;
            }
        }

        if !self.song_started {
            self.conductor.song_position += dt_ms;
            self.countdown_timer -= dt_ms;
            // Track countdown beats (-4, -3, -2, -1 → "3", "2", "1", "Go!")
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

        // Update note positions and detect misses
        for pn in &mut self.notes {
            if pn.data.was_good_hit || pn.data.too_late {
                // Keep updating y_pos for hit hold notes so the tail scrolls
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

            // Auto-hit opponent notes
            if !pn.data.must_press && self.conductor.song_position >= pn.data.strum_time {
                pn.data.was_good_hit = true;
            }

            // Miss detection
            if pn.data.must_press
                && self.conductor.song_position - pn.data.strum_time > KILL_OFFSET_MS
            {
                pn.data.too_late = true;
                if !pn.data.is_sustain() {
                    self.score.note_miss(scoring::HEALTH_MISS);
                    self.rating_popup = Some(RatingPopup {
                        text: "MISS".into(),
                        combo: 0,
                        timer: RATING_DURATION,
                    });
                    if let Some(audio) = &mut self.audio {
                        audio.mute_player_vocals();
                    }
                }
            }
        }
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        let white = [1.0, 1.0, 1.0, 1.0];
        let gray = [0.3, 0.3, 0.3, 1.0];
        let lane_colors: [[f32; 4]; 4] = [
            [0.7, 0.2, 0.8, 1.0], // purple - left
            [0.2, 0.4, 0.9, 1.0], // blue - down
            [0.2, 0.8, 0.2, 1.0], // green - up
            [0.8, 0.2, 0.2, 1.0], // red - right
        ];

        // Strum line receptors
        for player in [false, true] {
            for lane in 0..4 {
                let x = Self::strum_x(lane, player);
                let color = if player && self.keys_held[lane] {
                    [1.0, 1.0, 1.0, 0.8]
                } else {
                    gray
                };
                gpu.push_colored_quad(x, STRUM_Y, NOTE_WIDTH - 4.0, NOTE_WIDTH - 4.0, color);
            }
        }

        // Notes
        let note_size = NOTE_WIDTH - 4.0;
        for pn in &self.notes {
            let x = Self::strum_x(pn.data.lane, pn.data.must_press);
            let color = lane_colors[pn.data.lane];

            // Sustain tail (draw for both hit and unhit hold notes)
            if pn.data.sustain_length > 0.0 {
                let tail_full_h =
                    (SCROLL_SPEED_FACTOR * pn.data.sustain_length * self.song_speed) as f32;
                let tail_top = pn.y_pos + note_size;
                let tail_bot = tail_top + tail_full_h;

                // Clip: only draw the portion below the strum line bottom
                let clip_y = STRUM_Y + note_size;
                let visible_top = tail_top.max(clip_y);
                let visible_h = tail_bot - visible_top;

                if visible_h > 1.0 && visible_top < GAME_H + tail_full_h {
                    let tail_x = x + NOTE_WIDTH * 0.35;
                    let tail_w = NOTE_WIDTH * 0.3;
                    let mut tail_color = color;
                    tail_color[3] = 0.6;
                    gpu.push_colored_quad(tail_x, visible_top, tail_w, visible_h, tail_color);
                }
            }

            // Note head — skip if already hit or missed
            if pn.data.was_good_hit || pn.data.too_late {
                continue;
            }
            if pn.y_pos < -NOTE_WIDTH || pn.y_pos > GAME_H + NOTE_WIDTH {
                continue;
            }
            gpu.push_colored_quad(x, pn.y_pos, note_size, note_size, color);
        }

        // Health bar
        let health_pct = self.score.health_percent();
        // Background (red = opponent side)
        gpu.push_colored_quad(
            HEALTH_BAR_X - 2.0, HEALTH_BAR_Y - 2.0,
            HEALTH_BAR_W + 4.0, HEALTH_BAR_H + 4.0,
            [0.0, 0.0, 0.0, 1.0],
        );
        // Opponent side (full bar, red)
        gpu.push_colored_quad(
            HEALTH_BAR_X, HEALTH_BAR_Y,
            HEALTH_BAR_W, HEALTH_BAR_H,
            [0.8, 0.1, 0.1, 1.0],
        );
        // Player side (green, from right)
        let player_w = HEALTH_BAR_W * health_pct;
        gpu.push_colored_quad(
            HEALTH_BAR_X + HEALTH_BAR_W - player_w, HEALTH_BAR_Y,
            player_w, HEALTH_BAR_H,
            [0.2, 0.8, 0.2, 1.0],
        );

        // Score text below health bar
        let grade = self.score.grade();
        let score_text = format!(
            "Score: {} | Misses: {} | Acc: {:.2}% [{}]",
            self.score.score, self.score.misses, self.score.accuracy(), grade,
        );
        gpu.draw_text(&score_text, HEALTH_BAR_X, HEALTH_BAR_Y + HEALTH_BAR_H + 6.0, 16.0, white);

        // Rating popup (center screen)
        if let Some(popup) = &self.rating_popup {
            let alpha = (popup.timer / RATING_DURATION) as f32;
            let rating_color = [1.0, 1.0, 1.0, alpha];
            let popup_text = format!("{}\n{}", popup.text, popup.combo);
            gpu.draw_text(&popup_text, GAME_W / 2.0 - 40.0, GAME_H / 2.0 - 20.0, 32.0, rating_color);
        }

        // Countdown text
        if !self.song_started {
            let countdown_text = match self.countdown_beat {
                -4 => Some("3"),
                -3 => Some("2"),
                -2 => Some("1"),
                -1 => Some("Go!"),
                _ => None,
            };
            if let Some(text) = countdown_text {
                gpu.draw_text(text, GAME_W / 2.0 - 20.0, GAME_H / 2.0 - 30.0, 48.0, white);
            }
        }

        gpu.present_no_texture();
    }
}
