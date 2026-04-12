use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
        Self::default()
    }

    pub fn save(&self) {}

    pub fn get_score(&self, song: &str, diff: &str) -> Option<&HighscoreEntry> {
        self.songs.get(&song_key(song, diff))
    }

    pub fn save_score(&mut self, song: &str, diff: &str, score: i32, accuracy: f32, fc: bool) {
        let entry = HighscoreEntry {
            score,
            accuracy,
            full_combo: fc,
            timestamp: 0,
        };
        self.songs.insert(song_key(song, diff), entry);
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
