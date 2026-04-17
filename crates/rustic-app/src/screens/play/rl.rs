//! Per-frame RL glue: build an `Observation` from the live `PlayState`,
//! hand it to the harness, inject the agent's presses, and after the
//! gameplay tick feed reward + record demo line.
//!
//! Kept isolated so the feature-gate stays legible. Only compiled when
//! `rustic-app` is built with `--features rl`.

use rustic_rl::{build_observation, Action, UpcomingNote};

use super::PlayScreen;

impl PlayScreen {
    pub(super) fn rl_pre_update(&mut self) {
        // Build an observation independently of whether a harness is
        // attached — it's cheap and keeps the branch-heavy glue in one
        // place. Skip if no harness.
        if self.rl_harness.is_none() {
            return;
        }

        let song_pos = self.game.conductor.song_position;
        let bpm = self.game.conductor.bpm;
        let health = (self.game.score.health / 2.0).clamp(0.0, 1.0); // normalize
        let keys_held = self.game.keys_held;

        let play_as_opponent = self.game.play_as_opponent;
        let upcoming = self.game.notes.iter().filter_map(|n| {
            // Only playable lanes for the human/agent side.
            let playable = if play_as_opponent { !n.must_press } else { n.must_press };
            if !playable || n.was_good_hit || n.too_late {
                return None;
            }
            let dt = (n.strum_time - song_pos) as f32;
            // Drop notes that are far past — they contribute no useful signal.
            if dt < -500.0 {
                return None;
            }
            Some(UpcomingNote {
                lane: n.lane,
                time_until_hit_ms: dt,
                sustain_ms: n.sustain_length as f32,
            })
        });

        let obs = build_observation(song_pos, bpm, health, keys_held, upcoming);

        let Some(harness) = self.rl_harness.as_mut() else { return; };
        let action = match harness.decide(&obs) {
            Ok(a) => a,
            Err(e) => {
                log::warn!("rustic-rl: decide failed: {e}");
                return;
            }
        };

        if harness.control_gameplay() {
            // Diff desired presses against what the game currently sees as
            // held, and synthesize press/release events for each lane that
            // changed. This reuses the normal gameplay paths so hit
            // detection, holds, and misses all work without extra logic.
            let held = self.game.keys_held;
            for lane in 0..4 {
                match (held[lane], action.press[lane]) {
                    (false, true) => self.game.key_press(lane),
                    (true, false) => self.game.key_release(lane),
                    _ => {}
                }
            }
        }
    }

    pub(super) fn rl_post_update(&mut self) {
        let Some(harness) = self.rl_harness.as_mut() else { return; };
        let score = self.game.score.score;
        let health = (self.game.score.health / 2.0).clamp(0.0, 1.0);

        // When we're not driving gameplay, the "human action" for BC is
        // the post-update key-held vector — that's what the player had
        // their fingers on during this tick.
        let human_action = if harness.control_gameplay() {
            None
        } else {
            Some(Action { press: self.game.keys_held })
        };
        if let Err(e) = harness.end_step(score, health, human_action) {
            log::warn!("rustic-rl: end_step failed: {e}");
        }
    }

    /// Flush any buffered demo steps. Called from song end / screen drop.
    pub(super) fn rl_flush(&mut self) {
        if let Some(h) = self.rl_harness.as_mut() {
            h.flush();
        }
    }
}
