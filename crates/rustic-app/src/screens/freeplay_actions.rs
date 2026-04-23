use rustic_core::paths::AssetPaths;

use super::super::freeplay_support::{highscore_targets, personal_best_text};
use super::FreeplayScreen;

impl FreeplayScreen {
    pub(super) fn change_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            return;
        }
        self.stop_preview();
        let preferred_difficulty = self.current_difficulty().to_string();
        let len = self.filtered.len() as i32;
        self.cur_selected = ((self.cur_selected as i32 + delta).rem_euclid(len)) as usize;
        let song_idx = self.filtered[self.cur_selected];
        self.bg_color_target = self.songs[song_idx].color;
        self.funkin_ui
            .set_selected_song(&self.songs[song_idx].song_id);
        self.sync_difficulty_for_selection(Some(&preferred_difficulty));
        self.refresh_score_target();

        if let Some(audio) = &mut self.audio {
            let paths = AssetPaths::platform_default();
            if let Some(sfx) = paths.sound("scrollMenu") {
                audio.play_sound(&sfx, 0.4);
            }
        }
    }

    pub(super) fn change_difficulty(&mut self, delta: i32) {
        let len = self.current_difficulties().len() as i32;
        if len == 0 {
            return;
        }
        self.cur_difficulty = ((self.cur_difficulty as i32 + delta).rem_euclid(len)) as usize;
        self.refresh_score_target();
    }

    pub(super) fn rebuild_filter(&mut self) {
        let query = self.search.to_lowercase();
        self.filtered = (0..self.songs.len())
            .filter(|&i| query.is_empty() || self.songs[i].name.to_lowercase().contains(&query))
            .collect();
        if self.filtered.is_empty() {
            self.cur_selected = 0;
        } else {
            self.cur_selected = self.cur_selected.min(self.filtered.len() - 1);
            let song_idx = self.filtered[self.cur_selected];
            self.bg_color_target = self.songs[song_idx].color;
            self.funkin_ui
                .set_selected_song(&self.songs[song_idx].song_id);
            self.sync_difficulty_for_selection(None);
        }
        self.lerp_selected = self.cur_selected as f32;
        self.refresh_score_target();
    }

    pub(super) fn jump_to_letter(&mut self, letter: char) {
        let letter_lower = letter.to_lowercase().next().unwrap_or('a');
        for (i, &song_idx) in self.filtered.iter().enumerate() {
            if self.songs[song_idx]
                .name
                .to_lowercase()
                .starts_with(letter_lower)
            {
                let delta = i as i32 - self.cur_selected as i32;
                if delta != 0 {
                    self.change_selection(delta);
                }
                return;
            }
        }
    }

    pub(super) fn current_score_text(&self) -> String {
        personal_best_text(self.displayed_score, self.displayed_accuracy)
    }

    pub(super) fn current_difficulties(&self) -> &[String] {
        self.filtered
            .get(self.cur_selected)
            .and_then(|&song_idx| self.songs.get(song_idx))
            .map(|song| song.difficulties.as_slice())
            .unwrap_or(&[])
    }

    pub(super) fn current_difficulty(&self) -> &str {
        self.current_difficulties()
            .get(self.cur_difficulty)
            .map(String::as_str)
            .unwrap_or("normal")
    }

    pub(super) fn current_difficulty_count(&self) -> usize {
        self.current_difficulties().len().max(1)
    }

    pub(super) fn sync_difficulty_for_selection(&mut self, preferred: Option<&str>) {
        let difficulties = self.current_difficulties();
        if difficulties.is_empty() {
            self.cur_difficulty = 0;
            return;
        }

        if let Some(preferred) = preferred {
            if let Some(index) = difficulties.iter().position(|diff| diff == preferred) {
                self.cur_difficulty = index;
                return;
            }
        }

        if let Some(index) = difficulties.iter().position(|diff| diff == "normal") {
            self.cur_difficulty = index;
        } else {
            self.cur_difficulty = 0;
        }
    }

    pub(super) fn refresh_score_target(&mut self) {
        let difficulty = self.current_difficulty().to_string();
        (self.target_score, self.target_accuracy) = highscore_targets(
            &self.highscores,
            &self.filtered,
            self.cur_selected,
            &self.songs,
            &difficulty,
        );
    }

    pub(super) fn stop_preview(&mut self) {
        if self.previewing_song.take().is_none() {
            return;
        }
        if let Some(audio) = &mut self.audio {
            audio.stop_loop_music();
        }
        self.start_funkin_freeplay_music(&AssetPaths::platform_default());
    }

    pub(super) fn toggle_preview(&mut self) {
        let Some(&song_idx) = self.filtered.get(self.cur_selected) else {
            return;
        };
        let song_id = self.songs[song_idx].song_id.clone();
        if self.previewing_song.as_deref() == Some(song_id.as_str()) {
            self.stop_preview();
            return;
        }
        if let Some(audio) = &mut self.audio {
            if let Some(inst) = AssetPaths::platform_default().song_audio(&song_id, "Inst.ogg") {
                audio.stop_loop_music();
                audio.play_loop_music_vol(&inst, 0.8);
                self.previewing_song = Some(song_id);
            }
        }
    }
}
