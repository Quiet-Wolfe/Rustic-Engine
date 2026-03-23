use std::path::Path;

use rustic_gameplay::events::GameEvent;

use super::{
    PlayScreen, RatingPopup, DeathState, DeathPhase, NoteSplash,
    RATING_ACCEL, RATING_VEL_Y, RATING_FADE_SECS,
    SPLASH_FPS, SPLASH_FRAMES,
};

impl PlayScreen {
    pub(super) fn update_inner(&mut self, dt: f32) {
        if self.paused { return; }

        // Death state machine (visual only)
        if let Some(death) = &mut self.death {
            let dt_ms = dt as f64 * 1000.0;
            death.timer += dt_ms;
            death.character.update(dt);

            match death.phase {
                DeathPhase::FirstDeath => {
                    if death.character.anim.finished {
                        death.phase = DeathPhase::Loop;
                        death.character.play_anim("deathLoop", false);
                        if let Some(audio) = &mut self.audio {
                            let music = Path::new("references/FNF-PsychEngine/assets/shared/music/gameOver.ogg");
                            audio.play_loop_music(music);
                        }
                    }
                }
                DeathPhase::Loop => {}
                DeathPhase::Confirm => {
                    death.fade_alpha += dt / 2.0;
                    if death.fade_alpha >= 1.0 {
                        self.wants_restart = true;
                    }
                }
            }

            // Camera slowly follows death character
            let (mx, my) = death.character.midpoint();
            let lerp = 1.0 - (-dt * 0.6).exp();
            self.camera.x += (mx - self.camera.x) * lerp;
            self.camera.y += (my - self.camera.y) * lerp;
            return;
        }

        let dt_ms = dt as f64 * 1000.0;

        // Visual: rating popup physics
        for popup in &mut self.rating_popups {
            popup.age_ms += dt_ms;
            popup.vel_y += RATING_ACCEL * dt;
            popup.y += popup.vel_y * dt;
            if popup.age_ms > popup.fade_delay {
                popup.alpha -= dt / RATING_FADE_SECS;
            }
        }
        self.rating_popups.retain(|p| p.alpha > 0.0);

        // Visual: countdown sprite fade
        if self.countdown_alpha > 0.0 {
            self.countdown_alpha -= dt / (self.game.conductor.crochet as f32 / 1000.0);
        }

        // Lua: onUpdate (before gameplay logic)
        if self.scripts.has_scripts() {
            self.scripts.call_with_elapsed("onUpdate", dt as f64);
        }

        // === Call into gameplay logic ===
        let audio_pos = if self.game.song_started {
            self.audio.as_ref().map(|a| a.position_ms())
        } else {
            None
        };
        let audio_finished = self.audio.as_ref().is_some_and(|a| a.is_finished());
        self.game.update(dt, audio_pos, audio_finished);

        // === Process gameplay events ===
        let events = self.game.drain_events();
        for event in events {
            match event {
                GameEvent::NoteHit { lane, rating, combo, .. } => {
                    // Visual: spawn rating popup
                    self.rating_popups.push(RatingPopup {
                        rating_name: rating.clone(),
                        combo,
                        y: 0.0,
                        vel_y: RATING_VEL_Y,
                        age_ms: 0.0,
                        fade_delay: self.game.conductor.crochet,
                        alpha: 1.0,
                    });
                    // Visual: note splash on sick hits
                    if rating == "sick" {
                        self.splashes.push(NoteSplash {
                            lane, player: true, frame: 0, timer: 0.0,
                        });
                    }
                    // Character: BF sing
                    if let Some(bf) = &mut self.char_bf {
                        bf.play_sing(lane);
                    }
                    // Lua: goodNoteHit
                    if self.scripts.has_scripts() {
                        self.scripts.call("goodNoteHit");
                    }
                }
                GameEvent::NoteMiss { lane } => {
                    self.rating_popups.push(RatingPopup {
                        rating_name: "miss".into(), combo: 0,
                        y: 0.0, vel_y: RATING_VEL_Y,
                        age_ms: 0.0,
                        fade_delay: self.game.conductor.crochet,
                        alpha: 1.0,
                    });
                    if let Some(bf) = &mut self.char_bf {
                        bf.play_miss(lane);
                    }
                    // Lua: noteMiss
                    if self.scripts.has_scripts() {
                        self.scripts.call("noteMiss");
                    }
                }
                GameEvent::OpponentNoteHit { lane } => {
                    if !self.disable_zooming {
                        self.cam_zooming = true;
                    }
                    if let Some(dad) = &mut self.char_dad {
                        dad.play_sing(lane);
                    }
                    // Lua: opponentNoteHit
                    if self.scripts.has_scripts() {
                        self.scripts.call("opponentNoteHit");
                    }
                }
                GameEvent::CountdownBeat { swag } => {
                    let sfx_dir = Path::new("references/FNF-PsychEngine/assets/shared/sounds");
                    if let Some(audio) = &mut self.audio {
                        match swag {
                            0 => audio.play_sound(&sfx_dir.join("intro3.ogg"), 0.6),
                            1 => {
                                audio.play_sound(&sfx_dir.join("intro2.ogg"), 0.6);
                                self.countdown_swag = 1;
                                self.countdown_alpha = 1.0;
                            }
                            2 => {
                                audio.play_sound(&sfx_dir.join("intro1.ogg"), 0.6);
                                self.countdown_swag = 2;
                                self.countdown_alpha = 1.0;
                            }
                            3 => {
                                audio.play_sound(&sfx_dir.join("introGo.ogg"), 0.6);
                                self.countdown_swag = 3;
                                self.countdown_alpha = 1.0;
                            }
                            _ => {}
                        }
                    }
                }
                GameEvent::SongStart => {
                    if let Some(audio) = &mut self.audio {
                        audio.play();
                    }
                    if self.scripts.has_scripts() {
                        self.scripts.call("onSongStart");
                    }
                }
                GameEvent::SongEnd => {
                    if self.scripts.has_scripts() {
                        self.scripts.call("onEndSong");
                    }
                }
                GameEvent::StepHit { step } => {
                    if self.scripts.has_scripts() {
                        self.scripts.call_step("onStepHit", step);
                    }
                }
                GameEvent::BeatHit { beat } => {
                    // Character dance
                    if let Some(dad) = &mut self.char_dad {
                        let freq = if dad.has_dance_idle { 1 } else { 2 };
                        if beat % freq == 0 && !dad.anim.current_anim.starts_with("sing") {
                            dad.dance();
                        }
                    }
                    if let Some(bf) = &mut self.char_bf {
                        let freq = if bf.has_dance_idle { 1 } else { 2 };
                        if beat % freq == 0 && !bf.anim.current_anim.starts_with("sing") {
                            bf.dance();
                        }
                    }
                    if let Some(gf) = &mut self.char_gf {
                        let freq = if gf.has_dance_idle { 1 } else { 2 };
                        if beat % freq == 0 {
                            gf.dance();
                        }
                    }
                    // Icon bop
                    self.icon_scale_bf = 1.2;
                    self.icon_scale_dad = 1.2;
                    // Lua: onBeatHit
                    if self.scripts.has_scripts() {
                        self.scripts.call_beat("onBeatHit", beat);
                    }
                }
                GameEvent::SectionChange { must_hit, .. } => {
                    self.recompute_camera_targets();
                    let target = if must_hit { self.cam_bf } else { self.cam_dad };
                    self.camera.follow(target[0], target[1]);
                    if self.cam_zooming && !self.disable_zooming && self.camera.zoom < 1.35 {
                        self.camera.zoom += 0.015;
                        self.hud_zoom += 0.03;
                    }
                    // Lua: onSectionHit
                    if self.scripts.has_scripts() {
                        self.scripts.call("onSectionHit");
                    }
                }
                GameEvent::MuteVocals => {
                    if let Some(audio) = &mut self.audio {
                        audio.mute_player_vocals();
                    }
                }
                GameEvent::UnmuteVocals => {
                    if let Some(audio) = &mut self.audio {
                        audio.unmute_player_vocals();
                    }
                }
                GameEvent::PlayMissSound => {
                    if let Some(audio) = &mut self.audio {
                        audio.play_miss_sound();
                    }
                }
                GameEvent::Death => {
                    if let Some(audio) = &mut self.audio {
                        audio.pause();
                        let sfx = Path::new("references/FNF-PsychEngine/assets/shared/sounds/fnf_loss_sfx.ogg");
                        audio.play_sound(sfx, 1.0);
                    }
                    if let Some(mut death_char) = self.death_char_preloaded.take() {
                        if let Some(bf) = &self.char_bf {
                            death_char.x = bf.x;
                            death_char.y = bf.y;
                        }
                        death_char.play_anim("firstDeath", true);
                        self.death = Some(DeathState {
                            character: death_char,
                            phase: DeathPhase::FirstDeath,
                            timer: 0.0,
                            fade_alpha: 0.0,
                        });
                    }
                }
                _ => {}
            }
        }

        // Visual: character animations (sing→idle transitions)
        if let Some(dad) = &mut self.char_dad {
            if dad.anim.current_anim.starts_with("sing") {
                dad.hold_timer += dt_ms;
                let threshold = self.game.conductor.step_crochet * dad.sing_duration * 1.1;
                if dad.hold_timer >= threshold {
                    dad.dance();
                    dad.hold_timer = 0.0;
                }
            }
            dad.update(dt);
        }

        if let Some(bf) = &mut self.char_bf {
            if bf.anim.current_anim.starts_with("sing") {
                bf.hold_timer += dt_ms;
                let threshold = self.game.conductor.step_crochet * bf.sing_duration * 1.1;
                if bf.hold_timer >= threshold && !self.game.keys_held.iter().any(|&k| k) {
                    bf.dance();
                    bf.hold_timer = 0.0;
                }
            }
            bf.update(dt);
        }

        if let Some(gf) = &mut self.char_gf {
            gf.update(dt);
        }

        // Visual: icon scale decay
        let icon_lerp = (-dt * 9.0).exp();
        self.icon_scale_bf = 1.0 + (self.icon_scale_bf - 1.0) * icon_lerp;
        self.icon_scale_dad = 1.0 + (self.icon_scale_dad - 1.0) * icon_lerp;

        // Camera zoom decay
        let zoom_before = self.camera.zoom;
        self.camera.update(dt);
        if self.cam_zooming {
            let zoom_lerp = (-dt * 3.125).exp();
            self.camera.zoom = self.default_cam_zoom
                + (zoom_before - self.default_cam_zoom) * zoom_lerp;
        }

        // HUD zoom decay
        let hud_lerp = (-dt * 3.125).exp();
        self.hud_zoom = 1.0 + (self.hud_zoom - 1.0) * hud_lerp;

        // Lua: onUpdatePost (after all game logic)
        if self.scripts.has_scripts() {
            self.scripts.call_with_elapsed("onUpdatePost", dt as f64);
        }

        // Update tweens/timers and fire completion callbacks
        if self.scripts.has_scripts() {
            self.scripts.update_tweens(dt);
        }

        // Process game-level property writes from Lua scripts
        self.process_property_writes();

        // Visual: Lua sprite animation advancement
        for (tag, sprite) in self.scripts.state.lua_sprites.iter_mut() {
            if sprite.current_anim.is_empty() || sprite.anim_finished { continue; }
            if sprite.anim_fps <= 0.0 { continue; }
            let frame_count = self.lua_atlases.get(tag.as_str())
                .map(|a| a.frame_count(&sprite.current_anim))
                .unwrap_or(0);
            if frame_count == 0 { continue; }

            sprite.anim_timer += dt;
            let frame_dur = 1.0 / sprite.anim_fps;
            while sprite.anim_timer >= frame_dur {
                sprite.anim_timer -= frame_dur;
                sprite.anim_frame += 1;
                if sprite.anim_frame >= frame_count {
                    if sprite.anim_looping {
                        sprite.anim_frame = 0;
                    } else {
                        sprite.anim_frame = frame_count - 1;
                        sprite.anim_finished = true;
                        break;
                    }
                }
            }
        }

        // Visual: splash animation
        let splash_frame_ms = 1000.0 / SPLASH_FPS;
        for splash in &mut self.splashes {
            splash.timer += dt_ms;
            splash.frame = (splash.timer / splash_frame_ms) as usize;
        }
        self.splashes.retain(|s| s.frame < SPLASH_FRAMES);
    }
}
