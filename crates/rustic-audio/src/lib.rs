use std::path::Path;

use kira::manager::{AudioManager, AudioManagerSettings, DefaultBackend};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::sound::PlaybackState;
use kira::tween::Tween;

/// Audio engine wrapping kira. Handles inst + vocals playback with conductor sync.
pub struct AudioEngine {
    manager: AudioManager,
    inst: Option<StaticSoundHandle>,
    vocals_player: Option<StaticSoundHandle>,
    vocals_opponent: Option<StaticSoundHandle>,
    playing: bool,
}

impl AudioEngine {
    pub fn new() -> Self {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .expect("Failed to initialize audio");
        Self {
            manager,
            inst: None,
            vocals_player: None,
            vocals_opponent: None,
            playing: false,
        }
    }

    /// Load the instrumental track.
    pub fn load_inst(&mut self, path: &Path) {
        let data = StaticSoundData::from_file(path)
            .unwrap_or_else(|e| panic!("Failed to load inst {:?}: {}", path, e));
        let handle = self.manager.play(data).expect("Failed to play inst");
        self.inst = Some(handle);
        // Pause immediately — we'll start on cue
        if let Some(h) = &mut self.inst {
            h.pause(Tween::default());
        }
    }

    /// Load player vocals track (optional).
    pub fn load_vocals(&mut self, path: &Path) {
        if !path.exists() {
            return;
        }
        let data = StaticSoundData::from_file(path)
            .unwrap_or_else(|e| panic!("Failed to load vocals {:?}: {}", path, e));
        let handle = self.manager.play(data).expect("Failed to play vocals");
        self.vocals_player = Some(handle);
        if let Some(h) = &mut self.vocals_player {
            h.pause(Tween::default());
        }
    }

    /// Load opponent vocals track (optional).
    pub fn load_opponent_vocals(&mut self, path: &Path) {
        if !path.exists() {
            return;
        }
        let data = StaticSoundData::from_file(path)
            .unwrap_or_else(|e| panic!("Failed to load opp vocals {:?}: {}", path, e));
        let handle = self.manager.play(data).expect("Failed to play opp vocals");
        self.vocals_opponent = Some(handle);
        if let Some(h) = &mut self.vocals_opponent {
            h.pause(Tween::default());
        }
    }

    /// Start/resume all tracks.
    pub fn play(&mut self) {
        let t = Tween::default();
        if let Some(h) = &mut self.inst {
            h.resume(t);
        }
        if let Some(h) = &mut self.vocals_player {
            h.resume(t);
        }
        if let Some(h) = &mut self.vocals_opponent {
            h.resume(t);
        }
        self.playing = true;
    }

    /// Pause all tracks.
    pub fn pause(&mut self) {
        let t = Tween::default();
        if let Some(h) = &mut self.inst {
            h.pause(t);
        }
        if let Some(h) = &mut self.vocals_player {
            h.pause(t);
        }
        if let Some(h) = &mut self.vocals_opponent {
            h.pause(t);
        }
        self.playing = false;
    }

    /// Get the instrumental's current playback position in milliseconds.
    /// This is the authoritative song position for conductor sync.
    pub fn position_ms(&self) -> f64 {
        self.inst
            .as_ref()
            .map(|h| h.position() * 1000.0)
            .unwrap_or(0.0)
    }

    /// Whether audio has finished playing.
    pub fn is_finished(&self) -> bool {
        self.inst
            .as_ref()
            .map(|h| h.state() == PlaybackState::Stopped)
            .unwrap_or(true)
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Mute player vocals (on miss).
    pub fn mute_player_vocals(&mut self) {
        if let Some(h) = &mut self.vocals_player {
            h.set_volume(0.0, Tween::default());
        }
    }

    /// Unmute player vocals (on hit).
    pub fn unmute_player_vocals(&mut self) {
        if let Some(h) = &mut self.vocals_player {
            h.set_volume(1.0, Tween::default());
        }
    }

    /// Seek all tracks to a position in milliseconds.
    pub fn seek(&mut self, position_ms: f64) {
        let secs = position_ms / 1000.0;
        if let Some(h) = &mut self.inst {
            h.seek_to(secs);
        }
        if let Some(h) = &mut self.vocals_player {
            h.seek_to(secs);
        }
        if let Some(h) = &mut self.vocals_opponent {
            h.seek_to(secs);
        }
    }
}
