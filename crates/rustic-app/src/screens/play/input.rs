use winit::keyboard::KeyCode;

use super::{DeathPhase, PlayScreen};
use crate::screens::options;

impl PlayScreen {
    pub(super) fn handle_key_inner(&mut self, key: KeyCode) {
        // Death screen input
        if let Some(death) = &self.death {
            match key {
                KeyCode::Enter | KeyCode::Space => {
                    if death.phase != DeathPhase::Confirm {
                        self.start_death_confirm();
                    }
                }
                KeyCode::Escape => self.game.song_ended = true,
                _ => {}
            }
            return;
        }

        if let Some(super::CutsceneState::Video {
            skippable,
            blocks_gameplay,
            ..
        }) = &self.cutscene
        {
            if *blocks_gameplay {
                if *skippable
                    && (key == KeyCode::Enter || key == KeyCode::Escape || key == KeyCode::Space)
                {
                    self.skip_cutscene();
                }
                return;
            }
        }

        if let Some(menu) = &mut self.options_menu {
            if key == KeyCode::Escape {
                menu.save();
                self.downscroll = menu.prefs.downscroll;
                self.lane_keys = super::lane_keys_from_prefs(&menu.prefs);
                self.options_menu = None;
                self.pending_options_open = false;
            } else {
                options::handle_input(menu, key);
            }
            return;
        }

        if self.pause_menu.is_some() {
            self.handle_pause_input(key);
            return;
        }

        // Pause toggle (Enter or Escape when not paused)
        if key == KeyCode::Escape || key == KeyCode::Enter {
            if self.game.song_started || self.game.countdown_timer > 0.0 {
                self.enter_pause();
                return;
            }
        }

        // Time skip (debug): PageUp = +5s, PageDown = -5s
        match key {
            KeyCode::PageUp => {
                self.skip_time(5000.0);
                return;
            }
            KeyCode::PageDown => {
                self.skip_time(-5000.0);
                return;
            }
            _ => {}
        }

        // Forward gameplay input to PlayState
        if let Some(lane) = self.key_to_lane(key) {
            #[cfg(feature = "rl")]
            if let Some(harness) = &self.rl_harness {
                if harness.control_gameplay() {
                    return; // Ignore human input when agent is driving
                }
            }
            self.game.key_press(lane);
        }
    }

    /// Skip to an absolute position in the song (in milliseconds).
    pub(super) fn skip_to(&mut self, target: f64) {
        let going_back = target < self.game.conductor.song_position;
        self.game.conductor.song_position = target;

        // If song hasn't started yet but we're skipping past 0, force-start it
        if !self.game.song_started && target >= 0.0 {
            self.game.song_started = true;
            self.game.countdown_timer = 0.0;
            if let Some(audio) = &mut self.audio {
                audio.play();
            }
        }

        // Seek audio
        if self.game.song_started {
            if let Some(audio) = &mut self.audio {
                audio.seek(target);
            }
        }

        // Mark notes in the past as too_late (skip misses so we don't die)
        for note in &mut self.game.notes {
            if note.strum_time + note.sustain_length < target
                && !note.was_good_hit
                && !note.too_late
            {
                note.too_late = true;
            }
        }

        // Advance/rewind event index
        if !going_back {
            while self.event_index < self.chart_events.len()
                && self.chart_events[self.event_index].strum_time <= target
            {
                self.event_index += 1;
            }
        } else {
            while self.event_index > 0
                && self.chart_events[self.event_index - 1].strum_time > target
            {
                self.event_index -= 1;
            }
        }

        // Update section index
        if going_back {
            self.game.cur_section = 0;
        }
        while self.game.cur_section + 1 < self.game.sections.len()
            && self.game.sections[self.game.cur_section + 1].start_time <= target
        {
            self.game.cur_section += 1;
        }

        // Reset health so we don't die from accumulated misses
        self.game.score.health = 1.0;

        // Force custom health bar visible if we skipped past beat 16
        let beat16_time = self.game.conductor.crochet * 16.0;
        if target >= beat16_time {
            if let Some(chb) = &mut self.custom_healthbar {
                if !chb.visible {
                    chb.fade_in();
                }
            }
        }

        log::info!("Skipped to {:.0}ms", target);
    }

    /// Skip forward or backward by a relative offset.
    fn skip_time(&mut self, offset_ms: f64) {
        let target = (self.game.conductor.song_position + offset_ms).max(0.0);
        self.skip_to(target);
    }

    /// Transition death screen to confirm phase (retry).
    pub(super) fn start_death_confirm(&mut self) {
        if let Some(death) = &mut self.death {
            death.phase = DeathPhase::Confirm;
            death.fade_alpha = 0.0;
            death.character.play_anim("deathConfirm", true);
            if let Some(audio) = &mut self.audio {
                audio.stop_loop_music();
                if let Some(sfx) = self.paths.music(&self.death_end_name) {
                    audio.play_sound(&sfx, 1.0);
                }
            }
        }
    }
}
