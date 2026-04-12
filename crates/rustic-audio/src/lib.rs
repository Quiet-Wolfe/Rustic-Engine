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
    miss_sounds: Vec<StaticSoundData>,
    miss_index: usize,
    /// Looping music handle (e.g. gameOver.ogg).
    loop_music: Option<StaticSoundHandle>,
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
            miss_sounds: Vec::new(),
            miss_index: 0,
            loop_music: None,
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

    /// Load miss sound effects (missnote1.ogg, missnote2.ogg, missnote3.ogg).
    pub fn load_miss_sounds(&mut self, dir: &Path) {
        for i in 1..=3 {
            let path = dir.join(format!("missnote{i}.ogg"));
            if path.exists() {
                if let Ok(data) = StaticSoundData::from_file(&path) {
                    self.miss_sounds.push(data);
                }
            }
        }
    }

    /// Play a random miss sound effect.
    pub fn play_miss_sound(&mut self) {
        if self.miss_sounds.is_empty() {
            return;
        }
        let data = self.miss_sounds[self.miss_index].clone();
        self.miss_index = (self.miss_index + 1) % self.miss_sounds.len();
        let _ = self.manager.play(data);
    }

    /// Play a one-shot sound effect from a file path.
    pub fn play_sound(&mut self, path: &Path, volume: f64) {
        if !path.exists() {
            return;
        }
        if let Ok(data) = StaticSoundData::from_file(path) {
            if let Ok(mut handle) = self.manager.play(data) {
                handle.set_volume(volume, Tween::default());
            }
        }
    }

    /// Play a looping music track (e.g. gameOver.ogg). Stops any previous loop.
    pub fn play_loop_music(&mut self, path: &Path) {
        self.play_loop_music_vol(path, 1.0);
    }

    /// Play a looping music track and seek to a start position in milliseconds.
    pub fn play_loop_music_from(&mut self, path: &Path, volume: f64, start_ms: f64) {
        self.stop_loop_music();
        if !path.exists() { return; }
        if let Ok(data) = StaticSoundData::from_file(path) {
            let data = data.loop_region(..);
            if let Ok(mut handle) = self.manager.play(data) {
                handle.set_volume(volume, Tween::default());
                if start_ms > 0.0 {
                    handle.seek_to(start_ms / 1000.0);
                }
                self.loop_music = Some(handle);
            }
        }
    }

    /// Play a looping music track at a given volume.
    pub fn play_loop_music_vol(&mut self, path: &Path, volume: f64) {
        self.play_loop_music_from(path, volume, 0.0);
    }

    pub fn set_loop_music_volume(&mut self, volume: f64) {
        if let Some(h) = &mut self.loop_music {
            h.set_volume(volume, Tween::default());
        }
    }

    /// Stop the looping music track.
    pub fn stop_loop_music(&mut self) {
        if let Some(h) = &mut self.loop_music {
            h.stop(Tween::default());
        }
        self.loop_music = None;
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

    pub fn sound_duration_ms(path: &Path) -> Option<f64> {
        StaticSoundData::from_file(path)
            .ok()
            .map(|data| data.duration().as_secs_f64() * 1000.0)
    }
}
