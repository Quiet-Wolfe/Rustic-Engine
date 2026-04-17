//! What the agent sees each tick. Intentionally plain data — no ndarray
//! dependency yet. A feature-flagged adapter can flatten this into a tensor
//! when we wire up actual inference.

use serde::{Deserialize, Serialize};

/// Upcoming notes an agent considers, per lane. Each entry is (time-until-hit
/// in ms, sustain-length in ms). Lanes are always ordered
/// [left, down, up, right] — matching Psych Engine's input layout.
pub const LOOKAHEAD_NOTES: usize = 4;

/// Sentinel for "no upcoming note in this slot". Chosen as a large finite
/// value (roughly a full minute of song) instead of `f32::INFINITY` because
/// JSON has no standard infinity encoding — serde silently maps `Infinity`
/// to `null`, which then fails to deserialize back into an `f32`. A big
/// finite number round-trips cleanly and still reads as "very far in the
/// future" to any policy that clamps its inputs.
pub const NO_NOTE_TIME: f32 = 60_000.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Song position in ms. Lets policies reason about section/beat phase.
    pub song_pos_ms: f64,
    /// Current BPM. Same units as the conductor uses.
    pub bpm: f64,
    /// Player health, 0.0..2.0 (Psych Engine scale, 1.0 = neutral).
    pub health: f32,
    /// Which keys the engine currently sees as pressed.
    pub keys_held: [bool; 4],
    /// Next `LOOKAHEAD_NOTES` notes per lane. (time_until_hit_ms, sustain_ms).
    /// Shorter slots are padded with `(NO_NOTE_TIME, 0.0)`.
    pub upcoming: [[(f32, f32); LOOKAHEAD_NOTES]; 4],
}

impl Observation {
    pub fn zero() -> Self {
        Self {
            song_pos_ms: 0.0,
            bpm: 0.0,
            health: 1.0,
            keys_held: [false; 4],
            upcoming: [[(NO_NOTE_TIME, 0.0); LOOKAHEAD_NOTES]; 4],
        }
    }
}

/// One action emitted per tick. For v0 the agent only chooses which lanes to
/// press/release — the game loop translates these to the same input events
/// the keyboard handler would.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Action {
    /// Desired held-state per lane for the next tick.
    pub press: [bool; 4],
}
