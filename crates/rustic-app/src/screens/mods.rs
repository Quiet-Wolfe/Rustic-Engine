use std::fs;

use winit::keyboard::KeyCode;

use rustic_audio::AudioEngine;
use rustic_core::mods::ModLoader;
use rustic_core::paths::AssetPaths;
use rustic_render::gpu::GpuState;

use crate::screen::Screen;

const GAME_W: f32 = 1280.0;
const GAME_H: f32 = 720.0;

/// Mod manager screen - enable/disable mods and change load order.
pub struct ModsScreen {
    audio: Option<AudioEngine>,
    mods: Vec<ModEntry>,
    selected: usize,
    holding: bool, // Holding Enter to drag/reorder
    needs_restart: bool,
    next: Option<Box<dyn Screen>>,
}

#[derive(Clone)]
struct ModEntry {
    name: String,
    description: String,
    color: [u8; 3],
    enabled: bool,
}

impl ModsScreen {
    pub fn new() -> Self {
        let loader = ModLoader::from_environment();
        let mods: Vec<ModEntry> = scan_all_mods(&loader);

        Self {
            audio: None,
            mods,
            selected: 0,
            holding: false,
            needs_restart: false,
            next: None,
        }
    }

    fn change_selection(&mut self, delta: i32) {
        if self.mods.is_empty() {
            return;
        }
        let len = self.mods.len() as i32;
        self.selected = ((self.selected as i32 + delta).rem_euclid(len)) as usize;

        if let Some(audio) = &mut self.audio {
            let paths = AssetPaths::platform_default();
            if let Some(sfx) = paths.sound("scrollMenu") {
                audio.play_sound(&sfx, 0.7);
            }
        }
    }

    fn toggle_current(&mut self) {
        if let Some(entry) = self.mods.get_mut(self.selected) {
            entry.enabled = !entry.enabled;
            self.needs_restart = true;
            if let Some(audio) = &mut self.audio {
                let paths = AssetPaths::platform_default();
                if let Some(sfx) = paths.sound("scrollMenu") {
                    audio.play_sound(&sfx, 0.7);
                }
            }
        }
    }

    fn move_current(&mut self, delta: i32) {
        if self.mods.len() < 2 {
            return;
        }
        let len = self.mods.len() as i32;
        let new_pos = ((self.selected as i32 + delta).rem_euclid(len)) as usize;
        if new_pos != self.selected {
            self.mods.swap(self.selected, new_pos);
            self.selected = new_pos;
            self.needs_restart = true;
            if let Some(audio) = &mut self.audio {
                let paths = AssetPaths::platform_default();
                if let Some(sfx) = paths.sound("scrollMenu") {
                    audio.play_sound(&sfx, 0.7);
                }
            }
        }
    }

    fn save_mods_list(&self) {
        let loader = ModLoader::from_environment();
        let path = loader.mods_list_path();

        let mut content = String::new();
        for entry in &self.mods {
            if !content.is_empty() {
                content.push('\n');
            }
            let enabled_flag = if entry.enabled { "1" } else { "0" };
            content.push_str(&format!("{}|{}", entry.name, enabled_flag));
        }

        if let Err(e) = fs::write(&path, &content) {
            log::error!("Failed to save modsList.txt: {}", e);
        } else {
            log::info!("Saved mod list to {:?}", path);
        }
    }

    fn enable_all(&mut self) {
        for entry in &mut self.mods {
            entry.enabled = true;
        }
        self.needs_restart = true;
    }

    fn disable_all(&mut self) {
        for entry in &mut self.mods {
            entry.enabled = false;
        }
        self.needs_restart = true;
    }
}

impl Screen for ModsScreen {
    fn init(&mut self, _gpu: &GpuState) {
        // Audio - continue freakyMenu
        if self.audio.is_none() {
            let paths = AssetPaths::platform_default();
            if let Some(music) = paths.music("freakyMenu") {
                let mut audio = AudioEngine::new();
                audio.play_loop_music_vol(&music, 0.7);
                self.audio = Some(audio);
            }
        }
    }

    fn handle_key(&mut self, key: KeyCode) {
        if self.holding {
            // In reorder mode
            match key {
                KeyCode::ArrowUp | KeyCode::KeyW => self.move_current(-1),
                KeyCode::ArrowDown | KeyCode::KeyS => self.move_current(1),
                KeyCode::Enter | KeyCode::Space | KeyCode::Escape => {
                    self.holding = false;
                }
                _ => {}
            }
            return;
        }

        match key {
            KeyCode::ArrowUp | KeyCode::KeyW => self.change_selection(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => self.change_selection(1),
            KeyCode::ArrowLeft | KeyCode::KeyA => self.toggle_current(),
            KeyCode::ArrowRight | KeyCode::KeyD => self.toggle_current(),
            KeyCode::Enter | KeyCode::Space => {
                // Hold to reorder
                self.holding = true;
            }
            KeyCode::KeyE => self.enable_all(),
            KeyCode::KeyQ => self.disable_all(),
            KeyCode::Escape | KeyCode::Backspace => {
                self.save_mods_list();
                if let Some(audio) = &mut self.audio {
                    let paths = AssetPaths::platform_default();
                    if let Some(sfx) = paths.sound("cancelMenu") {
                        audio.play_sound(&sfx, 0.7);
                    }
                }
                self.next = Some(Box::new(super::main_menu::MainMenuScreen::new()));
            }
            _ => {}
        }
    }

    fn update(&mut self, _dt: f32) {}

    fn draw(&mut self, gpu: &mut GpuState) {
        if !gpu.begin_frame() {
            return;
        }

        let white = [1.0, 1.0, 1.0, 1.0];
        let gray = [0.6, 0.6, 0.6, 1.0];
        let yellow = [1.0, 0.95, 0.5, 1.0];
        let green = [0.4, 1.0, 0.4, 1.0];
        let red = [1.0, 0.4, 0.4, 1.0];
        let cyan = [0.4, 1.0, 1.0, 1.0];

        // Background
        let bg_color = if let Some(entry) = self.mods.get(self.selected) {
            let [r, g, b] = entry.color;
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
        } else {
            [0.4, 0.35, 1.0, 1.0] // Default purple
        };
        gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, bg_color);
        gpu.draw_batch(None);

        // Title
        let title = if self.holding { "MODS - REORDERING" } else { "MODS" };
        gpu.draw_text(title, GAME_W / 2.0 - 60.0, 30.0, 36.0, white);

        // Mod list panel (left side)
        gpu.push_colored_quad(30.0, 80.0, 400.0, 560.0, [0.0, 0.0, 0.0, 0.6]);
        gpu.draw_batch(None);

        if self.mods.is_empty() {
            gpu.draw_text("No mods found.", 50.0, 120.0, 24.0, gray);
            gpu.draw_text("Place mods in the 'mods' folder.", 50.0, 160.0, 20.0, gray);
        } else {
            // Show mods with scrolling (5 visible at a time)
            let visible_count = 6;
            let start = if self.selected >= visible_count {
                self.selected - visible_count + 1
            } else {
                0
            };

            for (i, entry) in self.mods.iter().enumerate().skip(start).take(visible_count + 1) {
                let y = 100.0 + (i - start) as f32 * 80.0;
                let is_selected = i == self.selected;

                // Selection highlight
                if is_selected {
                    let highlight_color = if self.holding {
                        [0.2, 0.6, 1.0, 0.4]
                    } else {
                        [1.0, 1.0, 1.0, 0.2]
                    };
                    gpu.push_colored_quad(35.0, y - 5.0, 390.0, 75.0, highlight_color);
                    gpu.draw_batch(None);
                }

                // Enable/disable indicator
                let status_color = if entry.enabled { green } else { red };
                let status_text = if entry.enabled { "[ON]" } else { "[OFF]" };
                gpu.draw_text(status_text, 50.0, y + 5.0, 18.0, status_color);

                // Mod name
                let name_color = if is_selected {
                    if self.holding { cyan } else { yellow }
                } else if entry.enabled {
                    white
                } else {
                    gray
                };
                gpu.draw_text(&entry.name, 110.0, y + 5.0, 22.0, name_color);

                // Priority number
                let priority = format!("#{}", i + 1);
                gpu.draw_text(&priority, 370.0, y + 5.0, 16.0, gray);
            }
        }

        // Description panel (right side)
        gpu.push_colored_quad(450.0, 80.0, 800.0, 400.0, [0.0, 0.0, 0.0, 0.6]);
        gpu.draw_batch(None);

        if let Some(entry) = self.mods.get(self.selected) {
            gpu.draw_text(&entry.name, 470.0, 100.0, 32.0, yellow);

            // Word-wrap description
            let desc = if entry.description.is_empty() {
                "No description provided."
            } else {
                &entry.description
            };
            let max_chars_per_line = 60;
            let mut y = 160.0;
            for line in word_wrap(desc, max_chars_per_line) {
                gpu.draw_text(&line, 470.0, y, 20.0, white);
                y += 28.0;
            }
        }

        // Controls help
        gpu.push_colored_quad(450.0, 500.0, 800.0, 140.0, [0.0, 0.0, 0.0, 0.6]);
        gpu.draw_batch(None);

        if self.holding {
            gpu.draw_text("REORDER MODE", 470.0, 520.0, 24.0, cyan);
            gpu.draw_text("UP/DOWN - Move mod in priority", 470.0, 560.0, 18.0, white);
            gpu.draw_text("ENTER/ESC - Stop reordering", 470.0, 590.0, 18.0, white);
        } else {
            gpu.draw_text("CONTROLS", 470.0, 520.0, 24.0, yellow);
            gpu.draw_text("UP/DOWN - Select | LEFT/RIGHT - Toggle", 470.0, 555.0, 18.0, white);
            gpu.draw_text("ENTER - Reorder mode | Q - Disable all | E - Enable all", 470.0, 585.0, 18.0, white);
            gpu.draw_text("ESC - Save and exit", 470.0, 615.0, 18.0, white);
        }

        // Restart warning
        if self.needs_restart {
            gpu.draw_text(
                "* Changes require game restart to take effect",
                30.0,
                GAME_H - 40.0,
                18.0,
                yellow,
            );
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

/// Scan all mod directories and return entries with their pack.json info.
fn scan_all_mods(loader: &ModLoader) -> Vec<ModEntry> {
    let mods_dir = loader.mods_dir();
    let Ok(entries) = fs::read_dir(mods_dir) else {
        return Vec::new();
    };

    let mut result = Vec::new();

    // First, add mods in the order they appear in active_mods (preserving priority)
    for mod_info in loader.active_mods() {
        result.push(ModEntry {
            name: mod_info.name.clone(),
            description: mod_info
                .pack_json
                .as_ref()
                .map(|p| p.description.clone())
                .unwrap_or_default(),
            color: mod_info
                .pack_json
                .as_ref()
                .and_then(|p| p.color)
                .unwrap_or([102, 90, 255]),
            enabled: mod_info.enabled,
        });
    }

    // Then add any mods found in directory but not in the list
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip if already added
        if result.iter().any(|m| m.name == name) {
            continue;
        }

        // Check if it's a valid mod directory
        let pack_json_path = path.join("pack.json");
        let pack_json = if pack_json_path.exists() {
            fs::read_to_string(&pack_json_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
        } else {
            None
        };

        result.push(ModEntry {
            name,
            description: pack_json
                .as_ref()
                .map(|p: &rustic_core::mods::PackJson| p.description.clone())
                .unwrap_or_default(),
            color: pack_json
                .as_ref()
                .and_then(|p| p.color)
                .unwrap_or([102, 90, 255]),
            enabled: true, // New mods default to enabled
        });
    }

    result
}

/// Simple word-wrap for descriptions.
fn word_wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= max_chars {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}
