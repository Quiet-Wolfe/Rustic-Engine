#[path = "story_menu_draw.rs"]
mod story_menu_draw;

use rustic_audio::AudioEngine;
use rustic_core::highscore::HighscoreStore;
use rustic_core::paths::AssetPaths;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::{AnimationController, SpriteAtlas};
use winit::event::TouchPhase;
use winit::keyboard::KeyCode;

use crate::screen::Screen;

use super::gameplay_changers::GameplayChangersState;
use super::play::PlayScreen;
use super::reset_score::{ResetScoreAction, ResetScoreModal};
use super::story_menu_support::{
    available_difficulties, load_story_weeks, song_id, StoryMenuCharacter, StoryWeekEntry,
};

struct StoryUi {
    texture: GpuTexture,
    atlas: SpriteAtlas,
    tex_w: f32,
    tex_h: f32,
    left_arrow: AnimationController,
    right_arrow: AnimationController,
}

pub struct StoryMenuScreen {
    audio: Option<AudioEngine>,
    paths: AssetPaths,
    highscores: HighscoreStore,
    weeks: Vec<StoryWeekEntry>,
    selected_week: usize,
    selection_lerp: f32,
    week_images: Vec<Option<GpuTexture>>,
    current_background: Option<GpuTexture>,
    current_characters: [Option<StoryMenuCharacter>; 3],
    current_difficulty_texture: Option<GpuTexture>,
    ui: Option<StoryUi>,
    track_header: Option<GpuTexture>,
    selected_difficulty: usize,
    available_difficulties: Vec<String>,
    displayed_score: f32,
    target_score: i32,
    difficulty_alpha: f32,
    difficulty_y: f32,
    confirm_timer: f32,
    confirming: bool,
    next: Option<Box<dyn Screen>>,
    pending_selection_assets: bool,
    initial_week: Option<String>,
    reset_modal: Option<ResetScoreModal>,
    practice_mode: bool,
    botplay: bool,
    gameplay_changers: Option<GameplayChangersState>,
}

impl StoryMenuScreen {
    pub fn new() -> Self {
        Self::with_week(None)
    }

    pub fn with_week(week: Option<String>) -> Self {
        Self {
            audio: None,
            paths: AssetPaths::platform_default(),
            highscores: HighscoreStore::load(),
            weeks: Vec::new(),
            selected_week: 0,
            selection_lerp: 0.0,
            week_images: Vec::new(),
            current_background: None,
            current_characters: [None, None, None],
            current_difficulty_texture: None,
            ui: None,
            track_header: None,
            selected_difficulty: 0,
            available_difficulties: vec!["normal".to_string()],
            displayed_score: 0.0,
            target_score: 0,
            difficulty_alpha: 1.0,
            difficulty_y: 170.0,
            confirm_timer: 0.0,
            confirming: false,
            next: None,
            pending_selection_assets: true,
            initial_week: week,
            reset_modal: None,
            practice_mode: false,
            botplay: false,
            gameplay_changers: None,
        }
    }

    fn current_week(&self) -> Option<&StoryWeekEntry> {
        self.weeks.get(self.selected_week)
    }

    fn refresh_selection_metadata(&mut self) {
        let Some(week) = self.current_week().map(|entry| entry.week.clone()) else {
            return;
        };

        self.available_difficulties = available_difficulties(&self.paths, &week);
        self.selected_difficulty = self.selected_difficulty.min(self.available_difficulties.len().saturating_sub(1));
        let diff = self.available_difficulties[self.selected_difficulty].clone();
        self.target_score = self.highscores.get_week_score(&week.file_name, &diff);
        self.difficulty_alpha = 0.0;
        self.difficulty_y = 140.0;
        self.pending_selection_assets = true;
    }

    fn reload_selection_assets(&mut self, gpu: &GpuState) {
        let Some(week) = self.current_week().map(|entry| entry.week.clone()) else {
            return;
        };

        self.current_background = if week.week_background.is_empty() {
            None
        } else {
            self.paths
                .find(&format!("images/menubackgrounds/menu_{}.png", week.week_background))
                .map(|path| gpu.load_texture_from_path(&path))
        };

        self.current_characters = std::array::from_fn(|slot| {
            StoryMenuCharacter::load(gpu, &self.paths, slot, &week.week_characters[slot])
        });

        self.current_difficulty_texture = self.paths
            .find(&format!(
                "images/menudifficulties/{}.png",
                self.available_difficulties[self.selected_difficulty]
            ))
            .map(|path| gpu.load_texture_from_path(&path));

        self.pending_selection_assets = false;
    }

    fn change_week(&mut self, delta: i32) {
        if self.weeks.is_empty() {
            return;
        }
        let len = self.weeks.len() as i32;
        self.selected_week = ((self.selected_week as i32 + delta).rem_euclid(len)) as usize;
        self.refresh_selection_metadata();
        if let Some(audio) = &mut self.audio {
            if let Some(sfx) = self.paths.sound("scrollMenu") {
                audio.play_sound(&sfx, 0.4);
            }
        }
    }

    fn change_difficulty(&mut self, delta: i32) {
        if self.available_difficulties.is_empty() {
            return;
        }
        let len = self.available_difficulties.len() as i32;
        self.selected_difficulty = ((self.selected_difficulty as i32 + delta).rem_euclid(len)) as usize;
        self.refresh_selection_metadata();
        if let Some(audio) = &mut self.audio {
            if let Some(sfx) = self.paths.sound("scrollMenu") {
                audio.play_sound(&sfx, 0.4);
            }
        }
    }

    fn confirm_week(&mut self) {
        let Some(week) = self.current_week() else {
            return;
        };
        if week.locked {
            if let Some(audio) = &mut self.audio {
                if let Some(sfx) = self.paths.sound("cancelMenu") {
                    audio.play_sound(&sfx, 0.7);
                }
            }
            return;
        }

        self.confirming = true;
        self.confirm_timer = 0.0;
        if let Some(audio) = &mut self.audio {
            if let Some(sfx) = self.paths.sound("confirmMenu") {
                audio.play_sound(&sfx, 0.7);
            }
        }
        for character in self.current_characters.iter_mut().flatten() {
            character.play_confirm();
        }
    }

    fn current_difficulty(&self) -> &str {
        self.available_difficulties
            .get(self.selected_difficulty)
            .map(String::as_str)
            .unwrap_or("normal")
    }

    fn open_reset_modal(&mut self) {
        let Some(week) = self.current_week() else {
            return;
        };
        self.reset_modal = Some(ResetScoreModal::week(
            week.week.file_name.clone(),
            week.week.week_name.clone(),
            self.current_difficulty().to_string(),
        ));
    }
}

impl Screen for StoryMenuScreen {
    fn init(&mut self, gpu: &GpuState) {
        self.weeks = load_story_weeks(&self.paths, &self.highscores);
        self.week_images = self
            .weeks
            .iter()
            .map(|week| {
                self.paths
                    .find(&format!("images/storymenu/{}.png", week.week.file_name))
                    .map(|path| gpu.load_texture_from_path(&path))
            })
            .collect();

        if let Some(initial_week) = &self.initial_week {
            if let Some(idx) = self
                .weeks
                .iter()
                .position(|week| week.week.file_name == *initial_week)
            {
                self.selected_week = idx;
            }
        }
        self.selection_lerp = self.selected_week as f32;
        self.refresh_selection_metadata();
        self.displayed_score = self.target_score as f32;

        if let (Some(png), Some(xml_path)) = (
            self.paths.image("campaign_menu_UI_assets"),
            self.paths.image_xml("campaign_menu_UI_assets"),
        ) {
            let xml = std::fs::read_to_string(xml_path).unwrap_or_default();
            let texture = gpu.load_texture_from_path(&png);
            let mut atlas = SpriteAtlas::from_xml(&xml);
            atlas.add_by_prefix("arrow_left", "arrow left");
            atlas.add_by_prefix("arrow_left_press", "arrow push left");
            atlas.add_by_prefix("arrow_right", "arrow right");
            atlas.add_by_prefix("arrow_right_press", "arrow push right");
            atlas.add_by_prefix("lock", "lock");
            let mut left_arrow = AnimationController::new();
            left_arrow.play("arrow_left", 24.0, true);
            let mut right_arrow = AnimationController::new();
            right_arrow.play("arrow_right", 24.0, true);
            self.ui = Some(StoryUi {
                tex_w: texture.width as f32,
                tex_h: texture.height as f32,
                texture,
                atlas,
                left_arrow,
                right_arrow,
            });
        }

        self.track_header = self
            .paths
            .image("Menu_Tracks")
            .map(|path| gpu.load_texture_from_path(&path));
        self.reload_selection_assets(gpu);

        if self.audio.is_none() {
            if let Some(music) = self.paths.music("freakyMenu") {
                let mut audio = AudioEngine::new();
                audio.play_loop_music_vol(&music, 0.7);
                self.audio = Some(audio);
            }
        }
    }

    fn handle_key(&mut self, key: KeyCode) {
        if let Some(gameplay_changers) = &mut self.gameplay_changers {
            match key {
                KeyCode::Escape | KeyCode::ControlLeft | KeyCode::ControlRight => {
                    self.practice_mode = gameplay_changers.practice_mode;
                    self.botplay = gameplay_changers.botplay;
                    self.gameplay_changers = None;
                }
                _ => {
                    gameplay_changers.handle_key(key);
                }
            }
            return;
        }
        if let Some(reset_modal) = &mut self.reset_modal {
            match reset_modal.handle_key(key) {
                ResetScoreAction::None => {}
                ResetScoreAction::Close => self.reset_modal = None,
                ResetScoreAction::Confirmed => {
                    reset_modal.apply(&mut self.highscores);
                    self.refresh_selection_metadata();
                    self.displayed_score = self.target_score as f32;
                    self.reset_modal = None;
                }
            }
            return;
        }

        if self.confirming {
            return;
        }

        match key {
            KeyCode::ArrowUp | KeyCode::KeyW => self.change_week(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => self.change_week(1),
            KeyCode::ArrowLeft | KeyCode::KeyA => {
                if let Some(ui) = &mut self.ui {
                    ui.left_arrow.force_play("arrow_left_press", 24.0, false);
                }
                self.change_difficulty(-1);
            }
            KeyCode::ArrowRight | KeyCode::KeyD => {
                if let Some(ui) = &mut self.ui {
                    ui.right_arrow.force_play("arrow_right_press", 24.0, false);
                }
                self.change_difficulty(1);
            }
            KeyCode::ControlLeft | KeyCode::ControlRight => {
                self.gameplay_changers = Some(GameplayChangersState::new(self.practice_mode, self.botplay));
            }
            KeyCode::KeyR => self.open_reset_modal(),
            KeyCode::Enter | KeyCode::Space => self.confirm_week(),
            KeyCode::Escape | KeyCode::Backspace => {
                if let Some(audio) = &mut self.audio {
                    if let Some(sfx) = self.paths.sound("cancelMenu") {
                        audio.play_sound(&sfx, 0.7);
                    }
                }
                self.next = Some(Box::new(super::main_menu::MainMenuScreen::new()));
            }
            _ => {}
        }
    }

    fn handle_touch(&mut self, _id: u64, phase: TouchPhase, x: f64, y: f64) {
        if phase != TouchPhase::Started || self.confirming || self.reset_modal.is_some() {
            return;
        }
        let (x, y) = (x as f32, y as f32);
        if x > 840.0 && x < 1110.0 && y > 500.0 && y < 590.0 {
            if x < 910.0 {
                self.handle_key(KeyCode::ArrowLeft);
            } else if x > 1090.0 {
                self.handle_key(KeyCode::ArrowRight);
            } else {
                self.handle_key(KeyCode::Enter);
            }
        }
    }

    fn update(&mut self, dt: f32) {
        self.selection_lerp = self.selected_week as f32
            + (self.selection_lerp - self.selected_week as f32) * (-dt * 10.2).exp();
        self.displayed_score = self.target_score as f32
            + (self.displayed_score - self.target_score as f32) * (-dt * 30.0).exp();
        if (self.displayed_score - self.target_score as f32).abs() < 10.0 {
            self.displayed_score = self.target_score as f32;
        }
        self.difficulty_alpha = (self.difficulty_alpha + dt * 14.0).min(1.0);
        self.difficulty_y += (170.0 - self.difficulty_y) * (1.0 - (-dt * 14.0).exp());

        for character in self.current_characters.iter_mut().flatten() {
            character.update(dt);
        }

        if let Some(reset_modal) = &mut self.reset_modal {
            reset_modal.update(dt);
        }

        if let Some(ui) = &mut self.ui {
            let left_count = ui.atlas.frame_count(&ui.left_arrow.current_anim);
            ui.left_arrow.update(dt, left_count);
            if ui.left_arrow.finished && ui.left_arrow.current_anim == "arrow_left_press" {
                ui.left_arrow.play("arrow_left", 24.0, true);
            }
            let right_count = ui.atlas.frame_count(&ui.right_arrow.current_anim);
            ui.right_arrow.update(dt, right_count);
            if ui.right_arrow.finished && ui.right_arrow.current_anim == "arrow_right_press" {
                ui.right_arrow.play("arrow_right", 24.0, true);
            }
        }

        if self.confirming {
            self.confirm_timer += dt;
            if self.confirm_timer >= 1.0 {
                if let Some(week) = self.current_week() {
                    let playlist = week
                        .week
                        .songs
                        .iter()
                        .map(|song| song_id(&song.name))
                        .collect();
                    let mut play = PlayScreen::new_story(
                        playlist,
                        &week.week.file_name,
                        self.current_difficulty(),
                        false,
                    );
                    play.apply_gameplay_modifiers(self.practice_mode, self.botplay);
                    self.next = Some(Box::new(play));
                }
            }
        }
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        self.draw_inner(gpu);
    }

    fn next_screen(&mut self) -> Option<Box<dyn Screen>> {
        self.next.take()
    }

    fn take_audio(&mut self) -> Option<AudioEngine> {
        self.audio.take()
    }

    fn set_audio(&mut self, audio: AudioEngine) {
        self.audio = Some(audio);
    }
}
