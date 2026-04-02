/// Events emitted by the gameplay layer for the render/audio layer to consume.
#[derive(Debug, Clone)]
pub enum GameEvent {
    /// Player hit a note. Render should show rating popup, confirm strum, splash, etc.
    NoteHit {
        lane: usize,
        rating: String,
        combo: i32,
        score: i32,
        note_type: String,
        is_sustain: bool,
        members_index: usize,
        /// If true, this note type damages the player on hit (like Hurt Note).
        hit_causes_miss: bool,
    },
    /// Player missed a note (too late or ghost tap).
    NoteMiss {
        lane: usize,
        note_type: String,
        members_index: usize,
        /// If true, this miss should not penalize the player (safe to ignore).
        ignored: bool,
    },
    /// Opponent auto-hit a note.
    OpponentNoteHit {
        lane: usize,
        note_type: String,
        is_sustain: bool,
        members_index: usize,
        /// If true, this note type causes damage on hit.
        hit_causes_miss: bool,
    },
    /// Strum confirm should be shown (player or opponent).
    StrumConfirm {
        lane: usize,
        player: bool,
    },
    /// Step hit (for Lua onStepHit).
    StepHit {
        step: i32,
    },
    /// Beat hit (for idle dance, icon bop).
    BeatHit {
        beat: i32,
    },
    /// Section changed (for camera targeting, camera bop).
    SectionChange {
        index: usize,
        must_hit: bool,
    },
    /// Countdown beat during pre-song.
    CountdownBeat {
        swag: i32,
    },
    /// Song audio should start playing.
    SongStart,
    /// Song finished.
    SongEnd,
    /// Player health reached zero.
    Death,
    /// Player vocals should be muted (on miss).
    MuteVocals,
    /// Player vocals should be unmuted (on hit).
    UnmuteVocals,
    /// Play a miss sound effect.
    PlayMissSound,
}
