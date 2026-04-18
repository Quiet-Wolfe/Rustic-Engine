use winit::keyboard::KeyCode;

use rustic_core::prefs::Preferences;
use rustic_render::gpu::GpuState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionsCategory {
    Gameplay,
    Visuals,
    Audio,
    Controls,
}

impl OptionsCategory {
    pub fn all() -> [Self; 4] {
        [Self::Gameplay, Self::Visuals, Self::Audio, Self::Controls]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Gameplay => "GAMEPLAY",
            Self::Visuals => "VISUALS",
            Self::Audio => "AUDIO",
            Self::Controls => "CONTROLS",
        }
    }
}

#[derive(Debug, Clone)]
pub struct OptionsMenuState {
    pub prefs: Preferences,
    pub category: OptionsCategory,
    pub selected: usize,
    pub waiting_for_rebind: Option<usize>,
    pub calibrating: bool,
    pub calibration_timer: f32,
    pub calibration_samples: Vec<i32>,
}

impl OptionsMenuState {
    pub fn load() -> Self {
        Self {
            prefs: Preferences::load(),
            category: OptionsCategory::Gameplay,
            selected: 0,
            waiting_for_rebind: None,
            calibrating: false,
            calibration_timer: 0.0,
            calibration_samples: Vec::new(),
        }
    }

    pub fn save(&self) {
        let _ = self.prefs.save();
        crate::settings::apply_preferences(&self.prefs);
    }

    pub fn item_count(&self) -> usize {
        match self.category {
            OptionsCategory::Gameplay => 4,
            OptionsCategory::Visuals => 4,
            OptionsCategory::Audio => 4,
            OptionsCategory::Controls => 4,
        }
    }

    pub fn move_up(&mut self) {
        let count = self.item_count();
        self.selected = (self.selected + count - 1) % count;
    }

    pub fn move_down(&mut self) {
        self.selected = (self.selected + 1) % self.item_count();
    }

    pub fn prev_category(&mut self) {
        let all = OptionsCategory::all();
        let idx = all.iter().position(|c| *c == self.category).unwrap_or(0);
        self.category = all[(idx + all.len() - 1) % all.len()];
        self.selected = 0;
    }

    pub fn next_category(&mut self) {
        let all = OptionsCategory::all();
        let idx = all.iter().position(|c| *c == self.category).unwrap_or(0);
        self.category = all[(idx + 1) % all.len()];
        self.selected = 0;
    }

    pub fn adjust_current(&mut self, delta: i32) {
        match self.category {
            OptionsCategory::Gameplay => match self.selected {
                0 => toggle_if_nonzero(&mut self.prefs.downscroll, delta),
                1 => toggle_if_nonzero(&mut self.prefs.ghost_tapping, delta),
                2 => self.prefs.note_offset = (self.prefs.note_offset + delta * 5).clamp(-500, 500),
                3 => self.prefs.safe_frames = (self.prefs.safe_frames + delta).clamp(1, 10),
                _ => {}
            },
            OptionsCategory::Visuals => match self.selected {
                0 => toggle_if_nonzero(&mut self.prefs.antialiasing, delta),
                1 => toggle_if_nonzero(&mut self.prefs.flashing_lights, delta),
                2 => toggle_if_nonzero(&mut self.prefs.fps_counter, delta),
                3 => self.prefs.fps_cap = cycle_fps(self.prefs.fps_cap, delta),
                _ => {}
            },
            OptionsCategory::Audio => match self.selected {
                0 => self.prefs.master_volume = adjust_percent(self.prefs.master_volume, delta),
                1 => self.prefs.music_volume = adjust_percent(self.prefs.music_volume, delta),
                2 => self.prefs.sfx_volume = adjust_percent(self.prefs.sfx_volume, delta),
                3 => {}
                _ => {}
            },
            OptionsCategory::Controls => {}
        }
        self.prefs.normalize();
        crate::settings::apply_preferences(&self.prefs);
    }

    pub fn draw(&self, gpu: &mut GpuState) {
        let white = [1.0, 1.0, 1.0, 1.0];
        let gray = [0.7, 0.7, 0.7, 1.0];
        let yellow = [1.0, 0.95, 0.5, 1.0];
        gpu.push_colored_quad(140.0, 100.0, 1000.0, 520.0, [0.0, 0.0, 0.0, 0.86]);
        gpu.draw_batch(None);

        let mut tab_x = 180.0;
        for category in OptionsCategory::all() {
            let color = if category == self.category { yellow } else { gray };
            gpu.draw_text(category.label(), tab_x, 130.0, 28.0, color);
            tab_x += 210.0;
        }

        let black = [0.0, 0.0, 0.0, 1.0];
        for (i, line) in self.lines().iter().enumerate() {
            let y = 220.0 + i as f32 * 58.0;
            // Draw highlight background for selected item
            if i == self.selected {
                gpu.push_colored_quad(180.0, y - 4.0, 820.0, 50.0, [1.0, 1.0, 1.0, 0.9]);
                gpu.draw_batch(None);
            }
            let color = if i == self.selected { black } else { white };
            let prefix = if i == self.selected { "> " } else { "  " };
            gpu.draw_text(&format!("{}{}", prefix, line), 190.0, y, 26.0, color);
        }

        gpu.draw_text("Press ESCAPE to save and return", 190.0, 570.0, 20.0, gray);
        if let Some(index) = self.waiting_for_rebind {
            gpu.draw_text(
                &format!("Press a key for {}", self.control_label(index)),
                190.0,
                535.0,
                20.0,
                yellow,
            );
        }
        if self.calibrating {
            let beat_phase = self.calibration_timer % 0.5;
            let pulse = 1.0 - (beat_phase / 0.5);
            gpu.draw_text("NOTE OFFSET CALIBRATION", 190.0, 505.0, 22.0, yellow);
            gpu.draw_text(
                "Press ENTER on each beat. ESCAPE to finish.",
                190.0,
                535.0,
                20.0,
                [1.0, 1.0, 1.0, 1.0],
            );
            gpu.push_colored_quad(1030.0, 510.0, 42.0, 42.0, [pulse, pulse * 0.5, 0.2, 1.0]);
            gpu.draw_batch(None);
            if !self.calibration_samples.is_empty() {
                gpu.draw_text(
                    &format!("Average: {}ms", self.prefs.note_offset),
                    760.0,
                    535.0,
                    20.0,
                    yellow,
                );
            }
        }
    }

    fn lines(&self) -> Vec<String> {
        match self.category {
            OptionsCategory::Gameplay => vec![
                format!("Downscroll           [ {} ]", on_off(self.prefs.downscroll)),
                format!("Ghost Tapping        [ {} ]", on_off(self.prefs.ghost_tapping)),
                format!("Note Offset          < {}ms >", self.prefs.note_offset),
                format!("Safe Frames          < {} >", self.prefs.safe_frames),
            ],
            OptionsCategory::Visuals => vec![
                format!("Antialiasing         [ {} ]", on_off(self.prefs.antialiasing)),
                format!("Flashing Lights      [ {} ]", on_off(self.prefs.flashing_lights)),
                format!("FPS Counter          [ {} ]", on_off(self.prefs.fps_counter)),
                format!("FPS Cap              < {} >", fps_cap_label(self.prefs.fps_cap)),
            ],
            OptionsCategory::Audio => vec![
                format!("Master Volume        < {}% >", percent(self.prefs.master_volume)),
                format!("Music Volume         < {}% >", percent(self.prefs.music_volume)),
                format!("SFX Volume           < {}% >", percent(self.prefs.sfx_volume)),
                "Note Offset Calibration  < OPEN >".to_string(),
            ],
            OptionsCategory::Controls => vec![
                format!("Left Lane            [ {} ]", key_display(&self.prefs.note_left)),
                format!("Down Lane            [ {} ]", key_display(&self.prefs.note_down)),
                format!("Up Lane              [ {} ]", key_display(&self.prefs.note_up)),
                format!("Right Lane           [ {} ]", key_display(&self.prefs.note_right)),
            ],
        }
    }

    fn control_label(&self, index: usize) -> &'static str {
        match index {
            0 => "Left Lane",
            1 => "Down Lane",
            2 => "Up Lane",
            _ => "Right Lane",
        }
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "ON" } else { "OFF" }
}

fn toggle_if_nonzero(value: &mut bool, delta: i32) {
    if delta != 0 {
        *value = !*value;
    }
}

fn percent(value: f32) -> i32 {
    (value.clamp(0.0, 1.0) * 100.0).round() as i32
}

fn adjust_percent(value: f32, delta: i32) -> f32 {
    (value + delta as f32 * 0.05).clamp(0.0, 1.0)
}

fn cycle_fps(current: u32, delta: i32) -> u32 {
    let values = [30, 60, 120, 240, 0];
    let idx = values.iter().position(|v| *v == current).unwrap_or(2) as i32;
    values[(idx + delta).rem_euclid(values.len() as i32) as usize]
}

fn fps_cap_label(value: u32) -> &'static str {
    match value {
        0 => "Unlimited",
        30 => "30",
        60 => "60",
        120 => "120",
        240 => "240",
        _ => "120",
    }
}

pub fn handle_input(menu: &mut OptionsMenuState, key: KeyCode) -> bool {
    if menu.calibrating {
        match key {
            KeyCode::Escape => menu.calibrating = false,
            KeyCode::Enter | KeyCode::Space => {
                let beat_window = 0.5f32;
                let phase = menu.calibration_timer % beat_window;
                let offset_ms = ((phase / beat_window) * 500.0).round() as i32;
                let signed = if offset_ms > 250 { offset_ms - 500 } else { offset_ms };
                menu.calibration_samples.push(signed);
                let sum: i32 = menu.calibration_samples.iter().sum();
                menu.prefs.note_offset = (sum / menu.calibration_samples.len() as i32).clamp(-500, 500);
            }
            _ => {}
        }
        return true;
    }
    if let Some(slot) = menu.waiting_for_rebind {
        match key {
            KeyCode::Escape => {
                menu.waiting_for_rebind = None;
            }
            _ => {
                let name = format!("{key:?}");
                match slot {
                    0 => menu.prefs.note_left = name,
                    1 => menu.prefs.note_down = name,
                    2 => menu.prefs.note_up = name,
                    _ => menu.prefs.note_right = name,
                }
                menu.waiting_for_rebind = None;
            }
        }
        return true;
    }
    match key {
        KeyCode::ArrowUp | KeyCode::KeyW => menu.move_up(),
        KeyCode::ArrowDown | KeyCode::KeyS => menu.move_down(),
        KeyCode::ArrowLeft | KeyCode::KeyA => menu.adjust_current(-1),
        KeyCode::ArrowRight | KeyCode::KeyD => menu.adjust_current(1),
        KeyCode::KeyQ => menu.prev_category(),
        KeyCode::KeyE | KeyCode::Tab => menu.next_category(),
        KeyCode::Enter | KeyCode::Space => {
            if menu.category == OptionsCategory::Controls {
                menu.waiting_for_rebind = Some(menu.selected);
            } else if menu.category == OptionsCategory::Audio && menu.selected == 3 {
                menu.calibrating = true;
                menu.calibration_timer = 0.0;
                menu.calibration_samples.clear();
            } else {
                menu.adjust_current(1);
            }
        }
        _ => return false,
    }
    true
}

fn key_display(value: &str) -> &str {
    value.strip_prefix("Key").unwrap_or(value)
}

pub fn update(menu: &mut OptionsMenuState, dt: f32) {
    if menu.calibrating {
        menu.calibration_timer += dt;
    }
}
