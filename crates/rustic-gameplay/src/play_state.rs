use rustic_core::conductor::Conductor;
use rustic_core::note::NoteData;
use rustic_core::rating::{self, Rating};
use rustic_core::scoring::{self, ScoreState};

use crate::events::GameEvent;

const KILL_OFFSET_MS: f64 = 350.0;

/// Section info for camera targeting.
pub struct SectionInfo {
    pub must_hit: bool,
    pub start_time: f64,
}

/// Core game state — no rendering, no audio. Emits events.
pub struct PlayState {
    pub notes: Vec<NoteData>,
    pub conductor: Conductor,
    pub score: ScoreState,
    pub ratings: Vec<Rating>,
    pub keys_held: [bool; 4],
    pub play_as_opponent: bool,
    pub song_speed: f64,
    pub base_song_speed: f64,
    pub song_started: bool,
    pub song_ended: bool,
    pub dead: bool,

    // Strum confirm timers (ms elapsed since confirm started, 0 = idle)
    pub player_confirm: [f64; 4],
    pub opponent_confirm: [f64; 4],

    // Countdown
    pub countdown_timer: f64,
    countdown_beat: i32,

    // Section tracking
    pub sections: Vec<SectionInfo>,
    pub cur_section: usize,
    last_beat: i32,
    last_step: i32,

    // Event buffer
    events: Vec<GameEvent>,
}

impl PlayState {
    pub fn new(bpm: f64) -> Self {
        Self {
            notes: Vec::new(),
            conductor: Conductor::new(bpm),
            score: ScoreState::new(),
            ratings: Rating::load_default(),
            keys_held: [false; 4],
            play_as_opponent: false,
            song_speed: 1.0,
            base_song_speed: 1.0,
            song_started: false,
            song_ended: false,
            dead: false,
            player_confirm: [0.0; 4],
            opponent_confirm: [0.0; 4],
            countdown_timer: 0.0,
            countdown_beat: -5,
            sections: Vec::new(),
            cur_section: 0,
            last_beat: -999,
            last_step: -999,
            events: Vec::new(),
        }
    }

    /// Take all pending events (clears the buffer).
    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }

    /// Handle a key press. Returns the lane if it was a gameplay lane.
    pub fn key_press(&mut self, lane: usize) {
        if self.dead { return; }
        if self.keys_held[lane] { return; }
        self.keys_held[lane] = true;
        self.try_hit_note(lane);
    }

    /// Handle a key release.
    pub fn key_release(&mut self, lane: usize) {
        self.keys_held[lane] = false;
    }

    fn try_hit_note(&mut self, lane: usize) {
        let mut best_idx = None;
        let mut best_time = f64::MAX;
        let max_window = 166.0;

        for (i, nd) in self.notes.iter().enumerate() {
            let note_is_playable = if self.play_as_opponent { !nd.must_press } else { nd.must_press };
            if !note_is_playable || nd.lane != lane || nd.was_good_hit || nd.too_late {
                continue;
            }
            let diff = (nd.strum_time - self.conductor.song_position).abs();
            if diff <= max_window && nd.strum_time < best_time {
                best_time = nd.strum_time;
                best_idx = Some(i);
            }
        }

        if let Some(idx) = best_idx {
            let diff = (self.notes[idx].strum_time - self.conductor.song_position).abs();
            if let Some(judgment) = rating::judge_note(&self.ratings, diff) {
                self.notes[idx].was_good_hit = true;
                let kind = &self.notes[idx].kind;
                let type_str = kind.as_type_str().to_string();
                let hit_causes_miss = kind.is_harmful();

                if hit_causes_miss {
                    // Harmful note: damage player, no score gain
                    let dmg = kind.hit_damage();
                    let config = kind.custom_config();
                    let sfx = config.as_ref().and_then(|c| c.hit_sfx.clone());
                    let drain_pct = config.as_ref().map(|c| c.health_drain_pct).unwrap_or(0.0);
                    let death_safe = config.as_ref().map(|c| c.drain_death_safe).unwrap_or(false);

                    if drain_pct > 0.0 {
                        // Sliding drain: we don't damage here — the visual layer
                        // will animate the health bar down over time.
                        // Emit the drain request so the app layer handles it.
                        self.events.push(GameEvent::HarmfulNoteHit {
                            sfx_path: sfx.unwrap_or_default(),
                            drain_pct,
                            death_safe,
                        });
                    } else {
                        // Instant damage
                        self.score.change_health(-dmg);
                    }
                    if self.play_as_opponent {
                        self.opponent_confirm[lane] = f64::MIN_POSITIVE;
                    } else {
                        self.player_confirm[lane] = f64::MIN_POSITIVE;
                    }

                    self.events.push(GameEvent::NoteHit {
                        lane,
                        rating: judgment.name.clone(),
                        combo: self.score.combo,
                        score: 0,
                        note_type: type_str,
                        is_sustain: self.notes[idx].sustain_length > 0.0,
                        members_index: idx,
                        hit_causes_miss: true,
                    });
                    self.events.push(GameEvent::MuteVocals);
                } else {
                    self.score.note_hit(
                        judgment.score, judgment.rating_mod,
                        judgment.health_gain, &judgment.name,
                    );
                    if self.play_as_opponent {
                        self.opponent_confirm[lane] = f64::MIN_POSITIVE;
                    } else {
                        self.player_confirm[lane] = f64::MIN_POSITIVE;
                    }

                    self.events.push(GameEvent::NoteHit {
                        lane,
                        rating: judgment.name.clone(),
                        combo: self.score.combo,
                        score: judgment.score,
                        note_type: type_str,
                        is_sustain: self.notes[idx].sustain_length > 0.0,
                        members_index: idx,
                        hit_causes_miss: false,
                    });
                    self.events.push(GameEvent::UnmuteVocals);
                }
            }
        }
    }

    /// Main update tick. Call with dt in seconds and optional audio position for sync.
    pub fn update(&mut self, dt: f32, audio_position_ms: Option<f64>, audio_finished: bool) {
        if self.dead { return; }

        let dt_ms = dt as f64 * 1000.0;

        // Strum confirm timers
        let confirm_dur = self.conductor.crochet / 4.0 * 1.25;
        for i in 0..4 {
            if self.player_confirm[i] > 0.0 {
                self.player_confirm[i] += dt_ms;
                let is_playable = !self.play_as_opponent;
                if self.player_confirm[i] > confirm_dur && (!is_playable || !self.keys_held[i]) {
                    self.player_confirm[i] = 0.0;
                }
            }
            if self.opponent_confirm[i] > 0.0 {
                self.opponent_confirm[i] += dt_ms;
                let is_playable = self.play_as_opponent;
                if self.opponent_confirm[i] > confirm_dur && (!is_playable || !self.keys_held[i]) {
                    self.opponent_confirm[i] = 0.0;
                }
            }
        }

        // Countdown / song position
        if !self.song_started {
            self.conductor.song_position += dt_ms;
            self.countdown_timer -= dt_ms;
            let beat = (self.conductor.song_position / self.conductor.crochet).floor() as i32;
            if beat != self.countdown_beat {
                self.countdown_beat = beat;
                let swag = beat + 4;
                if (0..=3).contains(&swag) {
                    self.events.push(GameEvent::CountdownBeat { swag });
                }
            }
            if self.countdown_timer <= 0.0 {
                self.events.push(GameEvent::SongStart);
                self.song_started = true;
                self.conductor.song_position = 0.0;
            }
        } else if let Some(audio_pos) = audio_position_ms {
            let diff = audio_pos - self.conductor.song_position;
            if diff.abs() > 200.0 {
                // Only hard-snap on very large desync (seek, lag spike)
                self.conductor.song_position = audio_pos;
            } else {
                // Smooth correction: advance by dt plus a fraction of the drift
                // Higher correction factor (0.15) keeps sync tight without visible jumps
                self.conductor.song_position += dt_ms + diff * 0.15;
            }
        }

        // Note processing
        for i in 0..self.notes.len() {
            if self.notes[i].was_good_hit || self.notes[i].too_late {
                continue;
            }

            let note_is_playable = if self.play_as_opponent { !self.notes[i].must_press } else { self.notes[i].must_press };

            // Opponent auto-hit
            if !note_is_playable && self.conductor.song_position >= self.notes[i].strum_time {
                self.notes[i].was_good_hit = true;
                if self.play_as_opponent {
                    self.player_confirm[self.notes[i].lane] = f64::MIN_POSITIVE;
                } else {
                    self.opponent_confirm[self.notes[i].lane] = f64::MIN_POSITIVE;
                }
                let type_str = self.notes[i].kind.as_type_str().to_string();
                let hit_causes_miss = self.notes[i].kind.is_harmful();
                self.events.push(GameEvent::OpponentNoteHit {
                    lane: self.notes[i].lane,
                    note_type: type_str,
                    is_sustain: self.notes[i].sustain_length > 0.0,
                    members_index: i,
                    hit_causes_miss,
                });
            }

            // Player miss
            if note_is_playable
                && self.conductor.song_position - self.notes[i].strum_time > KILL_OFFSET_MS
            {
                self.notes[i].too_late = true;
                let type_str = self.notes[i].kind.as_type_str().to_string();
                let ignored = self.notes[i].kind.should_ignore_miss();

                if !ignored {
                    self.score.note_miss(scoring::HEALTH_MISS);
                    self.events.push(GameEvent::MuteVocals);
                    self.events.push(GameEvent::PlayMissSound);
                }
                let lane = self.notes[i].lane;
                self.events.push(GameEvent::NoteMiss {
                    lane,
                    note_type: type_str,
                    members_index: i,
                    ignored,
                });
            }
        }

        // Hold notes: health gain/drain
        let step_ms = self.conductor.crochet / 4.0;
        let confirm_cycle_ms = 4.0 * (1000.0 / 24.0);
        for i in 0..self.notes.len() {
            if self.notes[i].sustain_length <= 0.0 { continue; }
            let end_time = self.notes[i].strum_time + self.notes[i].sustain_length;
            if self.conductor.song_position > end_time { continue; }

            let note_is_playable = if self.play_as_opponent { !self.notes[i].must_press } else { self.notes[i].must_press };

            if self.notes[i].was_good_hit {
                let lane = self.notes[i].lane;
                if note_is_playable && self.keys_held[lane] {
                    let ticks = dt_ms / step_ms;
                    self.score.change_health(scoring::HEALTH_HOLD_TICK * ticks as f32);
                    if self.play_as_opponent {
                        if self.opponent_confirm[lane] <= 0.0 || self.opponent_confirm[lane] >= confirm_cycle_ms {
                            self.opponent_confirm[lane] = f64::MIN_POSITIVE;
                        }
                    } else {
                        if self.player_confirm[lane] <= 0.0 || self.player_confirm[lane] >= confirm_cycle_ms {
                            self.player_confirm[lane] = f64::MIN_POSITIVE;
                        }
                    }
                } else if note_is_playable && !self.keys_held[lane] {
                    let ticks = dt_ms / step_ms;
                    self.score.change_health(-scoring::HEALTH_MISS * ticks as f32);
                } else if !note_is_playable {
                    if self.play_as_opponent {
                        if self.player_confirm[lane] <= 0.0 || self.player_confirm[lane] >= confirm_cycle_ms {
                            self.player_confirm[lane] = f64::MIN_POSITIVE;
                        }
                    } else {
                        if self.opponent_confirm[lane] <= 0.0 || self.opponent_confirm[lane] >= confirm_cycle_ms {
                            self.opponent_confirm[lane] = f64::MIN_POSITIVE;
                        }
                    }
                }
            } else if self.notes[i].too_late && note_is_playable {
                let ticks = dt_ms / step_ms;
                self.score.change_health(-scoring::HEALTH_MISS * ticks as f32);
            }
        }

        // Step detection
        let step = self.conductor.cur_step();
        if step != self.last_step {
            self.events.push(GameEvent::StepHit { step });
        }
        self.last_step = step;

        // Beat detection
        let beat = self.conductor.cur_beat();
        if beat != self.last_beat {
            self.events.push(GameEvent::BeatHit { beat });
        }
        self.last_beat = beat;

        // Section change detection
        if !self.sections.is_empty() {
            let mut sec_idx = self.cur_section;
            while sec_idx + 1 < self.sections.len()
                && self.conductor.song_position >= self.sections[sec_idx + 1].start_time
            {
                sec_idx += 1;
            }
            if sec_idx != self.cur_section {
                self.cur_section = sec_idx;
                self.events.push(GameEvent::SectionChange {
                    index: sec_idx,
                    must_hit: self.sections[sec_idx].must_hit,
                });
            }
        }

        // Death check
        if !self.dead && self.score.health <= 0.0 {
            self.dead = true;
            self.events.push(GameEvent::Death);
        }

        // Song end check
        if self.song_started && !self.song_ended && !self.dead && audio_finished {
            self.song_ended = true;
            self.events.push(GameEvent::SongEnd);
        }
    }

    /// Note Y position for rendering (scroll calculation).
    /// `downscroll`: if true, notes scroll upward toward a bottom strum line.
    pub fn note_y(&self, strum_time: f64, strum_y: f32, downscroll: bool) -> f32 {
        let dist = (0.45 * (self.conductor.song_position - strum_time) * self.song_speed) as f32;
        if downscroll { strum_y + dist } else { strum_y - dist }
    }
}
