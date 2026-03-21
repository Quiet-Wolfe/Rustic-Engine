use serde::{Deserialize, Serialize};

/// A BPM change event in the song timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BpmChangeEvent {
    pub step_time: f64,
    pub song_time: f64,
    pub bpm: f64,
    pub step_crochet: f64,
}

/// Conductor tracks BPM, beat/step timing, and song position.
/// Mirrors Psych Engine's `backend.Conductor`.
#[derive(Debug, Clone)]
pub struct Conductor {
    pub bpm: f64,
    /// Beat length in ms = (60 / bpm) * 1000
    pub crochet: f64,
    /// Step length in ms = crochet / 4
    pub step_crochet: f64,
    /// Current song position in milliseconds.
    pub song_position: f64,
    /// Map of BPM changes throughout the song.
    pub bpm_change_map: Vec<BpmChangeEvent>,
    /// Audio offset in milliseconds.
    pub offset: f64,
}

impl Conductor {
    pub fn new(bpm: f64) -> Self {
        let crochet = (60.0 / bpm) * 1000.0;
        Self {
            bpm,
            crochet,
            step_crochet: crochet / 4.0,
            song_position: 0.0,
            bpm_change_map: Vec::new(),
            offset: 0.0,
        }
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm;
        self.crochet = (60.0 / bpm) * 1000.0;
        self.step_crochet = self.crochet / 4.0;
    }

    /// Build the BPM change map from chart sections.
    /// `sections` yields (change_bpm, bpm, section_beats) per section.
    pub fn map_bpm_changes<I>(&mut self, initial_bpm: f64, sections: I)
    where
        I: IntoIterator<Item = (bool, f64, f64)>,
    {
        self.bpm_change_map.clear();

        let mut cur_bpm = initial_bpm;
        let mut total_steps: f64 = 0.0;
        let mut total_pos: f64 = 0.0;
        let mut cur_step_crochet = ((60.0 / cur_bpm) * 1000.0) / 4.0;

        for (change_bpm, section_bpm, section_beats) in sections {
            let steps_in_section = section_beats * 4.0;

            if change_bpm && (section_bpm - cur_bpm).abs() > 0.001 {
                cur_bpm = section_bpm;
                cur_step_crochet = ((60.0 / cur_bpm) * 1000.0) / 4.0;
                self.bpm_change_map.push(BpmChangeEvent {
                    step_time: total_steps,
                    song_time: total_pos,
                    bpm: cur_bpm,
                    step_crochet: cur_step_crochet,
                });
            }

            total_steps += steps_in_section;
            total_pos += cur_step_crochet * steps_in_section;
        }
    }

    /// Get the step number (continuous) from a song time in milliseconds.
    pub fn get_step(&self, time: f64) -> f64 {
        let mut last_change_step: f64 = 0.0;
        let mut last_change_time: f64 = 0.0;
        let mut last_step_crochet = self.step_crochet;

        for change in &self.bpm_change_map {
            if time >= change.song_time {
                last_change_step = change.step_time;
                last_change_time = change.song_time;
                last_step_crochet = change.step_crochet;
            } else {
                break;
            }
        }

        last_change_step + (time - last_change_time) / last_step_crochet
    }

    pub fn get_beat(&self, time: f64) -> f64 {
        self.get_step(time) / 4.0
    }

    pub fn get_bpm_at(&self, time: f64) -> f64 {
        let mut bpm = self.bpm;
        for change in &self.bpm_change_map {
            if time >= change.song_time {
                bpm = change.bpm;
            } else {
                break;
            }
        }
        bpm
    }

    pub fn get_step_crochet_at(&self, time: f64) -> f64 {
        let mut sc = self.step_crochet;
        for change in &self.bpm_change_map {
            if time >= change.song_time {
                sc = change.step_crochet;
            } else {
                break;
            }
        }
        sc
    }

    pub fn cur_step(&self) -> i32 {
        self.get_step(self.song_position) as i32
    }

    pub fn cur_beat(&self) -> i32 {
        self.get_beat(self.song_position) as i32
    }

    pub fn cur_section(&self, steps_per_section: i32) -> i32 {
        self.cur_step() / steps_per_section
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_conductor() {
        let c = Conductor::new(120.0);
        assert!((c.crochet - 500.0).abs() < 0.001);
        assert!((c.step_crochet - 125.0).abs() < 0.001);
    }

    #[test]
    fn test_step_no_changes() {
        let c = Conductor::new(120.0);
        let step = c.get_step(500.0);
        assert!((step - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_beat_calculation() {
        let c = Conductor::new(120.0);
        let beat = c.get_beat(1000.0);
        assert!((beat - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_bpm_change_map() {
        let mut c = Conductor::new(120.0);
        let sections = vec![
            (false, 120.0, 4.0),
            (true, 180.0, 4.0),
        ];
        c.map_bpm_changes(120.0, sections);

        assert_eq!(c.bpm_change_map.len(), 1);
        assert!((c.bpm_change_map[0].step_time - 16.0).abs() < 0.001);
        assert!((c.bpm_change_map[0].bpm - 180.0).abs() < 0.001);
    }

    #[test]
    fn test_step_with_bpm_change() {
        let mut c = Conductor::new(120.0);
        let sections = vec![
            (false, 120.0, 4.0),
            (true, 180.0, 4.0),
        ];
        c.map_bpm_changes(120.0, sections);

        let change_time = c.bpm_change_map[0].song_time;
        assert!((change_time - 2000.0).abs() < 0.001);

        let step = c.get_step(change_time + 500.0);
        assert!((step - 22.0).abs() < 0.1);
    }
}
