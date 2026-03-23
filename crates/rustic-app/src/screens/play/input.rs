use std::path::Path;

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

        // Pause toggle
        if key == KeyCode::Escape || (key == KeyCode::Enter && !self.game.song_ended) {
            if self.paused {
                self.paused = false;
                if let Some(audio) = &mut self.audio {
                    if self.game.song_started { audio.play(); }
                }
                return;
            } else if self.game.song_started || self.game.countdown_timer > 0.0 {
                self.paused = true;
                self.pause_selection = 0;
                if let Some(audio) = &mut self.audio {
                    audio.pause();
                    let sfx = Path::new("references/FNF-PsychEngine/assets/shared/sounds/cancelMenu.ogg");
                    audio.play_sound(sfx, 0.6);
                }
                return;
            }
        }

        // Pause menu navigation
        if self.paused {
            match key {
                KeyCode::ArrowUp | KeyCode::KeyW => {
                    if self.pause_selection > 0 { self.pause_selection -= 1; }
                }
                KeyCode::ArrowDown | KeyCode::KeyS => {
                    if self.pause_selection < 2 { self.pause_selection += 1; }
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
                        2 => self.game.song_ended = true,
                        _ => {}
                    }
                }
                _ => {}
            }
            return;
        }

        // Forward gameplay input to PlayState
        if let Some(lane) = Self::key_to_lane(key) {
            self.game.key_press(lane);
        }
    }

    /// Transition death screen to confirm phase (retry).
    pub(super) fn start_death_confirm(&mut self) {
        if let Some(death) = &mut self.death {
            death.phase = DeathPhase::Confirm;
            death.fade_alpha = 0.0;
            death.character.play_anim("deathConfirm", true);
            if let Some(audio) = &mut self.audio {
                audio.stop_loop_music();
                let sfx = Path::new("references/FNF-PsychEngine/assets/shared/music/gameOverEnd.ogg");
                audio.play_sound(sfx, 1.0);
            }
        }
    }
}
