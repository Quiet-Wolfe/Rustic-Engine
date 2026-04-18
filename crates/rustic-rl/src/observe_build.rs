//! Helpers for constructing an `Observation` from whatever the game already
//! carries. Kept out of `observe.rs` so the core types stay dependency-free.
//!
//! The game side supplies notes via an iterator of `UpcomingNote` — this lets
//! us stay decoupled from `rustic_core::note::NoteData` (which isn't a
//! dependency of this crate).

use crate::observe::{Observation, LOOKAHEAD_NOTES, NO_NOTE_TIME};

/// A single upcoming note, in the coordinate system the observation uses:
/// lane index 0..4 (playable lanes only) and strum time relative to the
/// current song position (in ms, can be negative if already passed).
#[derive(Debug, Clone, Copy)]
pub struct UpcomingNote {
    pub lane: usize,
    pub time_until_hit_ms: f32,
    pub sustain_ms: f32,
}

/// Build a full `Observation`. `notes` must be already filtered to
/// player-playable, not-yet-hit notes; this helper does not re-filter.
pub fn build_observation<I: Iterator<Item = UpcomingNote>>(
    song_pos_ms: f64,
    bpm: f64,
    health: f32,
    keys_held: [bool; 4],
    notes: I,
) -> Observation {
    let mut upcoming = [[(NO_NOTE_TIME, 0.0); LOOKAHEAD_NOTES]; 4];
    let mut filled = [0usize; 4];

    for note in notes {
        let lane = note.lane;
        if lane >= 4 {
            continue;
        }
        let slot = filled[lane];
        if slot >= LOOKAHEAD_NOTES {
            continue;
        }
        upcoming[lane][slot] = (note.time_until_hit_ms, note.sustain_ms);
        filled[lane] = slot + 1;
    }

    // Sort each lane's slots by time_until_hit ascending so the policy sees
    // the nearest note first regardless of input order.
    for lane in 0..4 {
        let end = filled[lane];
        upcoming[lane][..end]
            .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }

    Observation {
        song_pos_ms,
        bpm,
        health,
        keys_held,
        upcoming,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_slots_in_order_and_sorts_by_time() {
        let notes = vec![
            UpcomingNote {
                lane: 0,
                time_until_hit_ms: 500.0,
                sustain_ms: 0.0,
            },
            UpcomingNote {
                lane: 0,
                time_until_hit_ms: 100.0,
                sustain_ms: 0.0,
            },
            UpcomingNote {
                lane: 2,
                time_until_hit_ms: 300.0,
                sustain_ms: 250.0,
            },
        ];
        let obs = build_observation(0.0, 120.0, 1.0, [false; 4], notes.into_iter());

        assert_eq!(obs.upcoming[0][0].0, 100.0);
        assert_eq!(obs.upcoming[0][1].0, 500.0);
        assert_eq!(obs.upcoming[0][2].0, NO_NOTE_TIME);
        assert_eq!(obs.upcoming[2][0], (300.0, 250.0));
        assert_eq!(obs.upcoming[1][0].0, NO_NOTE_TIME);
    }

    #[test]
    fn respects_lookahead_cap() {
        let many = (0..10).map(|i| UpcomingNote {
            lane: 1,
            time_until_hit_ms: i as f32 * 100.0,
            sustain_ms: 0.0,
        });
        let obs = build_observation(0.0, 120.0, 1.0, [false; 4], many);
        for slot in 0..LOOKAHEAD_NOTES {
            assert!(
                obs.upcoming[1][slot].0 < NO_NOTE_TIME,
                "slot {slot} should be filled"
            );
        }
    }
}
