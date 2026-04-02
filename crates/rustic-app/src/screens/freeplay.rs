use winit::event::TouchPhase;
use winit::keyboard::KeyCode;

use rustic_audio::AudioEngine;
use rustic_core::paths::AssetPaths;
use rustic_core::week;
use rustic_render::gpu::{GpuState, GpuTexture};

use crate::screen::Screen;
use super::play::PlayScreen;

const GAME_W: f32 = 1280.0;
const GAME_H: f32 = 720.0;

/// Convert sRGB color component to linear space for the sRGB surface format.
fn srgb_to_linear(s: f32) -> f32 {
    if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

const DIFFICULTIES: [&str; 3] = ["easy", "normal", "hard"];

/// A song entry in the freeplay list.
struct FreeplaySong {
    name: String,
    #[allow(dead_code)]
    character: String,
    color: [f32; 3],
    #[allow(dead_code)]
    week: usize,
}

pub struct FreeplayScreen {
    audio: Option<AudioEngine>,
    bg_tex: Option<GpuTexture>,
    songs: Vec<FreeplaySong>,
    filtered: Vec<usize>, // indices into songs matching search
    search: String,
    cur_selected: usize,  // index into filtered
    cur_difficulty: usize, // index into DIFFICULTIES
    lerp_selected: f32,
    bg_color: [f32; 3],
    bg_color_target: [f32; 3],
    next: Option<Box<dyn Screen>>,
    confirmed: bool,
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
        }
    }

    fn change_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() { return; }
        let len = self.filtered.len() as i32;
        self.cur_selected = ((self.cur_selected as i32 + delta).rem_euclid(len)) as usize;
        let song_idx = self.filtered[self.cur_selected];
        self.bg_color_target = self.songs[song_idx].color;

        if let Some(audio) = &mut self.audio {
            let paths = AssetPaths::platform_default();
            if let Some(sfx) = paths.sound("scrollMenu") {
                audio.play_sound(&sfx, 0.4);
            }
        }
    }

    fn change_difficulty(&mut self, delta: i32) {
        let len = DIFFICULTIES.len() as i32;
        self.cur_difficulty = ((self.cur_difficulty as i32 + delta).rem_euclid(len)) as usize;
    }

    fn rebuild_filter(&mut self) {
        let query = self.search.to_lowercase();
        self.filtered = (0..self.songs.len())
            .filter(|&i| query.is_empty() || self.songs[i].name.to_lowercase().contains(&query))
            .collect();
        // Keep selection in bounds
        if self.filtered.is_empty() {
            self.cur_selected = 0;
        } else {
            self.cur_selected = self.cur_selected.min(self.filtered.len() - 1);
            let song_idx = self.filtered[self.cur_selected];
            self.bg_color_target = self.songs[song_idx].color;
        }
        self.lerp_selected = self.cur_selected as f32;
    }

    fn jump_to_letter(&mut self, letter: char) {
        let letter_lower = letter.to_lowercase().next().unwrap_or('a');
        // Find first song in filtered list starting with this letter
        for (i, &song_idx) in self.filtered.iter().enumerate() {
            if self.songs[song_idx].name.to_lowercase().starts_with(letter_lower) {
                let delta = i as i32 - self.cur_selected as i32;
                if delta != 0 {
                    self.change_selection(delta);
                }
                return;
            }
        }
    }

    fn key_to_char(key: KeyCode) -> Option<char> {
        match key {
            KeyCode::KeyA => Some('a'), KeyCode::KeyB => Some('b'),
            KeyCode::KeyC => Some('c'), KeyCode::KeyD => Some('d'),
            KeyCode::KeyE => Some('e'), KeyCode::KeyF => Some('f'),
            KeyCode::KeyG => Some('g'), KeyCode::KeyH => Some('h'),
            KeyCode::KeyI => Some('i'), KeyCode::KeyJ => Some('j'),
            KeyCode::KeyK => Some('k'), KeyCode::KeyL => Some('l'),
            KeyCode::KeyM => Some('m'), KeyCode::KeyN => Some('n'),
            KeyCode::KeyO => Some('o'), KeyCode::KeyP => Some('p'),
            KeyCode::KeyQ => Some('q'), KeyCode::KeyR => Some('r'),
            KeyCode::KeyS => Some('s'), KeyCode::KeyT => Some('t'),
            KeyCode::KeyU => Some('u'), KeyCode::KeyV => Some('v'),
            KeyCode::KeyW => Some('w'), KeyCode::KeyX => Some('x'),
            KeyCode::KeyY => Some('y'), KeyCode::KeyZ => Some('z'),
            KeyCode::Digit0 => Some('0'), KeyCode::Digit1 => Some('1'),
            KeyCode::Digit2 => Some('2'), KeyCode::Digit3 => Some('3'),
            KeyCode::Digit4 => Some('4'), KeyCode::Digit5 => Some('5'),
            KeyCode::Digit6 => Some('6'), KeyCode::Digit7 => Some('7'),
            KeyCode::Digit8 => Some('8'), KeyCode::Digit9 => Some('9'),
            KeyCode::Space => Some(' '), KeyCode::Minus => Some('-'),
            _ => None,
        }
    }
}

impl Screen for FreeplayScreen {
    fn init(&mut self, gpu: &GpuState) {
        let paths = AssetPaths::platform_default();

        // Background (desaturated, tinted per-song)
        if let Some(bg_path) = paths.image("menuDesat") {
            self.bg_tex = Some(gpu.load_texture_from_path(&bg_path));
        }

        // Load song list from weeks + direct data/ folder scan
        let mut seen_songs = std::collections::HashSet::new();

        // First: songs from week JSONs (these have colors/characters)
        let mut all_weeks = Vec::new();
        for weeks_dir in paths.all_weeks_dirs() {
            for w in week::load_weeks(&weeks_dir) {
                all_weeks.push(w);
            }
        }
        all_weeks.sort_by(|a, b| a.file_name.cmp(&b.file_name));
        for (week_idx, w) in all_weeks.iter().enumerate() {
            if w.hide_freeplay { continue; }
            for song in &w.songs {
                let key = song.name.to_lowercase().replace(' ', "-");
                seen_songs.insert(key);
                self.songs.push(FreeplaySong {
                    name: song.name.clone(),
                    character: song.character.clone(),
                    color: [
                        srgb_to_linear(song.color[0] as f32 / 255.0),
                        srgb_to_linear(song.color[1] as f32 / 255.0),
                        srgb_to_linear(song.color[2] as f32 / 255.0),
                    ],
                    week: week_idx,
                });
            }
        }

        // Second: discover songs from data/ folders (catches mods without weeks)
        for song_name in paths.discover_songs() {
            if seen_songs.contains(&song_name) { continue; }
            seen_songs.insert(song_name.clone());
            self.songs.push(FreeplaySong {
                name: song_name,
                character: String::new(),
                color: [146, 113, 253].map(|c| srgb_to_linear(c as f32 / 255.0)),
                week: 0,
            });
        }

        self.rebuild_filter();
        if !self.filtered.is_empty() {
            let song_idx = self.filtered[0];
            self.bg_color_target = self.songs[song_idx].color;
            self.bg_color = self.songs[song_idx].color;
        }
        self.lerp_selected = self.cur_selected as f32;

        // Audio (skip if already passed from previous screen)
        if self.audio.is_none() {
            if let Some(music) = paths.music("freakyMenu") {
                let mut audio = AudioEngine::new();
                audio.play_loop_music_vol(&music, 0.7);
                self.audio = Some(audio);
            }
        }
    }

    fn handle_key(&mut self, key: KeyCode) {
        if self.confirmed { return; }

        match key {
            KeyCode::ArrowUp => self.change_selection(-1),
            KeyCode::ArrowDown => self.change_selection(1),
            KeyCode::ArrowLeft => self.change_difficulty(-1),
            KeyCode::ArrowRight => self.change_difficulty(1),
            KeyCode::Enter => {
                if !self.filtered.is_empty() {
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
                    let song_path = song.name.to_lowercase().replace(' ', "-");
                    self.next = Some(Box::new(PlayScreen::new(&song_path, diff)));
                }
            }
            KeyCode::Escape => {
                if !self.search.is_empty() {
                    // First escape clears search
                    self.search.clear();
                    self.rebuild_filter();
                } else {
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
                if let Some(ch) = Self::key_to_char(key) {
                    self.search.push(ch);
                    self.rebuild_filter();
                }
            }
        }
    }

    fn handle_touch(&mut self, _id: u64, phase: TouchPhase, x: f64, y: f64) {
        if phase != TouchPhase::Started || self.confirmed { return; }
        let (x, y) = (x as f32, y as f32);

        // Alphabet strip on the left edge (0-30px, from y=70 to y=GAME_H-30)
        if x < 30.0 && y > 70.0 && y < GAME_H - 30.0 {
            let strip_h = GAME_H - 100.0;
            let t = (y - 70.0) / strip_h;
            let letter_idx = (t * 26.0) as usize;
            let letter = (b'A' + letter_idx.min(25) as u8) as char;
            // Jump to first song starting with this letter
            self.jump_to_letter(letter);
            return;
        }

        // Difficulty area (top-right score box)
        if y < 66.0 && x > GAME_W * 0.7 {
            if x < GAME_W * 0.85 {
                self.handle_key(KeyCode::ArrowLeft);
            } else {
                self.handle_key(KeyCode::ArrowRight);
            }
            return;
        }

        // Tap on a song in the list
        let draw_dist = 6;
        for (i, &_song_idx) in self.filtered.iter().enumerate() {
            let target_y = i as f32 - self.lerp_selected;
            if target_y.abs() > draw_dist as f32 { continue; }
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
        // Smooth scroll — Psych uses exp(-elapsed * 9.6) lerp
        let lerp = (-dt * 9.6).exp();
        self.lerp_selected = self.cur_selected as f32 + (self.lerp_selected - self.cur_selected as f32) * lerp;

        // Lerp background color (1 second tween)
        let color_lerp = 1.0 - (-dt * 3.0).exp();
        for i in 0..3 {
            self.bg_color[i] += (self.bg_color_target[i] - self.bg_color[i]) * color_lerp;
        }
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        if !gpu.begin_frame() { return; }

        // Background with color tint
        if let Some(bg) = &self.bg_tex {
            let c = &self.bg_color;
            let color = [c[0], c[1], c[2], 1.0];
            // screenCenter
            let bw = bg.width as f32;
            let bh = bg.height as f32;
            let x = (GAME_W - bw) / 2.0;
            let y = (GAME_H - bh) / 2.0;
            gpu.push_texture_region(
                bw, bh, 0.0, 0.0, bw, bh,
                x, y, bw, bh,
                false, color,
            );
            gpu.draw_batch(Some(bg));
        } else {
            let c = &self.bg_color;
            gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [c[0], c[1], c[2], 1.0]);
            gpu.draw_batch(None);
        }

        // Song list — Psych Engine Alphabet positioning
        let text_size = 28.0;
        let draw_dist = 6;

        for (i, &song_idx) in self.filtered.iter().enumerate() {
            let target_y = i as f32 - self.lerp_selected;
            if target_y.abs() > draw_dist as f32 { continue; }

            let x = target_y * 20.0 + 90.0;
            let y = target_y * 1.3 * 120.0 + 320.0;
            if y < -50.0 || y > GAME_H + 50.0 { continue; }

            let alpha = if i == self.cur_selected { 1.0 } else { 0.6 };
            let color = [alpha, alpha, alpha, alpha];

            gpu.draw_text(&self.songs[song_idx].name, x, y, text_size, color);
        }

        // Score area (top right) — Psych: FlxG.width * 0.7
        let score_x = GAME_W * 0.7;
        let score_bg_w = GAME_W - score_x + 6.0;
        gpu.push_colored_quad(score_x - 6.0, 0.0, score_bg_w, 66.0, [0.0, 0.0, 0.0, 0.6]);
        gpu.draw_batch(None);

        // Difficulty display with tappable arrows
        let diff_name = DIFFICULTIES[self.cur_difficulty].to_uppercase();
        if DIFFICULTIES.len() > 1 {
            // Draw < and > as separate tappable-looking buttons
            gpu.draw_text("<", score_x, 41.0, 24.0, [1.0, 1.0, 0.4, 0.9]);
            gpu.draw_text(&diff_name, score_x + 20.0, 41.0, 24.0, [1.0, 1.0, 1.0, 1.0]);
            let arrow_x = GAME_W - 20.0;
            gpu.draw_text(">", arrow_x, 41.0, 24.0, [1.0, 1.0, 0.4, 0.9]);
        } else {
            gpu.draw_text(&diff_name, score_x, 41.0, 24.0, [1.0, 1.0, 1.0, 1.0]);
        }

        // Score display
        gpu.draw_text("PERSONAL BEST: 0 (0%)", score_x, 5.0, 24.0, [1.0, 1.0, 1.0, 1.0]);

        // Search bar (top left)
        if !self.search.is_empty() {
            gpu.push_colored_quad(0.0, 0.0, 400.0, 36.0, [0.0, 0.0, 0.0, 0.7]);
            gpu.draw_batch(None);
            let search_display = format!("Search: {}_", self.search);
            gpu.draw_text(&search_display, 10.0, 8.0, 20.0, [1.0, 1.0, 0.4, 1.0]);
        }

        // Bottom bar
        let count_text = if cfg!(target_os = "android") {
            format!("{} songs | Tap song to play | Tap difficulty to change", self.filtered.len())
        } else if self.search.is_empty() {
            format!("{} songs | Type to search | ENTER to play | LEFT-RIGHT difficulty", self.filtered.len())
        } else {
            format!("{}/{} songs | ESC to clear search | ENTER to play", self.filtered.len(), self.songs.len())
        };
        gpu.push_colored_quad(0.0, GAME_H - 26.0, GAME_W, 26.0, [0.0, 0.0, 0.0, 0.6]);
        gpu.draw_batch(None);
        gpu.draw_text(&count_text, 10.0, GAME_H - 22.0, 16.0, [1.0, 1.0, 1.0, 1.0]);

        // Alphabet quick-jump strip (Android touch UI)
        if cfg!(target_os = "android") {
            let strip_x = 2.0;
            let strip_top = 70.0;
            let strip_h = GAME_H - 100.0;
            let letter_h = strip_h / 26.0;
            // Semi-transparent background
            gpu.push_colored_quad(0.0, strip_top, 28.0, strip_h, [0.0, 0.0, 0.0, 0.3]);
            gpu.draw_batch(None);
            for i in 0..26u8 {
                let letter = (b'A' + i) as char;
                let y = strip_top + i as f32 * letter_h;
                gpu.draw_text(
                    &letter.to_string(),
                    strip_x + 4.0, y, letter_h.min(18.0),
                    [1.0, 1.0, 1.0, 0.7],
                );
            }
        }

        gpu.end_frame();
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
