use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

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
    /// One-shot video/cutscene audio handle.
    cutscene_audio: Option<StaticSoundHandle>,
    /// Lua/Psych tagged sounds, addressed by tag for stop/pause/seek/fade APIs.
    tagged_sounds: HashMap<String, StaticSoundHandle>,
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
            cutscene_audio: None,
            tagged_sounds: HashMap::new(),
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
            h.set_volume(0.65, Tween::default());
        }
    }

    /// Set the volume of all vocal tracks (0.0 to 1.0).
    /// Psych Engine uses 0.65 as the default vocals volume.
    pub fn set_vocals_volume(&mut self, volume: f64) {
        if let Some(h) = &mut self.vocals_player {
            h.set_volume(volume, Tween::default());
        }
        if let Some(h) = &mut self.vocals_opponent {
            h.set_volume(volume, Tween::default());
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

    /// Play a Lua/Psych sound with an optional tag. Tagged sounds keep their
    /// handle so scripts can control them later.
    pub fn play_tagged_sound(&mut self, tag: &str, path: &Path, volume: f64, looping: bool) {
        if !path.exists() || tag.is_empty() {
            return;
        }
        self.stop_tagged_sound(tag);
        if let Ok(data) = StaticSoundData::from_file(path) {
            let data = if looping { data.loop_region(..) } else { data };
            if let Ok(mut handle) = self.manager.play(data) {
                handle.set_volume(volume, Tween::default());
                self.tagged_sounds.insert(tag.to_string(), handle);
            }
        }
    }

    pub fn stop_tagged_sound(&mut self, tag: &str) {
        if let Some(mut handle) = self.tagged_sounds.remove(tag) {
            handle.stop(Tween::default());
        }
    }

    pub fn stop_all_tagged_sounds(&mut self) {
        for (_, mut handle) in self.tagged_sounds.drain() {
            handle.stop(Tween::default());
        }
    }

    pub fn pause_tagged_sound(&mut self, tag: &str) {
        if let Some(handle) = self.tagged_sounds.get_mut(tag) {
            handle.pause(Tween::default());
        }
    }

    pub fn pause_all_tagged_sounds(&mut self) {
        for handle in self.tagged_sounds.values_mut() {
            handle.pause(Tween::default());
        }
    }

    pub fn resume_tagged_sound(&mut self, tag: &str) {
        if let Some(handle) = self.tagged_sounds.get_mut(tag) {
            handle.resume(Tween::default());
        }
    }

    pub fn resume_all_tagged_sounds(&mut self) {
        for handle in self.tagged_sounds.values_mut() {
            handle.resume(Tween::default());
        }
    }

    pub fn set_tagged_sound_volume(&mut self, tag: &str, volume: f64) {
        if let Some(handle) = self.tagged_sounds.get_mut(tag) {
            handle.set_volume(volume, Tween::default());
        }
    }

    pub fn fade_tagged_sound(&mut self, tag: &str, from: Option<f64>, to: f64, duration_secs: f64) {
        if let Some(handle) = self.tagged_sounds.get_mut(tag) {
            if let Some(from) = from {
                handle.set_volume(from, Tween::default());
            }
            handle.set_volume(to, tween_secs(duration_secs));
        }
    }

    pub fn fade_out_tagged_sound(&mut self, tag: &str, duration_secs: f64) {
        if let Some(handle) = self.tagged_sounds.get_mut(tag) {
            handle.stop(tween_secs(duration_secs));
        }
    }

    pub fn seek_tagged_sound(&mut self, tag: &str, position_ms: f64) {
        if let Some(handle) = self.tagged_sounds.get_mut(tag) {
            handle.seek_to(position_ms / 1000.0);
        }
    }

    pub fn tagged_sound_position_ms(&self, tag: &str) -> Option<f64> {
        self.tagged_sounds
            .get(tag)
            .map(|handle| handle.position() * 1000.0)
    }

    pub fn tagged_sound_exists(&self, tag: &str) -> bool {
        self.tagged_sounds
            .get(tag)
            .map(|handle| handle.state() != PlaybackState::Stopped)
            .unwrap_or(false)
    }

    pub fn set_tagged_sound_pitch(&mut self, tag: &str, pitch: f64) {
        if let Some(handle) = self.tagged_sounds.get_mut(tag) {
            handle.set_playback_rate(pitch, Tween::default());
        }
    }

    /// Play a looping music track (e.g. gameOver.ogg). Stops any previous loop.
    pub fn play_loop_music(&mut self, path: &Path) {
        self.play_loop_music_vol(path, 1.0);
    }

    /// Play a looping music track and seek to a start position in milliseconds.
    pub fn play_loop_music_from(&mut self, path: &Path, volume: f64, start_ms: f64) {
        self.stop_loop_music();
        if !path.exists() {
            return;
        }
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

    pub fn fade_loop_music(&mut self, from: Option<f64>, to: f64, duration_secs: f64) {
        if let Some(h) = &mut self.loop_music {
            if let Some(from) = from {
                h.set_volume(from, Tween::default());
            }
            h.set_volume(to, tween_secs(duration_secs));
        }
    }

    pub fn fade_out_loop_music(&mut self, duration_secs: f64) {
        if let Some(h) = &mut self.loop_music {
            h.stop(tween_secs(duration_secs));
        }
    }

    pub fn pause_loop_music(&mut self) {
        if let Some(h) = &mut self.loop_music {
            h.pause(Tween::default());
        }
    }

    pub fn resume_loop_music(&mut self) {
        if let Some(h) = &mut self.loop_music {
            h.resume(Tween::default());
        }
    }

    pub fn loop_music_position_ms(&self) -> f64 {
        self.loop_music
            .as_ref()
            .map(|h| h.position() * 1000.0)
            .unwrap_or(0.0)
    }

    pub fn seek_loop_music(&mut self, position_ms: f64) {
        if let Some(h) = &mut self.loop_music {
            h.seek_to(position_ms / 1000.0);
        }
    }

    pub fn set_loop_music_pitch(&mut self, pitch: f64) {
        if let Some(h) = &mut self.loop_music {
            h.set_playback_rate(pitch, Tween::default());
        }
    }

    pub fn play_cutscene_audio(&mut self, path: &Path, volume: f64) {
        self.stop_cutscene_audio();
        if !path.exists() {
            return;
        }
        if let Ok(data) = StaticSoundData::from_file(path) {
            if let Ok(mut handle) = self.manager.play(data) {
                handle.set_volume(volume, Tween::default());
                self.cutscene_audio = Some(handle);
            }
        }
    }

    pub fn pause_cutscene_audio(&mut self) {
        if let Some(h) = &mut self.cutscene_audio {
            h.pause(Tween::default());
        }
    }

    pub fn resume_cutscene_audio(&mut self) {
        if let Some(h) = &mut self.cutscene_audio {
            h.resume(Tween::default());
        }
    }

    pub fn stop_cutscene_audio(&mut self) {
        if let Some(h) = &mut self.cutscene_audio {
            h.stop(Tween::default());
        }
        self.cutscene_audio = None;
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

fn tween_secs(secs: f64) -> Tween {
    Tween {
        duration: Duration::from_secs_f64(secs.max(0.001)),
        ..Default::default()
    }
}
