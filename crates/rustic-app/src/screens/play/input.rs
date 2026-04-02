use winit::keyboard::KeyCode;

use super::{PlayScreen, DeathPhase};

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

        // Pause menu input (must come before toggle to prevent Enter from unpausing)
        if self.paused {
            const PAUSE_ITEMS: usize = 4; // Resume, Restart, Skip To, Exit
            match key {
                KeyCode::Escape => {
                    // Escape always resumes
                    self.paused = false;
                    if let Some(audio) = &mut self.audio {
                        if self.game.song_started { audio.play(); }
                    }
                }
                KeyCode::ArrowUp | KeyCode::KeyW => {
                    if self.pause_selection > 0 { self.pause_selection -= 1; }
                }
                KeyCode::ArrowDown | KeyCode::KeyS => {
                    if self.pause_selection < PAUSE_ITEMS - 1 { self.pause_selection += 1; }
                }
                // Left/Right adjust skip target when on the Skip To item
                KeyCode::ArrowLeft | KeyCode::KeyA => {
                    if self.pause_selection == 2 {
                        self.skip_target_ms = (self.skip_target_ms - 5000.0).max(0.0);
                    }
                }
                KeyCode::ArrowRight | KeyCode::KeyD => {
                    if self.pause_selection == 2 {
                        self.skip_target_ms += 5000.0;
                    }
                }
                KeyCode::Enter | KeyCode::Space => {
                    match self.pause_selection {
                        0 => {
                            self.paused = false;
                            if let Some(audio) = &mut self.audio {
                                if self.game.song_started { audio.play(); }
                            }
                        }
                        1 => self.wants_restart = true,
                        2 => {
                            // Skip To: jump to target time and resume
                            let target = self.skip_target_ms;
                            self.skip_to(target);
                            self.paused = false;
                            if let Some(audio) = &mut self.audio {
                                if self.game.song_started { audio.play(); }
                            }
                        }
                        3 => self.game.song_ended = true,
                        _ => {}
                    }
                }
                _ => {}
            }
            return;
        }

        // Pause toggle (Enter or Escape when not paused)
        if key == KeyCode::Escape || key == KeyCode::Enter {
            if self.game.song_started || self.game.countdown_timer > 0.0 {
                self.paused = true;
                self.pause_selection = 0;
                self.skip_target_ms = self.game.conductor.song_position.max(0.0);
                if let Some(audio) = &mut self.audio {
                    audio.pause();
                    if let Some(sfx) = self.paths.sound("cancelMenu") {
                        audio.play_sound(&sfx, 0.6);
                    }
                }
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
        if let Some(lane) = Self::key_to_lane(key) {
            self.game.key_press(lane);
        }
    }

    /// Skip to an absolute position in the song (in milliseconds).
    fn skip_to(&mut self, target: f64) {
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
            if note.strum_time + note.sustain_length < target && !note.was_good_hit && !note.too_late {
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
                if let Some(sfx) = self.paths.music("gameOverEnd") {
                    audio.play_sound(&sfx, 1.0);
                }
            }
        }
    }
}
