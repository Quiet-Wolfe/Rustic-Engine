use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::config_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    pub downscroll: bool,
    pub ghost_tapping: bool,
    pub note_offset: i32,
    pub safe_frames: i32,
    pub antialiasing: bool,
    pub flashing_lights: bool,
    #[serde(default)]
    pub fps_counter: bool,
    #[serde(default = "default_fps_cap")]
    pub fps_cap: u32,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default = "default_master_volume")]
    pub master_volume: f32,
    #[serde(default = "default_music_volume")]
    pub music_volume: f32,
    #[serde(default = "default_sfx_volume")]
    pub sfx_volume: f32,
    #[serde(default = "default_note_left")]
    pub note_left: String,
    #[serde(default = "default_note_down")]
    pub note_down: String,
    #[serde(default = "default_note_up")]
    pub note_up: String,
    #[serde(default = "default_note_right")]
    pub note_right: String,
}

impl Preferences {
    pub fn load() -> Self {
        let path = Self::path();
        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(mut prefs) = serde_json::from_str::<Self>(&contents) else {
            return Self::default();
        };
        prefs.normalize();
        prefs
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Failed to create config directory {:?}: {err}", parent))?;
        }

        let mut prefs = self.clone();
        prefs.normalize();
        let json = serde_json::to_string_pretty(&prefs)
            .map_err(|err| format!("Failed to serialize preferences: {err}"))?;
        fs::write(&path, json)
            .map_err(|err| format!("Failed to write preferences {:?}: {err}", path))?;
        Ok(())
    }

    pub fn path() -> PathBuf {
        config_dir().join("preferences.json")
    }

    pub fn normalize(&mut self) {
        self.note_offset = self.note_offset.clamp(-500, 500);
        self.safe_frames = self.safe_frames.clamp(1, 10);
        self.fps_cap = normalize_fps_cap(self.fps_cap);
        self.master_volume = self.master_volume.clamp(0.0, 1.0);
        self.music_volume = self.music_volume.clamp(0.0, 1.0);
        self.sfx_volume = self.sfx_volume.clamp(0.0, 1.0);
        if self.note_left.is_empty() {
            self.note_left = default_note_left();
        }
        if self.note_down.is_empty() {
            self.note_down = default_note_down();
        }
        if self.note_up.is_empty() {
            self.note_up = default_note_up();
        }
        if self.note_right.is_empty() {
            self.note_right = default_note_right();
        }
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            downscroll: false,
            ghost_tapping: true,
            note_offset: 0,
            safe_frames: 10,
            antialiasing: true,
            flashing_lights: true,
            fps_counter: false,
            fps_cap: default_fps_cap(),
            fullscreen: false,
            master_volume: default_master_volume(),
            music_volume: default_music_volume(),
            sfx_volume: default_sfx_volume(),
            note_left: default_note_left(),
            note_down: default_note_down(),
            note_up: default_note_up(),
            note_right: default_note_right(),
        }
    }
}

fn normalize_fps_cap(value: u32) -> u32 {
    match value {
        0 | 30 | 60 | 120 | 240 => value,
        1..=45 => 30,
        46..=90 => 60,
        91..=180 => 120,
        _ => 240,
    }
}

fn default_fps_cap() -> u32 {
    120
}

fn default_master_volume() -> f32 {
    1.0
}

fn default_music_volume() -> f32 {
    1.0
}

fn default_sfx_volume() -> f32 {
    1.0
}

fn default_note_left() -> String {
    "KeyD".to_string()
}

fn default_note_down() -> String {
    "KeyF".to_string()
}

fn default_note_up() -> String {
    "KeyJ".to_string()
}

fn default_note_right() -> String {
    "KeyK".to_string()
}
