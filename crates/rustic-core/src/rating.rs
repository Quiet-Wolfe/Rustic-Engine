/// A configurable rating tier (Sick, Good, Bad, Shit).
/// Matches Psych Engine's `backend.Rating` class.
#[derive(Debug, Clone)]
pub struct Rating {
    pub name: String,
    pub image: String,
    /// Hit window in milliseconds.
    pub hit_window: f64,
    /// Score multiplier for accuracy (0.0 to 1.0).
    pub rating_mod: f64,
    /// Points awarded for this rating.
    pub score: i32,
    /// Whether to show a note splash effect.
    pub note_splash: bool,
    /// Health gained when hitting a note with this rating.
    pub health_gain: f32,
    pub hits: i32,
}

impl Rating {
    /// Create the default set of ratings matching Psych Engine defaults.
    pub fn load_default() -> Vec<Rating> {
        vec![
            Rating {
                name: "sick".into(),
                image: "sick".into(),
                hit_window: 45.0,
                rating_mod: 1.0,
                score: 350,
                note_splash: true,
                health_gain: 0.023,
                hits: 0,
            },
            Rating {
                name: "good".into(),
                image: "good".into(),
                hit_window: 90.0,
                rating_mod: 0.67,
                score: 200,
                note_splash: false,
                health_gain: 0.015,
                hits: 0,
            },
            Rating {
                name: "bad".into(),
                image: "bad".into(),
                hit_window: 135.0,
                rating_mod: 0.34,
                score: 100,
                note_splash: false,
                health_gain: 0.005,
                hits: 0,
            },
            Rating {
                name: "shit".into(),
                image: "shit".into(),
                hit_window: 166.0,
                rating_mod: 0.0,
                score: 50,
                note_splash: false,
                health_gain: 0.0,
                hits: 0,
            },
        ]
    }
}

/// The result of judging a note hit.
#[derive(Debug, Clone)]
pub struct Judgment {
    pub rating_index: usize,
    pub name: String,
    pub score: i32,
    pub rating_mod: f64,
    pub note_splash: bool,
    pub health_gain: f32,
}

/// FC classification states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FcClassification {
    Sfc,
    Gfc,
    Fc,
    Sdcb,
    Clear,
}

/// Judge a note based on timing difference (absolute value).
/// Ratings must be sorted by hit_window ascending.
/// Returns None if outside all windows (miss).
pub fn judge_note(ratings: &[Rating], time_diff_abs: f64) -> Option<Judgment> {
    for (i, rating) in ratings.iter().enumerate() {
        if time_diff_abs <= rating.hit_window {
            return Some(Judgment {
                rating_index: i,
                name: rating.name.clone(),
                score: rating.score,
                rating_mod: rating.rating_mod,
                note_splash: rating.note_splash,
                health_gain: rating.health_gain,
            });
        }
    }
    None
}

#[allow(unused_variables)]
pub fn classify_fc(sicks: i32, goods: i32, bads: i32, shits: i32, misses: i32) -> FcClassification {
    if misses == 0 {
        if bads == 0 && shits == 0 {
            if goods == 0 {
                FcClassification::Sfc
            } else {
                FcClassification::Gfc
            }
        } else {
            FcClassification::Fc
        }
    } else if misses < 10 {
        FcClassification::Sdcb
    } else {
        FcClassification::Clear
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_judge_sick() {
        let ratings = Rating::load_default();
        let j = judge_note(&ratings, 20.0).unwrap();
        assert_eq!(j.name, "sick");
        assert_eq!(j.score, 350);
    }

    #[test]
    fn test_judge_good() {
        let ratings = Rating::load_default();
        let j = judge_note(&ratings, 60.0).unwrap();
        assert_eq!(j.name, "good");
    }

    #[test]
    fn test_judge_bad() {
        let ratings = Rating::load_default();
        let j = judge_note(&ratings, 100.0).unwrap();
        assert_eq!(j.name, "bad");
    }

    #[test]
    fn test_judge_shit() {
        let ratings = Rating::load_default();
        let j = judge_note(&ratings, 150.0).unwrap();
        assert_eq!(j.name, "shit");
    }

    #[test]
    fn test_judge_miss() {
        let ratings = Rating::load_default();
        assert!(judge_note(&ratings, 200.0).is_none());
    }

    #[test]
    fn test_judge_boundary() {
        let ratings = Rating::load_default();
        assert_eq!(judge_note(&ratings, 45.0).unwrap().name, "sick");
        assert_eq!(judge_note(&ratings, 90.0).unwrap().name, "good");
    }

    #[test]
    fn test_fc_classification() {
        assert_eq!(classify_fc(100, 0, 0, 0, 0), FcClassification::Sfc);
        assert_eq!(classify_fc(90, 10, 0, 0, 0), FcClassification::Gfc);
        assert_eq!(classify_fc(80, 10, 5, 3, 0), FcClassification::Fc);
        assert_eq!(classify_fc(80, 10, 5, 3, 5), FcClassification::Sdcb);
        assert_eq!(classify_fc(50, 10, 5, 3, 20), FcClassification::Clear);
    }
}
