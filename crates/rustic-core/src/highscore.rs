use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::config_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighscoreEntry {
    pub score: i32,
    pub accuracy: f32,
    pub full_combo: bool,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HighscoreStore {
    /// Map of "song:difficulty" -> entry.
    songs: HashMap<String, HighscoreEntry>,
    /// Map of "week:difficulty" -> total score.
    weeks: HashMap<String, i32>,
}

impl HighscoreStore {
    pub fn load() -> Self {
        let path = Self::path();
        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&contents).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                eprintln!("Failed to create highscore directory {:?}: {err}", parent);
                return;
            }
        }
        let Ok(json) = serde_json::to_string_pretty(self) else {
            eprintln!("Failed to serialize highscores");
            return;
        };
        if let Err(err) = fs::write(&path, json) {
            eprintln!("Failed to save highscores {:?}: {err}", path);
        }
    }

    pub fn get_score(&self, song: &str, diff: &str) -> Option<&HighscoreEntry> {
        self.songs.get(&song_key(song, diff))
    }

    pub fn save_score(&mut self, song: &str, diff: &str, score: i32, accuracy: f32, fc: bool) {
        let entry = HighscoreEntry {
            score,
            accuracy,
            full_combo: fc,
            timestamp: now_secs(),
        };
        let key = song_key(song, diff);
        let should_replace = self
            .songs
            .get(&key)
            .map(|current| should_replace(current, &entry))
            .unwrap_or(true);
        if should_replace {
            self.songs.insert(key, entry);
        }
    }

    pub fn reset_score(&mut self, song: &str, diff: &str) {
        self.songs.remove(&song_key(song, diff));
    }

    pub fn get_week_score(&self, week: &str, diff: &str) -> i32 {
        self.weeks
            .get(&week_key(week, diff))
            .copied()
            .unwrap_or_default()
    }

    pub fn add_week_score(&mut self, week: &str, diff: &str, score: i32) {
        let key = week_key(week, diff);
        let total = self.weeks.entry(key).or_default();
        *total += score;
    }

    pub fn reset_week(&mut self, week: &str, diff: &str) {
        self.weeks.remove(&week_key(week, diff));
    }

    fn path() -> PathBuf {
        config_dir().join("highscores.json")
    }
}

fn song_key(song: &str, diff: &str) -> String {
    format!("{}:{}", normalize_key(song), normalize_key(diff))
}

fn week_key(week: &str, diff: &str) -> String {
    format!("{}:{}", normalize_key(week), normalize_key(diff))
}

fn normalize_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn should_replace(current: &HighscoreEntry, next: &HighscoreEntry) -> bool {
    next.score > current.score
        || (next.score == current.score && next.accuracy > current.accuracy)
        || (next.score == current.score
            && (next.accuracy - current.accuracy).abs() < f32::EPSILON
            && next.full_combo
            && !current.full_combo)
}
