use super::{PlayScreen, GAME_W, GAME_H};
use rustic_audio::AudioEngine;
use rustic_render::gpu::GpuState;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseMenuMode {
    Main,
    Difficulty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseMenuItem {
    Resume,
    RestartSong,
    ChangeDifficulty,
    Options,
    SkipTime,
    ExitToMenu,
}

impl PauseMenuItem {
    pub fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume",
            Self::RestartSong => "Restart Song",
            Self::ChangeDifficulty => "Change Difficulty",
            Self::Options => "Options",
            Self::SkipTime => "Skip Time",
            Self::ExitToMenu => "Exit to Menu",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PauseMenuState {
    pub mode: PauseMenuMode,
    pub selected: usize,
    pub overlay_alpha: f32,
    pub difficulty_choices: Vec<String>,
    pub skip_target_ms: f64,
    pub skip_time_enabled: bool,
    pub skip_hold_time: f32,
    pub timer: f32,
    pub pause_music_volume: f32,
}

impl PauseMenuState {
    pub fn new(difficulty_choices: Vec<String>, current_time_ms: f64, skip_time_enabled: bool) -> Self {
        Self {
            mode: PauseMenuMode::Main,
            selected: 0,
            overlay_alpha: 0.0,
            difficulty_choices,
            skip_target_ms: current_time_ms.max(0.0),
            skip_time_enabled,
            skip_hold_time: 0.0,
            timer: 0.0,
            pause_music_volume: 0.0,
        }
    }

    pub fn main_items(&self) -> Vec<PauseMenuItem> {
        let mut items = vec![PauseMenuItem::Resume, PauseMenuItem::RestartSong];
        if self.difficulty_choices.len() > 1 { items.push(PauseMenuItem::ChangeDifficulty); }
        items.push(PauseMenuItem::Options);
        if self.skip_time_enabled { items.push(PauseMenuItem::SkipTime); }
        items.push(PauseMenuItem::ExitToMenu);
        items
    }

    pub fn item_count(&self) -> usize {
        match self.mode {
            PauseMenuMode::Main => self.main_items().len(),
            PauseMenuMode::Difficulty => self.difficulty_choices.len() + 1,
        }
    }

    pub fn move_up(&mut self) {
        let count = self.item_count();
        if count > 0 { self.selected = (self.selected + count - 1) % count; }
    }

    pub fn move_down(&mut self) {
        let count = self.item_count();
        if count > 0 { self.selected = (self.selected + 1) % count; }
    }

    pub fn format_skip_time(&self) -> String {
        let secs = (self.skip_target_ms / 1000.0).max(0.0);
        let min = (secs / 60.0) as u32;
        let sec = (secs % 60.0) as u32;
        format!("{}:{:02}", min, sec)
    }

    pub fn adjust_skip_time(&mut self, delta_ms: f64, song_length_ms: f64) {
        self.skip_target_ms = (self.skip_target_ms + delta_ms).clamp(0.0, song_length_ms);
    }

    pub fn update_skip_hold(&mut self, dt: f32, holding: bool) -> f32 {
        if holding {
            self.skip_hold_time += dt;
            if self.skip_hold_time > 0.5 { 45.0 } else { 0.0 }
        } else {
            self.skip_hold_time = 0.0;
            0.0
        }
    }
}

impl PlayScreen {
    pub(super) fn enter_pause(&mut self) {
        if self.pause_menu.is_some() { return; }
        if let Some(audio) = &mut self.audio {
            audio.pause();
            if let Some(sfx) = self.paths.sound("cancelMenu") { audio.play_sound(&sfx, 0.6); }
            if let Some(music) = self.paths.music("breakfast") {
                let duration = AudioEngine::sound_duration_ms(&music).unwrap_or(60_000.0);
                let max_start = (duration / 2.0).max(0.0);
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.subsec_nanos() as f64)
                    .unwrap_or(0.0);
                let start_ms = if max_start > 1.0 { seed % max_start } else { 0.0 };
                audio.play_loop_music_from(&music, 0.0, start_ms);
            }
        }
        let difficulties = self.get_available_difficulties();
        let skip_time_enabled = self.practice_mode;
        let current_time = self.game.conductor.song_position;
        self.pause_menu = Some(PauseMenuState::new(difficulties, current_time, skip_time_enabled));
    }

    pub(super) fn resume_from_pause(&mut self) {
        self.pause_menu = None;
        self.pause_skip_direction = 0;
        if let Some(audio) = &mut self.audio {
            audio.stop_loop_music();
            if self.game.song_started { audio.play(); }
            if let Some(sfx) = self.paths.sound("confirmMenu") { audio.play_sound(&sfx, 0.4); }
        }
    }

    fn get_available_difficulties(&self) -> Vec<String> {
        let mut difficulties = Vec::new();
        let song_name = &self.song_name;
        for diff in &["easy", "normal", "hard"] {
            if self.paths.chart(song_name, diff).is_some() { difficulties.push(diff.to_string()); }
        }
        if difficulties.is_empty() { difficulties.push(self.difficulty.clone()); }
        difficulties
    }

    /// Handle pause menu input. Returns true if input was consumed.
    pub(super) fn handle_pause_input(&mut self, key: winit::keyboard::KeyCode) -> bool {
        use winit::keyboard::KeyCode;

        let song_length_ms = self.get_song_length_ms();
        let Some(pause_menu) = &mut self.pause_menu else { return false; };

        match pause_menu.mode {
            PauseMenuMode::Main => {
                let items = pause_menu.main_items();
                match key {
                    KeyCode::Escape => {
                        self.resume_from_pause();
                        return true;
                    }
                    KeyCode::ArrowUp | KeyCode::KeyW => {
                        pause_menu.move_up();
                        self.play_scroll_sound();
                    }
                    KeyCode::ArrowDown | KeyCode::KeyS => {
                        pause_menu.move_down();
                        self.play_scroll_sound();
                    }
                    KeyCode::ArrowLeft | KeyCode::KeyA => {
                        // Adjust skip time if on that item
                        if let Some(PauseMenuItem::SkipTime) = items.get(pause_menu.selected) {
                            self.pause_skip_direction = -1;
                            pause_menu.adjust_skip_time(-1000.0, song_length_ms);
                            self.play_scroll_sound();
                        }
                    }
                    KeyCode::ArrowRight | KeyCode::KeyD => {
                        if let Some(PauseMenuItem::SkipTime) = items.get(pause_menu.selected) {
                            self.pause_skip_direction = 1;
                            pause_menu.adjust_skip_time(1000.0, song_length_ms);
                            self.play_scroll_sound();
                        }
                    }
                    KeyCode::Enter | KeyCode::Space => {
                        if let Some(item) = items.get(pause_menu.selected).copied() {
                            match item {
                                PauseMenuItem::Resume => {
                                    self.resume_from_pause();
                                }
                                PauseMenuItem::RestartSong => {
                                    self.play_confirm_sound();
                                    self.wants_restart = true;
                                }
                                PauseMenuItem::ChangeDifficulty => {
                                    // Find current difficulty index
                                    let current_idx = pause_menu.difficulty_choices
                                        .iter()
                                        .position(|d| d == &self.difficulty)
                                        .unwrap_or(0);
                                    pause_menu.mode = PauseMenuMode::Difficulty;
                                    pause_menu.selected = current_idx;
                                    self.play_scroll_sound();
                                }
                                PauseMenuItem::Options => {
                                    self.play_confirm_sound();
                                    self.pending_options_open = true;
                                }
                                PauseMenuItem::SkipTime => {
                                    let target = pause_menu.skip_target_ms;
                                    self.skip_to(target);
                                    self.resume_from_pause();
                                }
                                PauseMenuItem::ExitToMenu => {
                                    self.play_cancel_sound();
                                    self.game.song_ended = true;
                                    self.pause_menu = None;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            PauseMenuMode::Difficulty => {
                let choices_len = pause_menu.difficulty_choices.len();
                match key {
                    KeyCode::Escape => {
                        pause_menu.mode = PauseMenuMode::Main;
                        pause_menu.selected = 0;
                        self.play_cancel_sound();
                    }
                    KeyCode::ArrowUp | KeyCode::KeyW => {
                        pause_menu.move_up();
                        self.play_scroll_sound();
                    }
                    KeyCode::ArrowDown | KeyCode::KeyS => {
                        pause_menu.move_down();
                        self.play_scroll_sound();
                    }
                    KeyCode::Enter | KeyCode::Space => {
                        if pause_menu.selected >= choices_len {
                            // BACK selected
                            pause_menu.mode = PauseMenuMode::Main;
                            pause_menu.selected = 0;
                            self.play_cancel_sound();
                        } else {
                            // Difficulty selected - restart with new difficulty
                            let new_diff = pause_menu.difficulty_choices[pause_menu.selected].clone();
                            self.difficulty = new_diff;
                            self.play_confirm_sound();
                            self.wants_restart = true;
                        }
                    }
                    _ => {}
                }
            }
        }

        true
    }

    /// Get approximate song length in milliseconds.
    fn get_song_length_ms(&self) -> f64 {
        // Use the last note's strum time + sustain as an approximation
        self.game.notes.last()
            .map(|n| n.strum_time + n.sustain_length + 2000.0)
            .unwrap_or(180000.0) // Default to 3 minutes
    }

    fn play_scroll_sound(&mut self) {
        if let Some(audio) = &mut self.audio {
            if let Some(sfx) = self.paths.sound("scrollMenu") {
                audio.play_sound(&sfx, 0.4);
            }
        }
    }

    fn play_confirm_sound(&mut self) {
        if let Some(audio) = &mut self.audio {
            if let Some(sfx) = self.paths.sound("confirmMenu") {
                audio.play_sound(&sfx, 0.7);
            }
        }
    }

    fn play_cancel_sound(&mut self) {
        if let Some(audio) = &mut self.audio {
            if let Some(sfx) = self.paths.sound("cancelMenu") {
                audio.play_sound(&sfx, 0.7);
            }
        }
    }

    /// Update pause menu state (call each frame when paused).
    pub(super) fn update_pause(&mut self, dt: f32) {
        if self.pending_options_open && self.options_menu.is_none() {
            self.options_menu = Some(crate::screens::options::OptionsMenuState::load());
            self.pending_options_open = false;
        }
        if let Some(menu) = &mut self.options_menu {
            crate::screens::options::update(menu, dt);
            return;
        }
        let song_length_ms = self.get_song_length_ms();
        if let Some(pause_menu) = &mut self.pause_menu {
            pause_menu.timer += dt;
            let t = (pause_menu.timer / 0.4).clamp(0.0, 1.0);
            let eased = if t < 0.5 {
                8.0 * t * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
            };
            pause_menu.overlay_alpha = 0.6 * eased;
            pause_menu.pause_music_volume = (pause_menu.pause_music_volume + 0.01 * dt).min(0.5);
            if let Some(audio) = &mut self.audio {
                audio.set_loop_music_volume(pause_menu.pause_music_volume as f64);
            }
            if self.pause_skip_direction != 0 {
                let speed = pause_menu.update_skip_hold(dt, true);
                if speed > 0.0 {
                    pause_menu.adjust_skip_time(
                        1000.0 * speed as f64 * dt as f64 * self.pause_skip_direction as f64,
                        song_length_ms,
                    );
                }
            } else {
                pause_menu.update_skip_hold(dt, false);
            }
        }
    }

    /// Draw the pause menu overlay.
    pub(super) fn draw_pause(&self, gpu: &mut GpuState) {
        if let Some(options_menu) = &self.options_menu {
            options_menu.draw(gpu);
            return;
        }
        let Some(pause_menu) = &self.pause_menu else {
            return;
        };

        let white = [1.0, 1.0, 1.0, 1.0];
        let gray = [0.7, 0.7, 0.7, 1.0];
        let dark_gray = [0.5, 0.5, 0.5, 1.0];

        // Semi-transparent overlay
        gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.0, pause_menu.overlay_alpha]);
        gpu.draw_batch(None);

        // Menu box background
        let box_x = 80.0;
        let box_y = 160.0;
        let box_w = 400.0;
        let box_h = 340.0;
        gpu.push_colored_quad(box_x, box_y, box_w, box_h, [0.0, 0.0, 0.0, 0.55]);
        gpu.draw_batch(None);

        // Song info (top right)
        let song_display = self.song_name.replace('-', " ");
        let info_alpha = ((pause_menu.timer - 0.3) / 0.4).clamp(0.0, 1.0);
        let diff_alpha = ((pause_menu.timer - 0.5) / 0.4).clamp(0.0, 1.0);
        let blue_alpha = ((pause_menu.timer - 0.7) / 0.4).clamp(0.0, 1.0);
        let info_x = GAME_W - 300.0 + (1.0 - info_alpha) * 120.0;
        let diff_x = GAME_W - 300.0 + (1.0 - diff_alpha) * 120.0;
        let blue_x = GAME_W - 300.0 + (1.0 - blue_alpha) * 120.0;
        gpu.draw_text(&song_display, info_x, 24.0, 28.0, [1.0, 1.0, 1.0, info_alpha]);
        gpu.draw_text(&self.difficulty.to_uppercase(), diff_x, 56.0, 22.0, [0.7, 0.7, 0.7, diff_alpha]);

        // Death counter
        let blueballed = format!("Blueballed: {}", self.death_counter);
        gpu.draw_text(&blueballed, blue_x, 84.0, 20.0, [0.5, 0.5, 0.5, blue_alpha]);

        // Title
        let title = match pause_menu.mode {
            PauseMenuMode::Main => "PAUSED",
            PauseMenuMode::Difficulty => "SELECT DIFFICULTY",
        };
        gpu.draw_text(title, box_x + 20.0, box_y + 16.0, 32.0, white);

        // Menu items
        let item_start_y = box_y + 70.0;
        let item_height = 38.0;

        match pause_menu.mode {
            PauseMenuMode::Main => {
                let items = pause_menu.main_items();
                for (i, item) in items.iter().enumerate() {
                    let y = item_start_y + i as f32 * item_height;
                    let is_selected = i == pause_menu.selected;

                    // Selection highlight
                    if is_selected {
                        gpu.push_colored_quad(
                            box_x + 12.0, y - 4.0,
                            box_w - 24.0, item_height - 4.0,
                            [1.0, 1.0, 1.0, 0.12],
                        );
                        gpu.draw_batch(None);
                    }

                    let color = if is_selected { white } else { gray };
                    let prefix = if is_selected { "> " } else { "  " };

                    // Special formatting for skip time
                    let label = if *item == PauseMenuItem::SkipTime {
                        format!("{}Skip Time  < {} >", prefix, pause_menu.format_skip_time())
                    } else {
                        format!("{}{}", prefix, item.label())
                    };

                    gpu.draw_text(&label, box_x + 24.0, y, 26.0, color);
                }
            }
            PauseMenuMode::Difficulty => {
                for (i, diff) in pause_menu.difficulty_choices.iter().enumerate() {
                    let y = item_start_y + i as f32 * item_height;
                    let is_selected = i == pause_menu.selected;

                    if is_selected {
                        gpu.push_colored_quad(
                            box_x + 12.0, y - 4.0,
                            box_w - 24.0, item_height - 4.0,
                            [1.0, 1.0, 1.0, 0.12],
                        );
                        gpu.draw_batch(None);
                    }

                    let color = if is_selected { white } else { gray };
                    let prefix = if is_selected { "> " } else { "  " };
                    let label = format!("{}{}", prefix, diff.to_uppercase());
                    gpu.draw_text(&label, box_x + 24.0, y, 26.0, color);
                }

                // BACK option
                let back_idx = pause_menu.difficulty_choices.len();
                let back_y = item_start_y + back_idx as f32 * item_height;
                let is_back_selected = pause_menu.selected == back_idx;

                if is_back_selected {
                    gpu.push_colored_quad(
                        box_x + 12.0, back_y - 4.0,
                        box_w - 24.0, item_height - 4.0,
                        [1.0, 1.0, 1.0, 0.12],
                    );
                    gpu.draw_batch(None);
                }

                let color = if is_back_selected { white } else { gray };
                let prefix = if is_back_selected { "> " } else { "  " };
                gpu.draw_text(&format!("{}BACK", prefix), box_x + 24.0, back_y, 26.0, color);
            }
        }

        // Bottom hint
        let hint = match pause_menu.mode {
            PauseMenuMode::Main => "ESC: Resume  ENTER: Select",
            PauseMenuMode::Difficulty => "ESC: Back",
        };
        gpu.draw_text(hint, box_x + 20.0, box_y + box_h - 32.0, 18.0, dark_gray);
    }
}
