#[derive(Debug, Clone)]
pub struct StorySession {
    pub week_id: String,
    pub playlist: Vec<String>,
    pub song_index: usize,
    pub total_score: i32,
    pub difficulty: String,
}

impl StorySession {
    pub fn new(week_id: &str, playlist: Vec<String>, difficulty: &str) -> Self {
        Self {
            week_id: week_id.to_string(),
            playlist,
            song_index: 0,
            total_score: 0,
            difficulty: difficulty.to_string(),
        }
    }

    pub fn current_song(&self) -> &str {
        self.playlist
            .get(self.song_index)
            .map(String::as_str)
            .unwrap_or_default()
    }

    pub fn advance(&self, score: i32) -> Option<Self> {
        let next_index = self.song_index + 1;
        if next_index >= self.playlist.len() {
            return None;
        }

        Some(Self {
            week_id: self.week_id.clone(),
            playlist: self.playlist.clone(),
            song_index: next_index,
            total_score: self.total_score + score,
            difficulty: self.difficulty.clone(),
        })
    }

    pub fn completed_total(&self, final_score: i32) -> i32 {
        self.total_score + final_score
    }
}
