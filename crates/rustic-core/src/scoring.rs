/// Score tracking state matching Psych Engine's scoring system.
#[derive(Debug, Clone)]
pub struct ScoreState {
    pub score: i32,
    pub combo: i32,
    pub max_combo: i32,
    pub misses: i32,
    /// Health in Psych Engine 0.0-2.0 range.
    pub health: f32,

    pub sicks: i32,
    pub goods: i32,
    pub bads: i32,
    pub shits: i32,

    /// Sum of all rating_mods for accuracy calculation.
    pub total_notes_hit: f64,
    /// Total number of notes that have been judged.
    pub total_notes_played: i32,
}

impl ScoreState {
    pub fn new() -> Self {
        Self {
            score: 0,
            combo: 0,
            max_combo: 0,
            misses: 0,
            health: 1.0,
            sicks: 0,
            goods: 0,
            bads: 0,
            shits: 0,
            total_notes_hit: 0.0,
            total_notes_played: 0,
        }
    }

    pub fn note_hit(&mut self, score: i32, rating_mod: f64, health_gain: f32, rating_name: &str) {
        self.score += score;
        self.combo += 1;
        if self.combo > self.max_combo {
            self.max_combo = self.combo;
        }
        self.health = (self.health + health_gain).min(2.0);

        self.total_notes_hit += rating_mod.max(0.0);
        self.total_notes_played += 1;

        match rating_name {
            "sick" => self.sicks += 1,
            "good" => self.goods += 1,
            "bad" => self.bads += 1,
            "shit" => self.shits += 1,
            _ => {}
        }
    }

    pub fn note_miss(&mut self, health_loss: f32) {
        self.misses += 1;
        self.combo = 0;
        self.health = (self.health - health_loss).max(0.0);
        self.total_notes_played += 1;
    }

    pub fn change_health(&mut self, delta: f32) {
        self.health = (self.health + delta).clamp(0.0, 2.0);
    }

    /// Psych Engine accuracy formula.
    pub fn accuracy(&self) -> f64 {
        if self.total_notes_played == 0 {
            return 0.0;
        }
        (self.total_notes_hit / self.total_notes_played as f64) * 100.0
    }

    pub fn grade(&self) -> &'static str {
        let acc = self.accuracy();
        if acc >= 100.0 {
            "S+"
        } else if acc >= 95.0 {
            "S"
        } else if acc >= 90.0 {
            "A"
        } else if acc >= 80.0 {
            "B"
        } else if acc >= 70.0 {
            "C"
        } else if acc >= 60.0 {
            "D"
        } else {
            "F"
        }
    }

    pub fn health_percent(&self) -> f32 {
        self.health / 2.0
    }
}

impl Default for ScoreState {
    fn default() -> Self {
        Self::new()
    }
}

pub const HEALTH_MISS: f32 = 0.0475;
pub const HEALTH_HOLD_TICK: f32 = 0.01;
pub const HEALTH_HOLD_DROP: f32 = 0.08;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_score_state() {
        let s = ScoreState::new();
        assert_eq!(s.health, 1.0);
        assert_eq!(s.score, 0);
        assert_eq!(s.combo, 0);
    }

    #[test]
    fn test_note_hit() {
        let mut s = ScoreState::new();
        s.note_hit(350, 1.0, 0.023, "sick");
        assert_eq!(s.score, 350);
        assert_eq!(s.combo, 1);
        assert_eq!(s.sicks, 1);
        assert!(s.health > 1.0);
    }

    #[test]
    fn test_note_miss() {
        let mut s = ScoreState::new();
        s.note_miss(HEALTH_MISS);
        assert_eq!(s.misses, 1);
        assert_eq!(s.combo, 0);
        assert!(s.health < 1.0);
    }

    #[test]
    fn test_accuracy() {
        let mut s = ScoreState::new();
        s.note_hit(350, 1.0, 0.0, "sick");
        s.note_hit(200, 0.67, 0.0, "good");
        assert!((s.accuracy() - 83.5).abs() < 0.1);
    }

    #[test]
    fn test_health_clamp() {
        let mut s = ScoreState::new();
        s.health = 1.95;
        s.note_hit(350, 1.0, 0.1, "sick");
        assert_eq!(s.health, 2.0);

        s.health = 0.01;
        s.note_miss(0.1);
        assert_eq!(s.health, 0.0);
    }
}
