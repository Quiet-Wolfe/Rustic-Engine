#[path = "freeplay_actions.rs"]
mod freeplay_actions;

use winit::event::TouchPhase;
use winit::keyboard::KeyCode;

use rustic_audio::AudioEngine;
use rustic_core::highscore::HighscoreStore;
use rustic_core::paths::AssetPaths;
use rustic_core::week;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::health_icon::{HealthIcon, IconState};

use super::freeplay_support::{approx_text_width, key_to_char, srgb_to_linear, FreeplaySong};
use super::gameplay_changers::GameplayChangersState;
use super::loading::LoadingScreen;
use super::reset_score::{ResetScoreAction, ResetScoreModal};
use crate::screen::Screen;

const GAME_W: f32 = 1280.0;
const GAME_H: f32 = 720.0;

const DIFFICULTIES: [&str; 3] = ["easy", "normal", "hard"];

pub struct FreeplayScreen {
    audio: Option<AudioEngine>,
    bg_tex: Option<GpuTexture>,
    songs: Vec<FreeplaySong>,
    filtered: Vec<usize>, // indices into songs matching search
    search: String,
    cur_selected: usize,   // index into filtered
    cur_difficulty: usize, // index into DIFFICULTIES
    lerp_selected: f32,
    bg_color: [f32; 3],
    bg_color_target: [f32; 3],
    next: Option<Box<dyn Screen>>,
    confirmed: bool,
    play_as_opponent: bool,
    highscores: HighscoreStore,
    displayed_score: f32,
    displayed_accuracy: f32,
    target_score: i32,
    target_accuracy: f32,
    previewing_song: Option<String>,
    reset_modal: Option<ResetScoreModal>,
    practice_mode: bool,
    botplay: bool,
    gameplay_changers: Option<GameplayChangersState>,
}

impl FreeplayScreen {
    pub fn new() -> Self {
        Self {
            audio: None,
            bg_tex: None,
            songs: Vec::new(),
            filtered: Vec::new(),
            search: String::new(),
            cur_selected: 0,
            cur_difficulty: 1, // normal
            lerp_selected: 0.0,
            bg_color: [0.57, 0.44, 0.99], // default purple
            bg_color_target: [0.57, 0.44, 0.99],
            next: None,
            confirmed: false,
            play_as_opponent: false,
            highscores: HighscoreStore::load(),
            displayed_score: 0.0,
            displayed_accuracy: 0.0,
            target_score: 0,
            target_accuracy: 0.0,
            previewing_song: None,
            reset_modal: None,
            practice_mode: false,
            botplay: false,
            gameplay_changers: None,
        }
    }
}

impl Screen for FreeplayScreen {
    fn init(&mut self, gpu: &GpuState) {
        let paths = AssetPaths::platform_default();

        if let Some(bg_path) = paths.image("menuDesat") {
            self.bg_tex = Some(gpu.load_texture_from_path(&bg_path));
        }

        let mut seen_songs = std::collections::HashSet::new();

        let mut all_weeks = Vec::new();
        for weeks_dir in paths.all_weeks_dirs() {
            for w in week::load_weeks(&weeks_dir) {
                all_weeks.push(w);
            }
        }
        all_weeks.sort_by(|a, b| a.file_name.cmp(&b.file_name));
        for w in &all_weeks {
            if w.hide_freeplay {
                continue;
            }
            for song in &w.songs {
                let key = song.name.to_lowercase().replace(' ', "-");
                seen_songs.insert(key);
                self.songs.push(FreeplaySong {
                    name: song.name.clone(),
                    song_id: song.name.to_lowercase().replace(' ', "-"),
                    character: song.character.clone(),
                    color: [
                        srgb_to_linear(song.color[0] as f32 / 255.0),
                        srgb_to_linear(song.color[1] as f32 / 255.0),
                        srgb_to_linear(song.color[2] as f32 / 255.0),
                    ],
                    icon: None,
                });
            }
        }

        for song_name in paths.discover_songs() {
            if seen_songs.contains(&song_name) {
                continue;
            }
            seen_songs.insert(song_name.clone());
            self.songs.push(FreeplaySong {
                name: song_name.clone(),
                song_id: song_name,
                character: String::new(),
                color: [146, 113, 253].map(|c| srgb_to_linear(c as f32 / 255.0)),
                icon: None,
            });
        }

        for song in &mut self.songs {
            let icon_path = paths
                .health_icon(&song.character)
                .or_else(|| paths.health_icon("face"));
            if let Some(path) = icon_path {
                let mut icon = HealthIcon::load(gpu, &path, false);
                icon.set_state(IconState::Neutral);
                song.icon = Some(icon);
            }
        }

        self.rebuild_filter();
        if !self.filtered.is_empty() {
            let song_idx = self.filtered[0];
            self.bg_color_target = self.songs[song_idx].color;
            self.bg_color = self.songs[song_idx].color;
        }
        self.lerp_selected = self.cur_selected as f32;
        self.refresh_score_target();
        self.displayed_score = self.target_score as f32;
        self.displayed_accuracy = self.target_accuracy;

        if self.audio.is_none() {
            if let Some(music) = paths.music("freakyMenu") {
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
                    self.refresh_score_target();
                    self.displayed_score = self.target_score as f32;
                    self.displayed_accuracy = self.target_accuracy;
                    self.reset_modal = None;
                }
            }
            return;
        }
        if self.confirmed {
            return;
        }

        match key {
            KeyCode::Tab => {
                self.play_as_opponent = !self.play_as_opponent;
            }
            KeyCode::ControlLeft | KeyCode::ControlRight => {
                self.gameplay_changers =
                    Some(GameplayChangersState::new(self.practice_mode, self.botplay));
            }
            KeyCode::ArrowUp => self.change_selection(-1),
            KeyCode::ArrowDown => self.change_selection(1),
            KeyCode::ArrowLeft => self.change_difficulty(-1),
            KeyCode::ArrowRight => self.change_difficulty(1),
            KeyCode::Space => self.toggle_preview(),
            KeyCode::KeyR => {
                if let Some(&song_idx) = self.filtered.get(self.cur_selected) {
                    let song = &self.songs[song_idx];
                    self.reset_modal = Some(ResetScoreModal::song(
                        song.song_id.clone(),
                        song.name.clone(),
                        DIFFICULTIES[self.cur_difficulty].to_string(),
                        song.character.clone(),
                    ));
                }
            }
            KeyCode::Enter => {
                if !self.filtered.is_empty() {
                    self.stop_preview();
                    self.confirmed = true;
                    if let Some(audio) = &mut self.audio {
                        let paths = AssetPaths::platform_default();
                        if let Some(sfx) = paths.sound("confirmMenu") {
                            audio.play_sound(&sfx, 0.7);
                        }
                    }
                    let song_idx = self.filtered[self.cur_selected];
                    let song = &self.songs[song_idx];
                    let diff = DIFFICULTIES[self.cur_difficulty];
                    self.next = Some(Box::new(LoadingScreen::song(
                        song.song_id.clone(),
                        diff.to_string(),
                        self.play_as_opponent,
                        self.practice_mode,
                        self.botplay,
                    )));
                }
            }
            KeyCode::Escape => {
                if !self.search.is_empty() {
                    self.search.clear();
                    self.rebuild_filter();
                } else {
                    self.stop_preview();
                    if let Some(audio) = &mut self.audio {
                        let paths = AssetPaths::platform_default();
                        if let Some(sfx) = paths.sound("cancelMenu") {
                            audio.play_sound(&sfx, 0.7);
                        }
                    }
                    self.next = Some(Box::new(super::main_menu::MainMenuScreen::new()));
                }
            }
            KeyCode::Backspace => {
                if !self.search.is_empty() {
                    self.search.pop();
                    self.rebuild_filter();
                } else {
                    self.stop_preview();
                    if let Some(audio) = &mut self.audio {
                        let paths = AssetPaths::platform_default();
                        if let Some(sfx) = paths.sound("cancelMenu") {
                            audio.play_sound(&sfx, 0.7);
                        }
                    }
                    self.next = Some(Box::new(super::main_menu::MainMenuScreen::new()));
                }
            }
            _ => {
                if let Some(ch) = key_to_char(key) {
                    self.search.push(ch);
                    self.rebuild_filter();
                }
            }
        }
    }

    fn handle_touch(&mut self, _id: u64, phase: TouchPhase, x: f64, y: f64) {
        if phase != TouchPhase::Started || self.confirmed {
            return;
        }
        let (x, y) = (x as f32, y as f32);

        if x < 30.0 && y > 70.0 && y < GAME_H - 30.0 {
            let strip_h = GAME_H - 100.0;
            let t = (y - 70.0) / strip_h;
            let letter_idx = (t * 26.0) as usize;
            let letter = (b'A' + letter_idx.min(25) as u8) as char;
            self.jump_to_letter(letter);
            return;
        }

        if y < 66.0 && x > GAME_W * 0.7 {
            if x < GAME_W * 0.85 {
                self.handle_key(KeyCode::ArrowLeft);
            } else {
                self.handle_key(KeyCode::ArrowRight);
            }
            return;
        }

        let draw_dist = 6;
        for (i, &_song_idx) in self.filtered.iter().enumerate() {
            let target_y = i as f32 - self.lerp_selected;
            if target_y.abs() > draw_dist as f32 {
                continue;
            }
            let item_y = target_y * 1.3 * 120.0 + 320.0;
            if y >= item_y - 15.0 && y < item_y + 35.0 {
                if i == self.cur_selected {
                    self.handle_key(KeyCode::Enter);
                } else {
                    self.change_selection(i as i32 - self.cur_selected as i32);
                }
                return;
            }
        }
    }

    fn update(&mut self, dt: f32) {
        if let Some(reset_modal) = &mut self.reset_modal {
            reset_modal.update(dt);
        }
        let lerp = (-dt * 9.6).exp();
        self.lerp_selected =
            self.cur_selected as f32 + (self.lerp_selected - self.cur_selected as f32) * lerp;

        let color_lerp = 1.0 - (-dt * 3.0).exp();
        for i in 0..3 {
            self.bg_color[i] += (self.bg_color_target[i] - self.bg_color[i]) * color_lerp;
        }

        self.displayed_score = self.target_score as f32
            + (self.displayed_score - self.target_score as f32) * (-dt * 24.0).exp();
        if (self.displayed_score - self.target_score as f32).abs() <= 10.0 {
            self.displayed_score = self.target_score as f32;
        }

        self.displayed_accuracy = self.target_accuracy
            + (self.displayed_accuracy - self.target_accuracy) * (-dt * 12.0).exp();
        if (self.displayed_accuracy - self.target_accuracy).abs() <= 0.01 {
            self.displayed_accuracy = self.target_accuracy;
        }
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        if !gpu.begin_frame() {
            return;
        }

        // Background with color tint
        if let Some(bg) = &self.bg_tex {
            let c = &self.bg_color;
            let color = [c[0], c[1], c[2], 1.0];
            // screenCenter
            let bw = bg.width as f32;
            let bh = bg.height as f32;
            let x = (GAME_W - bw) / 2.0;
            let y = (GAME_H - bh) / 2.0;
            gpu.push_texture_region(bw, bh, 0.0, 0.0, bw, bh, x, y, bw, bh, false, color);
            gpu.draw_batch(Some(bg));
        } else {
            let c = &self.bg_color;
            gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [c[0], c[1], c[2], 1.0]);
            gpu.draw_batch(None);
        }

        let text_size = 28.0;
        let draw_dist = 6;

        for (i, &song_idx) in self.filtered.iter().enumerate() {
            let target_y = i as f32 - self.lerp_selected;
            if target_y.abs() > draw_dist as f32 {
                continue;
            }

            let x = target_y * 20.0 + 90.0;
            let y = target_y * 1.3 * 120.0 + 320.0;
            if y < -50.0 || y > GAME_H + 50.0 {
                continue;
            }

            let alpha = if i == self.cur_selected { 1.0 } else { 0.6 };
            let color = [alpha, alpha, alpha, alpha];
            let icon_x = x + approx_text_width(&self.songs[song_idx].name, text_size) + 12.0;

            gpu.draw_text(&self.songs[song_idx].name, x, y, text_size, color);
            if let Some(icon) = &mut self.songs[song_idx].icon {
                let state = if i == self.cur_selected {
                    IconState::Winning
                } else {
                    IconState::Neutral
                };
                icon.set_state(state);
                icon.draw(gpu, icon_x, y - 30.0, 150.0, color);
            }
        }

        let score_x = GAME_W * 0.7;
        let score_bg_w = GAME_W - score_x + 6.0;
        gpu.push_colored_quad(score_x - 6.0, 0.0, score_bg_w, 66.0, [0.0, 0.0, 0.0, 0.6]);
        gpu.draw_batch(None);

        let diff_name = DIFFICULTIES[self.cur_difficulty].to_uppercase();
        if DIFFICULTIES.len() > 1 {
            gpu.draw_text("<", score_x, 41.0, 24.0, [1.0, 1.0, 0.4, 0.9]);
            gpu.draw_text(&diff_name, score_x + 20.0, 41.0, 24.0, [1.0, 1.0, 1.0, 1.0]);
            let arrow_x = GAME_W - 20.0;
            gpu.draw_text(">", arrow_x, 41.0, 24.0, [1.0, 1.0, 0.4, 0.9]);
        } else {
            gpu.draw_text(&diff_name, score_x, 41.0, 24.0, [1.0, 1.0, 1.0, 1.0]);
        }

        let score_text = self.current_score_text();
        gpu.draw_text(&score_text, score_x, 5.0, 24.0, [1.0, 1.0, 1.0, 1.0]);

        if !self.search.is_empty() {
            gpu.push_colored_quad(0.0, 0.0, 400.0, 36.0, [0.0, 0.0, 0.0, 0.7]);
            gpu.draw_batch(None);
            let search_display = format!("Search: {}_", self.search);
            gpu.draw_text(&search_display, 10.0, 8.0, 20.0, [1.0, 1.0, 0.4, 1.0]);
        }

        let count_text = if cfg!(target_os = "android") {
            format!(
                "{} songs | Tap song to play | Tap difficulty to change | Opponent: {}",
                self.filtered.len(),
                if self.play_as_opponent { "ON" } else { "OFF" }
            )
        } else if self.search.is_empty() {
            format!("{} songs | SPACE Preview:{} | ENTER Play | LEFT-RIGHT difficulty | TAB Opponent: {}", self.filtered.len(), if self.previewing_song.is_some() { "ON" } else { "OFF" }, if self.play_as_opponent { "ON" } else { "OFF" })
        } else {
            format!(
                "{}/{} songs | ESC to clear search | ENTER to play | TAB Opponent: {}",
                self.filtered.len(),
                self.songs.len(),
                if self.play_as_opponent { "ON" } else { "OFF" }
            )
        };
        gpu.push_colored_quad(0.0, GAME_H - 26.0, GAME_W, 26.0, [0.0, 0.0, 0.0, 0.6]);
        gpu.draw_batch(None);
        gpu.draw_text(&count_text, 10.0, GAME_H - 22.0, 16.0, [1.0, 1.0, 1.0, 1.0]);

        if cfg!(target_os = "android") {
            let strip_x = 2.0;
            let strip_top = 70.0;
            let strip_h = GAME_H - 100.0;
            let letter_h = strip_h / 26.0;
            gpu.push_colored_quad(0.0, strip_top, 28.0, strip_h, [0.0, 0.0, 0.0, 0.3]);
            gpu.draw_batch(None);
            for i in 0..26u8 {
                let letter = (b'A' + i) as char;
                let y = strip_top + i as f32 * letter_h;
                gpu.draw_text(
                    &letter.to_string(),
                    strip_x + 4.0,
                    y,
                    letter_h.min(18.0),
                    [1.0, 1.0, 1.0, 0.7],
                );
            }
        }

        if let Some(gameplay_changers) = &self.gameplay_changers {
            gameplay_changers.draw(gpu);
        }
        if let Some(reset_modal) = &mut self.reset_modal {
            reset_modal.draw(gpu);
        }

        crate::debug_overlay::finish_frame(gpu);
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
