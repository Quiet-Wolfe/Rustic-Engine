use rustic_core::rating;
use rustic_gameplay::events::GameEvent;

use super::{
    DeathPhase, DeathState, NoteSplash, PlayScreen, RatingPopup, GAME_W, HEALTH_BAR_H,
    HEALTH_BAR_W, HEALTH_BAR_X, HEALTH_BAR_Y, RATING_ACCEL, RATING_FADE_SECS, RATING_VEL_Y,
    SPLASH_FPS, SPLASH_FRAMES,
};

/// Convert an sRGB component (0..1) to linear space.
fn srgb_to_linear(s: f32) -> f32 {
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// Convert an sRGB [R,G,B,A] color to linear space (alpha unchanged).
fn srgb_color(r: f32, g: f32, b: f32) -> [f32; 4] {
    [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), 1.0]
}

/// Parse a hex color string (e.g., "CCCCCC", "FB002D") to [R,G,B,A] floats in linear space.
fn parse_hex_color(hex: &str) -> [f32; 4] {
    let hex = hex
        .trim_start_matches('#')
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    let hex = if hex.len() > 6 {
        &hex[hex.len() - 6..]
    } else {
        hex
    };
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return srgb_color(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        }
    }
    srgb_color(0.8, 0.8, 0.8) // fallback: CCCCCC
}

impl PlayScreen {
    pub(super) fn update_inner(&mut self, dt: f32) {
        self.last_dt = dt;
        let dt_ms = dt as f64 * 1000.0;
        let mut blocking_cutscene = false;
        if let Some(super::CutsceneState::Video {
            player,
            wall_clock_ms,
            blocks_gameplay,
            ..
        }) = &mut self.cutscene
        {
            *wall_clock_ms += dt_ms;
            if player.is_playing() {
                player.tick(*wall_clock_ms);
            }
            blocking_cutscene = *blocks_gameplay;
            let finished = player.is_finished();
            if finished {
                self.finish_cutscene();
            }
        }
        if blocking_cutscene {
            return;
        }

        if self.pause_menu.is_some() {
            self.update_pause(dt);
            return;
        }

        // Death state machine (visual only)
        if let Some(death) = &mut self.death {
            let dt_ms = dt as f64 * 1000.0;
            death.timer += dt_ms;
            death.character.update(dt);

            match death.phase {
                DeathPhase::FirstDeath => {
                    if death.character.anim_finished() {
                        death.phase = DeathPhase::Loop;
                        death.character.play_anim("deathLoop", false);
                        if let Some(audio) = &mut self.audio {
                            if let Some(music) = self.paths.music(&self.death_loop_name) {
                                audio.play_loop_music(&music);
                            }
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

        // Health drain animation (sliding health bar for harmful notes)
        if let Some(drain) = &mut self.health_drain {
            drain.elapsed += dt;
            let t = (drain.elapsed / drain.duration).min(1.0);
            // Ease out quad
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            self.game.score.health = drain.start + (drain.target - drain.start) * eased;
            if t >= 1.0 {
                self.game.score.health = drain.target;
                self.health_drain = None;
            }
        }

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

        // Sync game state to scripting layer before callbacks
        self.scripts.state.song_position = self.game.conductor.song_position;
        self.scripts.state.camera_zoom = self.camera.zoom;
        self.scripts.state.default_cam_zoom = self.default_cam_zoom;
        self.scripts.state.camera_speed = self.camera.camera_speed;
        self.scripts.state.health = self.game.score.health;
        self.scripts.state.score = self.game.score.score;
        self.scripts.state.misses = self.game.score.misses;
        self.scripts.state.hits = self.game.score.total_notes_played - self.game.score.misses;
        self.scripts.state.combo = self.game.score.combo;
        let rating_value = self.game.score.accuracy() / 100.0;
        self.scripts.state.rating = self.scripts.state.rating_override.unwrap_or(rating_value);
        self.scripts.state.rating_name = self
            .scripts
            .state
            .rating_name_override
            .clone()
            .unwrap_or_else(|| self.game.score.grade().to_string());
        let fc = match rating::classify_fc(
            self.game.score.sicks,
            self.game.score.goods,
            self.game.score.bads,
            self.game.score.shits,
            self.game.score.misses,
        ) {
            rating::FcClassification::Sfc => "SFC",
            rating::FcClassification::Gfc => "GFC",
            rating::FcClassification::Fc => "FC",
            rating::FcClassification::Sdcb => "SDCB",
            rating::FcClassification::Clear => "Clear",
        };
        self.scripts.state.rating_fc = self
            .scripts
            .state
            .rating_fc_override
            .clone()
            .unwrap_or_else(|| fc.to_string());
        if let Some(audio) = &self.audio {
            self.scripts
                .set_on_all("__music_time", audio.loop_music_position_ms());
        }

        // Push game object properties that scripts commonly read
        use rustic_scripting::LuaValue as SLV;
        let health_pct = self.game.score.health / 2.0;
        let (bar_x, bar_y, overlay_x, overlay_y, overlay_w, overlay_h, bar_w, bar_h, icon_y) =
            if let Some(chb) = &self.custom_healthbar {
                let scale = chb.scale;
                let overlay_w = chb.overlay_texture.width as f32 * scale;
                let overlay_h = chb.overlay_texture.height as f32 * scale;
                let bar_w = chb.bar_texture.width as f32 * scale;
                let bar_h = chb.bar_texture.height as f32 * scale;
                let overlay_x = (GAME_W - overlay_w) / 2.0;
                let overlay_y = HEALTH_BAR_Y - overlay_h / 2.0;
                let bar_x = overlay_x + (overlay_w - bar_w) / 2.0;
                let bar_y = overlay_y + (overlay_h - bar_h) / 2.0;
                let icon_size = 150.0 * 0.75;
                let icon_y = overlay_y + overlay_h / 2.0 - icon_size / 2.0;
                (
                    bar_x, bar_y, overlay_x, overlay_y, overlay_w, overlay_h, bar_w, bar_h, icon_y,
                )
            } else {
                let icon_size = 150.0 * 0.75;
                let icon_y = HEALTH_BAR_Y + HEALTH_BAR_H / 2.0 - icon_size / 2.0;
                (
                    HEALTH_BAR_X,
                    HEALTH_BAR_Y,
                    HEALTH_BAR_X,
                    HEALTH_BAR_Y,
                    HEALTH_BAR_W,
                    HEALTH_BAR_H,
                    HEALTH_BAR_W,
                    HEALTH_BAR_H,
                    icon_y,
                )
            };
        let divider_x = bar_x + bar_w * (1.0 - health_pct as f32);
        // iconP1 = player icon (BF), iconP2 = opponent icon
        self.scripts
            .state
            .custom_vars
            .insert("iconP1.x".into(), SLV::Float((divider_x - 15.0) as f64));
        self.scripts.state.custom_vars.insert(
            "iconP2.x".into(),
            SLV::Float((divider_x - 150.0 * 0.75 + 15.0) as f64),
        );
        self.scripts
            .state
            .custom_vars
            .entry("iconP1.alpha".into())
            .or_insert(SLV::Float(1.0));
        self.scripts
            .state
            .custom_vars
            .entry("iconP2.alpha".into())
            .or_insert(SLV::Float(1.0));
        for (key, value) in [
            ("bar.leftBar.x", bar_x),
            ("bar.leftBar.y", bar_y),
            ("bar.leftBar.width", bar_w),
            ("bar.leftBar.height", bar_h),
            ("bar.rightBar.x", bar_x),
            ("bar.rightBar.y", bar_y),
            ("bar.rightBar.width", bar_w),
            ("bar.rightBar.height", bar_h),
            ("bar.overlay.x", overlay_x),
            ("bar.overlay.y", overlay_y),
            ("bar.overlay.width", overlay_w),
            ("bar.overlay.height", overlay_h),
            ("iconP1.y", icon_y),
            ("iconP2.y", icon_y),
        ] {
            self.scripts
                .state
                .custom_vars
                .entry(key.into())
                .or_insert(SLV::Float(value as f64));
        }
        // Camera follow position
        self.scripts
            .state
            .custom_vars
            .insert("camFollow.x".into(), SLV::Float(self.camera.x as f64));
        self.scripts
            .state
            .custom_vars
            .insert("camFollow.y".into(), SLV::Float(self.camera.y as f64));
        // Unspawn notes length (approximate — total notes remaining)
        self.scripts.state.custom_vars.insert(
            "unspawnNotes.length".into(),
            SLV::Int(self.game.notes.len() as i64),
        );
        // Conductor timing
        self.scripts
            .state
            .custom_vars
            .insert("crochet".into(), SLV::Float(self.game.conductor.crochet));
        self.scripts.state.custom_vars.insert(
            "stepCrochet".into(),
            SLV::Float(self.game.conductor.step_crochet),
        );

        // Character midpoints for getMidpointX/Y('dad'), getMidpointX/Y('boyfriend')
        if let Some(dad) = &self.char_dad {
            let (mx, my) = dad.midpoint();
            self.scripts.set_on_all("__midX_dad", mx as f64);
            self.scripts.set_on_all("__midY_dad", my as f64);
            self.scripts.set_on_all("__midX_opponent", mx as f64);
            self.scripts.set_on_all("__midY_opponent", my as f64);
        }
        if let Some(bf) = &self.char_bf {
            let (mx, my) = bf.midpoint();
            self.scripts.set_on_all("__midX_boyfriend", mx as f64);
            self.scripts.set_on_all("__midY_boyfriend", my as f64);
            self.scripts.set_on_all("__midX_bf", mx as f64);
            self.scripts.set_on_all("__midY_bf", my as f64);
        }

        // Update tweens/timers BEFORE Lua callbacks (matches Psych Engine: FlxG updates
        // tweens first, then game update runs Lua callbacks which can override tween values)
        if self.scripts.has_scripts() {
            self.scripts.update_tweens(dt);
            self.process_property_writes();
        }

        // Sync character state so Lua getProperty works for animation names and positions
        if let Some(dad) = &self.char_dad {
            self.scripts.state.dad_anim_name = dad.current_anim_name().to_string();
            self.scripts.state.dad_anim_frame = dad.anim_frame_index();
            self.scripts.state.dad_anim_finished = dad.anim_finished();
            self.scripts.state.dad_pos = (dad.x(), dad.y());
            let camera_position = dad.camera_position();
            self.scripts.state.dad_camera_position =
                (camera_position[0] as f32, camera_position[1] as f32);
        }
        if let Some(bf) = &self.char_bf {
            self.scripts.state.bf_anim_name = bf.current_anim_name().to_string();
            self.scripts.state.bf_anim_frame = bf.anim_frame_index();
            self.scripts.state.bf_anim_finished = bf.anim_finished();
            self.scripts.state.bf_pos = (bf.x(), bf.y());
            let camera_position = bf.camera_position();
            self.scripts.state.bf_camera_position =
                (camera_position[0] as f32, camera_position[1] as f32);
        }
        if let Some(gf) = &self.char_gf {
            self.scripts.state.gf_anim_name = gf.current_anim_name().to_string();
            self.scripts.state.gf_anim_frame = gf.anim_frame_index();
            self.scripts.state.gf_anim_finished = gf.anim_finished();
            self.scripts.state.gf_pos = (gf.x(), gf.y());
            let camera_position = gf.camera_position();
            self.scripts.state.gf_camera_position =
                (camera_position[0] as f32, camera_position[1] as f32);
        }

        // Lua: onUpdate (before gameplay logic)
        if self.scripts.has_scripts() {
            self.scripts.call_with_elapsed("onUpdate", dt as f64);
        }
        self.process_audio_requests();

        // === RL: observe and act (before gameplay update so presses feed
        //       into this tick's hit-detection).
        #[cfg(feature = "rl")]
        self.rl_pre_update();

        // === Call into gameplay logic ===
        let audio_pos = if self.game.song_started {
            self.audio.as_ref().map(|a| a.position_ms())
        } else {
            None
        };
        let audio_finished = self.audio.as_ref().is_some_and(|a| a.is_finished());
        self.game.update(dt, audio_pos, audio_finished);

        // === RL: pair reward + record step after the game state has advanced.
        #[cfg(feature = "rl")]
        self.rl_post_update();

        // === Dispatch chart events (onEvent) ===
        let song_pos = self.game.conductor.song_position;
        while self.event_index < self.chart_events.len()
            && self.chart_events[self.event_index].strum_time <= song_pos
        {
            let evt = &self.chart_events[self.event_index];
            let name = evt.name.clone();
            let v1 = evt.value1.clone();
            let v2 = evt.value2.clone();
            self.event_index += 1;

            log::debug!("Chart event: {} ({}, {})", name, v1, v2);

            // Fire onEvent on all Lua scripts
            if self.scripts.has_scripts() {
                self.scripts.call_event(&name, &v1, &v2);
                self.process_property_writes();
            }

            // Handle built-in events
            match name.as_str() {
                "Add Camera Zoom" => {
                    let game_zoom: f32 = v1.parse().unwrap_or(0.015);
                    let hud_zoom: f32 = v2.parse().unwrap_or(0.03);
                    if !self.disable_zooming && self.camera.zoom < 1.35 {
                        self.camera.zoom += game_zoom;
                        self.hud_zoom += hud_zoom;
                    }
                }
                "Change Scroll Speed" => {
                    let multiplier: f64 = v1.parse().unwrap_or(1.0);
                    let _duration: f32 = v2.parse().unwrap_or(0.0);
                    self.game.song_speed = self.game.base_song_speed * multiplier;
                }
                "Set GF Speed" => {
                    // v1 = frequency (1 = every beat, 2 = every 2 beats)
                    self.gf_dance_freq = v1.parse::<i32>().unwrap_or(0);
                }
                "Play Animation" => {
                    // v1 = animation name, v2 = character (empty = dad)
                    let target = if v2.is_empty() { "dad" } else { v2.as_str() };
                    match target {
                        "dad" | "opponent" | "1" => {
                            if let Some(dad) = &mut self.char_dad {
                                dad.play_anim(&v1, true);
                            }
                        }
                        "bf" | "boyfriend" | "0" => {
                            if let Some(bf) = &mut self.char_bf {
                                bf.play_anim(&v1, true);
                            }
                        }
                        "gf" | "girlfriend" | "2" => {
                            if let Some(gf) = &mut self.char_gf {
                                gf.play_anim(&v1, true);
                            }
                        }
                        _ => {}
                    }
                }
                "Change Character" => {
                    // v1 = target ("dad", "bf", "gf", or "0"/"1"/"2")
                    // v2 = new character name
                    if !v2.is_empty() {
                        self.char_change_requests.push((v1.clone(), v2.clone()));
                    }
                }
                // Stage color events (data-driven from events.json, use generic overlay)
                "Nightflaid color tween" => {
                    let color = parse_hex_color(&v1);
                    let dur: f32 = v2.parse().unwrap_or(1.0);
                    self.stage_color_both(color, dur);
                }
                "Nightflaid color tween left-only" => {
                    let color = parse_hex_color(&v1);
                    let dur: f32 = v2.parse().unwrap_or(1.0);
                    self.stage_color_left(color, dur);
                }
                "Nightflaid color tween right-only" => {
                    let color = parse_hex_color(&v1);
                    let dur: f32 = v2.parse().unwrap_or(1.0);
                    self.stage_color_right(color, dur);
                }
                "Nightflaid swap sides" => {
                    let dur: f32 = v1.parse().unwrap_or(0.15);
                    let old_left = self.stage_overlay.color_left;
                    let old_right = self.stage_overlay.color_right;
                    self.stage_color_left(old_right, dur);
                    self.stage_color_right(old_left, dur);
                }
                "Nightflaid lightings" => {
                    let on = matches!(v1.to_lowercase().as_str(), "on" | "1" | "");
                    self.stage_overlay.lights_on = on;
                }
                "Wildcard" => {
                    // VS Retrospecter custom event: calls Lua function by name.
                    // v1 = function name, v2 = argument
                    if self.scripts.has_scripts() {
                        self.scripts.call_lua_function(&v1, &v2);
                        self.process_property_writes();
                    }
                    // setOppAnimation: set the opponent's animation suffix
                    if v1 == "setOppAnimation" {
                        if let Some(dad) = &mut self.char_dad {
                            let suffix = if v2.is_empty() {
                                String::new()
                            } else {
                                format!("-{}", v2)
                            };
                            log::info!("Setting opponent anim suffix to '{}'", suffix);
                            dad.set_anim_suffix(&suffix);
                        }
                        // Health bar colors are handled by rustic/rustic_ext.lua via setHealthBarColor
                    }
                }
                "Midsong Video" => {
                    // Play a mid-song video. v1 = video name (filename without extension).
                    // The Lua onEvent callback fires first (for character changes etc),
                    // then this queues the video for playback in the draw phase.
                    // Non-blocking: the song keeps playing and gameplay continues.
                    if !v1.is_empty() {
                        self.scripts
                            .state
                            .video_requests
                            .push((v1.clone(), None, false));
                    }
                }
                "Checkpoint" => {
                    // Store checkpoint position so getPropertyFromClass('states.PlayState', 'pressedCheckpoint') returns true
                    self.scripts.set_on_all("__pressedCheckpoint", 1.0);
                    log::info!("Checkpoint reached at song position {}", song_pos);
                }
                _ => {}
            }
        }

        // === Process gameplay events ===
        let events = self.game.drain_events();
        for event in events {
            match event {
                GameEvent::NoteHit {
                    lane,
                    rating,
                    combo,
                    note_type,
                    is_sustain,
                    members_index,
                    hit_causes_miss,
                    ..
                } => {
                    if hit_causes_miss {
                        // Harmful note: play miss animation, show miss popup
                        self.rating_popups.push(RatingPopup {
                            rating_name: "miss".into(),
                            combo: 0,
                            y: 0.0,
                            vel_y: RATING_VEL_Y,
                            age_ms: 0.0,
                            fade_delay: self.game.conductor.crochet,
                            alpha: 1.0,
                        });
                        if self.game.play_as_opponent {
                            if let Some(dad) = &mut self.char_dad {
                                dad.play_miss(lane);
                            }
                        } else {
                            if let Some(bf) = &mut self.char_bf {
                                bf.play_miss(lane);
                            }
                        }
                    } else {
                        // Normal hit: show rating popup
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
                                lane,
                                player: !self.game.play_as_opponent,
                                frame: 0,
                                timer: 0.0,
                            });
                        }
                        // Character: sing
                        if self.game.play_as_opponent {
                            if let Some(dad) = &mut self.char_dad {
                                dad.play_sing(lane);
                            }
                        } else {
                            if let Some(bf) = &mut self.char_bf {
                                bf.play_sing(lane);
                            }
                        }
                    }
                    // Lua: goodNoteHit(membersIndex, noteData, noteType, isSustainNote)
                    if self.scripts.has_scripts() {
                        let lua_event = if self.game.play_as_opponent {
                            "opponentNoteHit"
                        } else {
                            "goodNoteHit"
                        };
                        self.scripts.call_note_hit(
                            lua_event,
                            members_index,
                            lane,
                            &note_type,
                            is_sustain,
                            false,
                        );
                    }
                }
                GameEvent::NoteMiss {
                    lane,
                    note_type,
                    members_index,
                    ignored,
                } => {
                    if !ignored {
                        self.rating_popups.push(RatingPopup {
                            rating_name: "miss".into(),
                            combo: 0,
                            y: 0.0,
                            vel_y: RATING_VEL_Y,
                            age_ms: 0.0,
                            fade_delay: self.game.conductor.crochet,
                            alpha: 1.0,
                        });
                        if self.game.play_as_opponent {
                            if let Some(dad) = &mut self.char_dad {
                                dad.play_miss(lane);
                            }
                        } else {
                            if let Some(bf) = &mut self.char_bf {
                                bf.play_miss(lane);
                            }
                        }
                    }
                    // Lua: noteMiss(membersIndex, noteData, noteType, isSustainNote)
                    if self.scripts.has_scripts() {
                        self.scripts.call_note_hit(
                            "noteMiss",
                            members_index,
                            lane,
                            &note_type,
                            false,
                            false,
                        );
                    }
                }
                GameEvent::OpponentNoteHit {
                    lane,
                    note_type,
                    is_sustain,
                    members_index,
                    hit_causes_miss: _,
                } => {
                    if !self.disable_zooming {
                        self.cam_zooming = true;
                    }
                    if self.game.play_as_opponent {
                        if let Some(bf) = &mut self.char_bf {
                            bf.play_sing(lane);
                        }
                    } else {
                        if let Some(dad) = &mut self.char_dad {
                            dad.play_sing(lane);
                        }
                    }
                    // Lua: opponentNoteHit(membersIndex, noteData, noteType, isSustainNote)
                    if self.scripts.has_scripts() {
                        let lua_event = if self.game.play_as_opponent {
                            "goodNoteHit"
                        } else {
                            "opponentNoteHit"
                        };
                        self.scripts.call_note_hit(
                            lua_event,
                            members_index,
                            lane,
                            &note_type,
                            is_sustain,
                            false,
                        );
                    }
                }
                GameEvent::CountdownBeat { swag } => {
                    if let Some(audio) = &mut self.audio {
                        let sfx_name = match swag {
                            0 => Some("intro3"),
                            1 => Some("intro2"),
                            2 => Some("intro1"),
                            3 => Some("introGo"),
                            _ => None,
                        };
                        if let Some(name) = sfx_name {
                            if let Some(sfx) = self.paths.sound(name) {
                                audio.play_sound(&sfx, 0.6);
                            }
                        }
                        match swag {
                            1 => {
                                self.countdown_swag = 1;
                                self.countdown_alpha = 1.0;
                            }
                            2 => {
                                self.countdown_swag = 2;
                                self.countdown_alpha = 1.0;
                            }
                            3 => {
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
                    self.completed_song = true;
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
                    // Character dance — dance() itself guards against interrupting
                    // special animations (descend, ascend, intro, etc.)
                    if let Some(dad) = &mut self.char_dad {
                        let freq = if dad.has_dance_idle() { 1 } else { 2 };
                        if beat % freq == 0 && !dad.current_anim().starts_with("sing") {
                            dad.dance();
                        }
                    }
                    if let Some(bf) = &mut self.char_bf {
                        let freq = if bf.has_dance_idle() { 1 } else { 2 };
                        if beat % freq == 0 && !bf.current_anim().starts_with("sing") {
                            bf.dance();
                        }
                    }
                    if let Some(gf) = &mut self.char_gf {
                        let freq = if self.gf_dance_freq > 0 {
                            self.gf_dance_freq
                        } else {
                            if gf.has_dance_idle() {
                                1
                            } else {
                                2
                            }
                        };
                        if beat % freq == 0 {
                            gf.dance();
                        }
                    }
                    // Icon bop (every 2 beats for custom bar, every beat otherwise)
                    if self.custom_healthbar.is_some() {
                        if beat % 2 == 0 {
                            self.icon_scale_bf = 1.2;
                            self.icon_scale_dad = 1.2;
                        }
                    } else {
                        self.icon_scale_bf = 1.2;
                        self.icon_scale_dad = 1.2;
                    }
                    // Custom health bar fade-in at beat 16 (or later if time was skipped)
                    if let Some(chb) = &mut self.custom_healthbar {
                        if !chb.visible && beat >= 16 {
                            chb.fade_in();
                        }
                    }
                    // Lua: onBeatHit
                    if self.scripts.has_scripts() {
                        self.scripts.call_beat("onBeatHit", beat);
                    }
                }
                GameEvent::SectionChange { must_hit, .. } => {
                    // Update mustHitSection global for scripts
                    self.scripts.set_bool_on_all("mustHitSection", must_hit);
                    if !self.camera_forced_pos {
                        self.recompute_camera_targets();
                        let target = if must_hit { self.cam_bf } else { self.cam_dad };
                        self.camera.follow(target[0], target[1]);
                    }
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
                GameEvent::HarmfulNoteHit {
                    sfx_path,
                    drain_pct,
                    death_safe,
                } => {
                    // Play custom SFX
                    if !sfx_path.is_empty() {
                        if let Some(audio) = &mut self.audio {
                            if let Some(sfx) = self.paths.sound(&sfx_path) {
                                audio.play_sound(&sfx, 1.0);
                            }
                        }
                    }
                    // Start animated health drain
                    if drain_pct > 0.0 {
                        let current = self.game.score.health;
                        // Max health is 2.0; drain_pct is fraction of max (e.g. 0.5 = drain 1.0)
                        let drain_amount = drain_pct * 2.0;
                        let raw_target = current - drain_amount;
                        let target = if death_safe && current > 0.05 {
                            // Safe: clamp to just above death threshold, but only if
                            // health was comfortably above it. A second hit when already
                            // near the threshold is lethal.
                            raw_target.max(0.025) // ~1.25% health, just above death
                        } else {
                            raw_target.max(0.0)
                        };
                        self.health_drain = Some(super::HealthDrain {
                            start: current,
                            target,
                            elapsed: 0.0,
                            duration: 0.5,
                        });
                    }
                }
                GameEvent::Death => {
                    self.death_counter += 1;
                    if let Some(audio) = &mut self.audio {
                        audio.pause();
                        if let Some(sfx) = self.paths.sound(&self.death_sound_name) {
                            audio.play_sound(&sfx, 1.0);
                        }
                    }
                    if let Some(mut death_char) = self.death_char_preloaded.take() {
                        if let Some(bf) = &self.char_bf {
                            death_char.set_x(bf.x());
                            death_char.set_y(bf.y());
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
        // Opponent: don't return to idle while any hold note is sustaining
        let opp_holding = self.game.opponent_confirm.iter().any(|&c| c > 0.0);
        if let Some(dad) = &mut self.char_dad {
            if dad.current_anim().starts_with("sing") {
                let ht = dad.hold_timer() + dt_ms;
                dad.set_hold_timer(ht);
                let threshold = self.game.conductor.step_crochet * dad.sing_duration() * 1.1;
                if ht >= threshold && !opp_holding {
                    dad.dance();
                    dad.set_hold_timer(0.0);
                }
            }
            dad.update(dt);
        }

        if let Some(bf) = &mut self.char_bf {
            if bf.current_anim().starts_with("sing") {
                let ht = bf.hold_timer() + dt_ms;
                bf.set_hold_timer(ht);
                let threshold = self.game.conductor.step_crochet * bf.sing_duration() * 1.1;
                if ht >= threshold && !self.game.keys_held.iter().any(|&k| k) {
                    bf.dance();
                    bf.set_hold_timer(0.0);
                }
            }
            bf.update(dt);
        }

        if let Some(gf) = &mut self.char_gf {
            gf.update(dt);
        }

        for instance in self.lua_characters.values_mut() {
            instance.character.update(dt);
        }

        if let Some(dad) = &self.char_dad {
            self.scripts.state.dad_anim_name = dad.current_anim_name().to_string();
            self.scripts.state.dad_anim_frame = dad.anim_frame_index();
            self.scripts.state.dad_anim_finished = dad.anim_finished();
            self.scripts.state.dad_pos = (dad.x(), dad.y());
            let camera_position = dad.camera_position();
            self.scripts.state.dad_camera_position =
                (camera_position[0] as f32, camera_position[1] as f32);
        }
        if let Some(bf) = &self.char_bf {
            self.scripts.state.bf_anim_name = bf.current_anim_name().to_string();
            self.scripts.state.bf_anim_frame = bf.anim_frame_index();
            self.scripts.state.bf_anim_finished = bf.anim_finished();
            self.scripts.state.bf_pos = (bf.x(), bf.y());
            let camera_position = bf.camera_position();
            self.scripts.state.bf_camera_position =
                (camera_position[0] as f32, camera_position[1] as f32);
        }
        if let Some(gf) = &self.char_gf {
            self.scripts.state.gf_anim_name = gf.current_anim_name().to_string();
            self.scripts.state.gf_anim_frame = gf.anim_frame_index();
            self.scripts.state.gf_anim_finished = gf.anim_finished();
            self.scripts.state.gf_pos = (gf.x(), gf.y());
            let camera_position = gf.camera_position();
            self.scripts.state.gf_camera_position =
                (camera_position[0] as f32, camera_position[1] as f32);
        }

        // Visual: icon scale decay
        let icon_lerp = (-dt * 9.0).exp();
        self.icon_scale_bf = 1.0 + (self.icon_scale_bf - 1.0) * icon_lerp;
        self.icon_scale_dad = 1.0 + (self.icon_scale_dad - 1.0) * icon_lerp;

        // Custom health bar update
        if let Some(chb) = &mut self.custom_healthbar {
            let health = self.game.score.health as f32;
            chb.update(dt, health);
        }

        // Camera zoom decay
        let zoom_before = self.camera.zoom;
        self.camera.update(dt);
        if self.cam_zooming {
            let zoom_lerp = (-dt * 3.125).exp();
            self.camera.zoom =
                self.default_cam_zoom + (zoom_before - self.default_cam_zoom) * zoom_lerp;
        }

        // HUD zoom decay
        let hud_lerp = (-dt * 3.125).exp();
        self.hud_zoom = 1.0 + (self.hud_zoom - 1.0) * hud_lerp;

        // Lua: onUpdatePost (after all game logic)
        if self.scripts.has_scripts() {
            self.scripts.call_with_elapsed("onUpdatePost", dt as f64);
        }

        // Process game-level property writes from Lua scripts
        self.process_property_writes();

        // Process Lua extension requests (stage colors, health bar colors, etc.)
        self.process_lua_extensions();

        // Update generic stage overlay color tweens
        self.update_stage_overlay(dt);

        // Process moveCameraSection requests from scripts (runHaxeCode / moveCameraSection)
        let cam_sections: Vec<i32> = self
            .scripts
            .state
            .camera_section_requests
            .drain(..)
            .collect();
        for section_idx in cam_sections {
            let idx = section_idx as usize;
            if idx < self.game.sections.len() {
                let must_hit = self.game.sections[idx].must_hit;
                self.recompute_camera_targets();
                let target = if must_hit { self.cam_bf } else { self.cam_dad };
                self.camera.follow(target[0], target[1]);
            }
        }

        // Process camera target requests from Lua (cameraSetTarget)
        let cam_targets: Vec<String> = self
            .scripts
            .state
            .camera_target_requests
            .drain(..)
            .collect();
        for target in cam_targets {
            let raw_target = target.trim();
            let (target, snap) = raw_target
                .strip_prefix("__snap:")
                .map(|target| (target, true))
                .unwrap_or((raw_target, false));
            let mut follow_target = None;
            match target.to_lowercase().as_str() {
                "dad" | "opponent" => {
                    self.recompute_camera_targets();
                    follow_target = Some((self.cam_dad[0], self.cam_dad[1]));
                }
                "gf" | "girlfriend" => {
                    // GF camera: use GF midpoint if available, otherwise dad position
                    if let Some(gf) = &self.char_gf {
                        let (mx, my) = gf.midpoint();
                        follow_target = Some((mx, my));
                    }
                }
                _ => {
                    // Default = boyfriend
                    self.recompute_camera_targets();
                    follow_target = Some((self.cam_bf[0], self.cam_bf[1]));
                }
            }
            if let Some((x, y)) = follow_target {
                if snap {
                    self.camera.snap_to(x, y);
                } else {
                    self.camera.follow(x, y);
                }
            }
        }

        // Process triggered events from Lua (triggerEvent)
        let events: Vec<(String, String, String)> =
            self.scripts.state.triggered_events.drain(..).collect();
        for (name, v1, v2) in events {
            if self.scripts.has_scripts() {
                self.scripts.call_event(&name, &v1, &v2);
                self.process_property_writes();
            }

            match name.as_str() {
                "Add Camera Zoom" => {
                    let game_zoom: f32 = v1.parse().unwrap_or(0.015);
                    let hud_zoom: f32 = v2.parse().unwrap_or(0.03);
                    if !self.disable_zooming && self.camera.zoom < 1.35 {
                        self.camera.zoom += game_zoom;
                        self.hud_zoom += hud_zoom;
                    }
                }
                "Change Scroll Speed" => {
                    let multiplier: f64 = v1.parse().unwrap_or(1.0);
                    self.game.song_speed = self.game.base_song_speed * multiplier;
                }
                "Set GF Speed" => {
                    self.gf_dance_freq = v1.parse::<i32>().unwrap_or(0);
                }
                "Play Animation" => {
                    let target = if v2.is_empty() { "dad" } else { v2.as_str() };
                    match target.to_ascii_lowercase().as_str() {
                        "dad" | "opponent" | "1" => {
                            if let Some(dad) = &mut self.char_dad {
                                dad.play_anim(&v1, true);
                            }
                        }
                        "bf" | "boyfriend" | "0" => {
                            if let Some(bf) = &mut self.char_bf {
                                bf.play_anim(&v1, true);
                            }
                        }
                        "gf" | "girlfriend" | "2" => {
                            if let Some(gf) = &mut self.char_gf {
                                gf.play_anim(&v1, true);
                            }
                        }
                        _ => {}
                    }
                }
                "Change Character" => {
                    if !v2.is_empty() {
                        self.char_change_requests.push((v1.clone(), v2.clone()));
                    }
                }
                _ => {
                    log::debug!("Unhandled triggerEvent: {} ({}, {})", name, v1, v2);
                }
            }
        }

        // Process camera shake requests
        for (camera, intensity, duration) in self.scripts.state.camera_shake_requests.drain(..) {
            log::debug!(
                "Camera shake: {} intensity={} duration={}",
                camera,
                intensity,
                duration
            );
            if camera == "camGame" {
                self.camera.start_shake(intensity, duration);
            }
            // camHUD shake would need HUD shake state — log for now
        }

        // Process camera flash requests
        for (camera, color, duration, alpha) in self.scripts.state.camera_flash_requests.drain(..) {
            log::debug!(
                "Camera flash: {} color={} duration={} alpha={}",
                camera,
                color,
                duration,
                alpha
            );
            if camera == "camGame" {
                self.camera.start_flash(&color, duration, alpha);
            }
        }

        for (camera, color, duration, fade_in) in self.scripts.state.camera_fade_requests.drain(..)
        {
            log::debug!(
                "Camera fade: {} color={} duration={} fade_in={}",
                camera,
                color,
                duration,
                fade_in
            );
            if camera == "camGame" {
                self.camera.start_fade(&color, duration, fade_in);
            }
        }

        // Process subtitle requests (display as log for now; rendering handled by text system)
        for (text, _font, _color, _size, _duration, _border) in
            self.scripts.state.subtitle_requests.drain(..)
        {
            if !text.trim().is_empty() {
                log::info!("[Subtitle] {}", text);
            }
        }

        // Visual: Lua sprite animation advancement
        for (tag, sprite) in self.scripts.state.lua_sprites.iter_mut() {
            if sprite.current_anim.is_empty() || sprite.anim_finished {
                continue;
            }
            if sprite.anim_fps <= 0.0 {
                continue;
            }
            let frame_count = self
                .lua_atlases
                .get(tag.as_str())
                .map(|a| a.frame_count(&sprite.current_anim))
                .unwrap_or(0);
            if frame_count == 0 {
                continue;
            }

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

        self.scripts.state.input_just_pressed.clear();
        self.scripts.state.input_just_released.clear();
        self.scripts.state.mouse_just_pressed = false;
        self.scripts.state.mouse_just_released = false;
    }
}
