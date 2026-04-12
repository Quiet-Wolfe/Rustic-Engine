use rustic_core::paths::AssetPaths;
use rustic_render::gpu::GpuState;
use winit::event::TouchPhase;
use winit::keyboard::KeyCode;

use crate::screen::Screen;

use super::play::{PlayScreen, SharedStorySession};

const TIPS: [&str; 5] = [
    "Press SPACE in Freeplay to preview songs",
    "Ghost tapping lets you press keys without missing",
    "Hold notes give health each tick",
    "Camera follows the current singer",
    "Sick hits give the most points",
];

pub enum LoadingTarget {
    Song {
        song_id: String,
        difficulty: String,
        play_as_opponent: bool,
        practice_mode: bool,
        botplay: bool,
    },
    Story {
        story: SharedStorySession,
        play_as_opponent: bool,
        practice_mode: bool,
        botplay: bool,
    },
}

pub struct LoadingScreen {
    target: LoadingTarget,
    steps: Vec<String>,
    completed_steps: usize,
    step_timer: f32,
    tip_timer: f32,
    tip_index: usize,
    current_asset: String,
    next: Option<Box<dyn Screen>>,
}

impl LoadingScreen {
    pub fn song(
        song_id: String,
        difficulty: String,
        play_as_opponent: bool,
        practice_mode: bool,
        botplay: bool,
    ) -> Self {
        let steps = build_steps(&song_id, &difficulty);
        let current_asset = steps.first().cloned().unwrap_or_else(|| "Loading...".to_string());
        Self {
            target: LoadingTarget::Song {
                song_id,
                difficulty,
                play_as_opponent,
                practice_mode,
                botplay,
            },
            steps,
            completed_steps: 0,
            step_timer: 0.0,
            tip_timer: 0.0,
            tip_index: 0,
            current_asset,
            next: None,
        }
    }

    pub fn story(
        story: SharedStorySession,
        play_as_opponent: bool,
        practice_mode: bool,
        botplay: bool,
    ) -> Self {
        let steps = build_steps(story.current_song(), &story.difficulty);
        let current_asset = steps.first().cloned().unwrap_or_else(|| "Loading...".to_string());
        Self {
            target: LoadingTarget::Story {
                story,
                play_as_opponent,
                practice_mode,
                botplay,
            },
            steps,
            completed_steps: 0,
            step_timer: 0.0,
            tip_timer: 0.0,
            tip_index: 0,
            current_asset,
            next: None,
        }
    }

    fn finish_loading(&mut self) {
        let next: Box<dyn Screen> = match &self.target {
            LoadingTarget::Song {
                song_id,
                difficulty,
                play_as_opponent,
                practice_mode,
                botplay,
            } => {
                let mut play = PlayScreen::new(song_id, difficulty, *play_as_opponent);
                play.apply_gameplay_modifiers(*practice_mode, *botplay);
                Box::new(play)
            }
            LoadingTarget::Story {
                story,
                play_as_opponent,
                practice_mode,
                botplay,
            } => {
                let mut play = PlayScreen::from_story_session(story.clone(), *play_as_opponent);
                play.apply_gameplay_modifiers(*practice_mode, *botplay);
                Box::new(play)
            }
        };
        self.next = Some(next);
    }
}

impl Screen for LoadingScreen {
    fn init(&mut self, _gpu: &GpuState) {}

    fn handle_key(&mut self, _key: KeyCode) {}

    fn handle_touch(&mut self, _id: u64, _phase: TouchPhase, _x: f64, _y: f64) {}

    fn update(&mut self, dt: f32) {
        self.tip_timer += dt;
        if self.tip_timer >= 2.5 {
            self.tip_timer = 0.0;
            self.tip_index = (self.tip_index + 1) % TIPS.len();
        }

        if self.next.is_some() {
            return;
        }

        self.step_timer += dt;
        if self.step_timer >= 0.08 {
            self.step_timer = 0.0;
            if self.completed_steps < self.steps.len() {
                self.current_asset = self.steps[self.completed_steps].clone();
                self.completed_steps += 1;
            } else {
                self.finish_loading();
            }
        }
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        if !gpu.begin_frame() {
            return;
        }

        gpu.push_colored_quad(0.0, 0.0, 1280.0, 720.0, [0.05, 0.04, 0.08, 1.0]);
        gpu.draw_batch(None);
        gpu.draw_text("Loading...", 470.0, 180.0, 42.0, [1.0, 1.0, 1.0, 1.0]);
        gpu.draw_text(&self.current_asset, 270.0, 280.0, 24.0, [0.85, 0.85, 0.85, 1.0]);

        let progress = if self.steps.is_empty() {
            1.0
        } else {
            self.completed_steps as f32 / self.steps.len() as f32
        };
        gpu.push_colored_quad(220.0, 360.0, 840.0, 28.0, [0.12, 0.12, 0.16, 1.0]);
        gpu.draw_batch(None);
        gpu.push_colored_quad(224.0, 364.0, 832.0 * progress.clamp(0.0, 1.0), 20.0, [0.9, 0.45, 0.3, 1.0]);
        gpu.draw_batch(None);
        gpu.draw_text(&format!("{:.0}%", progress * 100.0), 595.0, 405.0, 24.0, [1.0, 1.0, 1.0, 1.0]);

        gpu.draw_text("TIP", 220.0, 500.0, 22.0, [1.0, 0.85, 0.4, 1.0]);
        gpu.draw_text(TIPS[self.tip_index], 220.0, 540.0, 24.0, [0.9, 0.9, 0.9, 1.0]);

        gpu.end_frame();
    }

    fn next_screen(&mut self) -> Option<Box<dyn Screen>> {
        self.next.take()
    }
}

fn build_steps(song_id: &str, difficulty: &str) -> Vec<String> {
    let paths = AssetPaths::platform_default();
    let mut steps = vec![
        format!("Resolving chart for {song_id} ({difficulty})"),
        format!("Loading instrument for {song_id}"),
    ];
    if paths.song_audio(song_id, "Voices.ogg").is_some() {
        steps.push(format!("Loading vocals for {song_id}"));
    }
    if let Some(chart_path) = paths.chart(song_id, difficulty) {
        if let Ok(chart_json) = std::fs::read_to_string(chart_path) {
            if let Ok(chart) = rustic_core::chart::parse_chart(&chart_json) {
                steps.push(format!("Preparing stage {}", chart.song.stage));
                steps.push(format!("Preparing characters {} / {}", chart.song.player1, chart.song.player2));
            }
        }
    }
    steps.push("Finalizing gameplay scene".to_string());
    steps
}

