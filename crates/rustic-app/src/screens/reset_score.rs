use std::path::PathBuf;

use rustic_core::highscore::HighscoreStore;
use rustic_core::paths::AssetPaths;
use rustic_render::gpu::GpuState;
use rustic_render::health_icon::{HealthIcon, IconState};
use winit::keyboard::KeyCode;

#[derive(Debug, Clone)]
pub enum ResetScoreTarget {
    Song {
        song_id: String,
        label: String,
        difficulty: String,
        character: String,
    },
    Week {
        week_id: String,
        label: String,
        difficulty: String,
    },
}

pub enum ResetScoreAction {
    None,
    Close,
    Confirmed,
}

pub struct ResetScoreModal {
    target: ResetScoreTarget,
    on_yes: bool,
    alpha: f32,
    icon_path: Option<PathBuf>,
    icon: Option<HealthIcon>,
}

impl ResetScoreModal {
    pub fn song(song_id: String, label: String, difficulty: String, character: String) -> Self {
        let paths = AssetPaths::platform_default();
        let icon_path = paths.health_icon(&character).or_else(|| paths.health_icon("face"));
        Self {
            target: ResetScoreTarget::Song {
                song_id,
                label,
                difficulty,
                character,
            },
            on_yes: false,
            alpha: 0.0,
            icon_path,
            icon: None,
        }
    }

    pub fn week(week_id: String, label: String, difficulty: String) -> Self {
        Self {
            target: ResetScoreTarget::Week {
                week_id,
                label,
                difficulty,
            },
            on_yes: false,
            alpha: 0.0,
            icon_path: None,
            icon: None,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.alpha = (self.alpha + dt * 1.5).min(0.6);
    }

    pub fn handle_key(&mut self, key: KeyCode) -> ResetScoreAction {
        match key {
            KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::KeyA | KeyCode::KeyD => {
                self.on_yes = !self.on_yes;
                ResetScoreAction::None
            }
            KeyCode::Escape | KeyCode::Backspace => ResetScoreAction::Close,
            KeyCode::Enter | KeyCode::Space => {
                if self.on_yes {
                    ResetScoreAction::Confirmed
                } else {
                    ResetScoreAction::Close
                }
            }
            _ => ResetScoreAction::None,
        }
    }

    pub fn apply(&self, highscores: &mut HighscoreStore) {
        match &self.target {
            ResetScoreTarget::Song {
                song_id,
                difficulty,
                ..
            } => highscores.reset_score(song_id, difficulty),
            ResetScoreTarget::Week {
                week_id,
                difficulty,
                ..
            } => highscores.reset_week(week_id, difficulty),
        }
        highscores.save();
    }

    pub fn draw(&mut self, gpu: &mut GpuState) {
        if self.icon.is_none() {
            if let Some(path) = &self.icon_path {
                self.icon = Some(HealthIcon::load(gpu, path, false));
            }
        }

        gpu.push_colored_quad(0.0, 0.0, 1280.0, 720.0, [0.0, 0.0, 0.0, self.alpha]);
        gpu.draw_batch(None);

        let white = [1.0, 1.0, 1.0, 1.0];
        let gray = [0.8, 0.8, 0.8, 1.0];
        let red = [1.0, 0.4, 0.4, 1.0];

        gpu.draw_text("Reset the score of", 420.0, 190.0, 34.0, white);
        gpu.draw_text(&self.target_label(), 380.0, 280.0, 34.0, white);
        gpu.draw_text("If you do, your score and accuracy will be deleted.", 250.0, 350.0, 24.0, gray);

        if let Some(icon) = &mut self.icon {
            icon.set_state(if self.on_yes { IconState::Losing } else { IconState::Neutral });
            icon.draw(gpu, 250.0, 240.0, 120.0, [1.0, 1.0, 1.0, 1.0]);
        }

        let yes_color = if self.on_yes { red } else { gray };
        let no_color = if self.on_yes { gray } else { white };
        gpu.draw_text("YES", 430.0, 470.0, if self.on_yes { 42.0 } else { 32.0 }, yes_color);
        gpu.draw_text("NO", 770.0, 470.0, if self.on_yes { 32.0 } else { 42.0 }, no_color);
    }

    fn target_label(&self) -> String {
        match &self.target {
            ResetScoreTarget::Song {
                label, difficulty, ..
            }
            | ResetScoreTarget::Week {
                label, difficulty, ..
            } => format!("{} ({})?", label, difficulty.to_uppercase()),
        }
    }
}

