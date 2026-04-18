use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use rustic_core::character::CharacterFile;
use rustic_core::paths::AssetPaths;
use rustic_core::stage::StageFile;
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
    items: Vec<PreloadItem>,
    completed_steps: usize,
    tip_timer: f32,
    tip_index: usize,
    current_asset: String,
    warnings: Vec<String>,
    preload_rx: Option<Receiver<PreloadMessage>>,
    preload_started: bool,
    next: Option<Box<dyn Screen>>,
}

#[derive(Debug, Clone)]
struct PreloadItem {
    label: String,
    path: Option<PathBuf>,
}

#[derive(Debug)]
enum PreloadMessage {
    Progress { completed: usize, label: String },
    Warning(String),
    Done,
}

impl LoadingScreen {
    pub fn song(
        song_id: String,
        difficulty: String,
        play_as_opponent: bool,
        practice_mode: bool,
        botplay: bool,
    ) -> Self {
        let items = build_preload_items(&song_id, &difficulty);
        let current_asset = items
            .first()
            .map(|item| item.label.clone())
            .unwrap_or_else(|| "Loading...".to_string());
        Self {
            target: LoadingTarget::Song {
                song_id,
                difficulty,
                play_as_opponent,
                practice_mode,
                botplay,
            },
            items,
            completed_steps: 0,
            tip_timer: 0.0,
            tip_index: 0,
            current_asset,
            warnings: Vec::new(),
            preload_rx: None,
            preload_started: false,
            next: None,
        }
    }

    pub fn story(
        story: SharedStorySession,
        play_as_opponent: bool,
        practice_mode: bool,
        botplay: bool,
    ) -> Self {
        let items = build_preload_items(story.current_song(), &story.difficulty);
        let current_asset = items
            .first()
            .map(|item| item.label.clone())
            .unwrap_or_else(|| "Loading...".to_string());
        Self {
            target: LoadingTarget::Story {
                story,
                play_as_opponent,
                practice_mode,
                botplay,
            },
            items,
            completed_steps: 0,
            tip_timer: 0.0,
            tip_index: 0,
            current_asset,
            warnings: Vec::new(),
            preload_rx: None,
            preload_started: false,
            next: None,
        }
    }

    fn start_preload(&mut self) {
        if self.preload_started {
            return;
        }

        self.preload_started = true;
        let items = self.items.clone();
        let (tx, rx) = mpsc::channel();
        self.preload_rx = Some(rx);
        thread::spawn(move || {
            for (idx, item) in items.iter().enumerate() {
                if let Some(path) = &item.path {
                    if let Err(err) = std::fs::read(path) {
                        let _ =
                            tx.send(PreloadMessage::Warning(format!("{}: {}", item.label, err)));
                    }
                }
                let _ = tx.send(PreloadMessage::Progress {
                    completed: idx + 1,
                    label: item.label.clone(),
                });
            }
            let _ = tx.send(PreloadMessage::Done);
        });
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
    fn init(&mut self, _gpu: &GpuState) {
        self.start_preload();
    }

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

        let mut done = false;
        if let Some(rx) = &self.preload_rx {
            while let Ok(message) = rx.try_recv() {
                match message {
                    PreloadMessage::Progress { completed, label } => {
                        self.completed_steps = completed;
                        self.current_asset = label;
                    }
                    PreloadMessage::Warning(warning) => self.warnings.push(warning),
                    PreloadMessage::Done => done = true,
                }
            }
        }
        if done {
            self.preload_rx = None;
            self.finish_loading();
        }
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        if !gpu.begin_frame() {
            return;
        }

        gpu.push_colored_quad(0.0, 0.0, 1280.0, 720.0, [0.05, 0.04, 0.08, 1.0]);
        gpu.draw_batch(None);
        gpu.draw_text("Loading...", 470.0, 180.0, 42.0, [1.0, 1.0, 1.0, 1.0]);
        gpu.draw_text(
            &self.current_asset,
            270.0,
            280.0,
            24.0,
            [0.85, 0.85, 0.85, 1.0],
        );

        let progress = if self.items.is_empty() {
            1.0
        } else {
            self.completed_steps as f32 / self.items.len() as f32
        };
        gpu.push_colored_quad(220.0, 360.0, 840.0, 28.0, [0.12, 0.12, 0.16, 1.0]);
        gpu.draw_batch(None);
        gpu.push_colored_quad(
            224.0,
            364.0,
            832.0 * progress.clamp(0.0, 1.0),
            20.0,
            [0.9, 0.45, 0.3, 1.0],
        );
        gpu.draw_batch(None);
        gpu.draw_text(
            &format!("{:.0}%", progress * 100.0),
            595.0,
            405.0,
            24.0,
            [1.0, 1.0, 1.0, 1.0],
        );

        gpu.draw_text("TIP", 220.0, 500.0, 22.0, [1.0, 0.85, 0.4, 1.0]);
        gpu.draw_text(
            TIPS[self.tip_index],
            220.0,
            540.0,
            24.0,
            [0.9, 0.9, 0.9, 1.0],
        );
        if let Some(warning) = self.warnings.last() {
            gpu.draw_text(
                &format!("Warning: {warning}"),
                220.0,
                630.0,
                18.0,
                [1.0, 0.75, 0.35, 1.0],
            );
        }

        crate::debug_overlay::finish_frame(gpu);
    }

    fn next_screen(&mut self) -> Option<Box<dyn Screen>> {
        self.next.take()
    }
}

fn build_preload_items(song_id: &str, difficulty: &str) -> Vec<PreloadItem> {
    let paths = AssetPaths::platform_default();
    let mut items = Vec::new();
    push_item(
        &mut items,
        format!("Resolving chart for {song_id} ({difficulty})"),
        paths.chart(song_id, difficulty),
    );
    push_item(
        &mut items,
        format!("Loading instrument for {song_id}"),
        paths.song_audio(song_id, "Inst.ogg"),
    );
    push_item(
        &mut items,
        format!("Loading vocals for {song_id}"),
        paths.song_audio(song_id, "Voices.ogg"),
    );
    push_item(
        &mut items,
        "Loading note skin".to_string(),
        paths.image("noteSkins/NOTE_assets"),
    );
    push_item(
        &mut items,
        "Loading note skin atlas".to_string(),
        paths.image_xml("noteSkins/NOTE_assets"),
    );

    if let Some(chart_path) = paths.chart(song_id, difficulty) {
        if let Ok(chart_json) = std::fs::read_to_string(chart_path) {
            if let Ok(chart) = rustic_core::chart::parse_chart(&chart_json) {
                push_stage_items(&paths, &mut items, &chart.song.stage);
                for name in [
                    &chart.song.player1,
                    &chart.song.player2,
                    &chart.song.gf_version,
                ] {
                    push_character_items(&paths, &mut items, name);
                }
            }
        }
    }
    items.push(PreloadItem {
        label: "Finalizing gameplay scene".to_string(),
        path: None,
    });
    items
}

fn push_item(items: &mut Vec<PreloadItem>, label: String, path: Option<PathBuf>) {
    items.push(PreloadItem { label, path });
}

fn push_stage_items(paths: &AssetPaths, items: &mut Vec<PreloadItem>, stage_name: &str) {
    push_item(
        items,
        format!("Preparing stage {stage_name}"),
        paths.stage_json(stage_name),
    );
    let Some(stage_path) = paths.stage_json(stage_name) else {
        return;
    };
    let Ok(json) = std::fs::read_to_string(stage_path) else {
        return;
    };
    let Ok(stage) = StageFile::from_json(&json) else {
        return;
    };
    for object in &stage.objects {
        if object.image.is_empty() {
            continue;
        }
        push_item(
            items,
            format!("Loading stage image {}", object.image),
            paths.stage_image(&object.image, &stage.directory),
        );
        if object.obj_type == "animatedSprite" {
            push_item(
                items,
                format!("Loading stage atlas {}", object.image),
                paths.image_xml(&object.image),
            );
        }
    }
}

fn push_character_items(paths: &AssetPaths, items: &mut Vec<PreloadItem>, name: &str) {
    if name.is_empty() {
        return;
    }
    let json_path = paths.character_json(name);
    push_item(
        items,
        format!("Preparing character {name}"),
        json_path.clone(),
    );
    let Some(json_path) = json_path else {
        return;
    };
    let Ok(json) = std::fs::read_to_string(json_path) else {
        return;
    };
    let Ok(character) = CharacterFile::from_json(&json) else {
        return;
    };
    let image = character.effective_image().to_string();
    if !image.is_empty() {
        push_item(
            items,
            format!("Loading character image {image}"),
            paths.image(&image),
        );
        push_item(
            items,
            format!("Loading character atlas {image}"),
            paths.image_xml(&image),
        );
    }
    if !character.healthicon.is_empty() {
        push_item(
            items,
            format!("Loading icon {}", character.healthicon),
            paths.health_icon(&character.healthicon),
        );
    }
}
