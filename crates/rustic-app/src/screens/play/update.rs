use std::path::Path;

use rustic_gameplay::events::GameEvent;

use super::{
    PlayScreen, RatingPopup, DeathState, DeathPhase, NoteSplash,
    RATING_ACCEL, RATING_VEL_Y, RATING_FADE_SECS,
    SPLASH_FPS, SPLASH_FRAMES, GAME_W,
};

/// Convert an sRGB component (0..1) to linear space.
fn srgb_to_linear(s: f32) -> f32 {
    if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

/// Convert an sRGB [R,G,B,A] color to linear space (alpha unchanged).
fn srgb_color(r: f32, g: f32, b: f32) -> [f32; 4] {
    [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), 1.0]
}

/// Parse a hex color string (e.g., "CCCCCC", "FB002D") to [R,G,B,A] floats in linear space.
fn parse_hex_color(hex: &str) -> [f32; 4] {
    let hex = hex.trim_start_matches('#').trim_start_matches("0x").trim_start_matches("0X");
    let hex = if hex.len() > 6 { &hex[hex.len()-6..] } else { hex };
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
        if self.paused { return; }

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

        // Sync game state to scripting layer before callbacks
        self.scripts.state.song_position = self.game.conductor.song_position;
        self.scripts.state.camera_zoom = self.camera.zoom;
        self.scripts.state.default_cam_zoom = self.default_cam_zoom;
        self.scripts.state.camera_speed = self.camera.camera_speed;
        self.scripts.state.health = self.game.score.health;

        // Push game object properties that scripts commonly read
        use rustic_scripting::LuaValue as SLV;
        let health_pct = self.game.score.health / 2.0;
        let hbx = (GAME_W - 601.0) / 2.0;
        let hbw = 601.0;
        let divider_x = hbx + hbw * (1.0 - health_pct as f32);
        // iconP1 = player icon (BF), iconP2 = opponent icon
        self.scripts.state.custom_vars.insert("iconP1.x".into(), SLV::Float((divider_x - 15.0) as f64));
        self.scripts.state.custom_vars.insert("iconP2.x".into(), SLV::Float((divider_x - 150.0 * 0.75 + 15.0) as f64));
        self.scripts.state.custom_vars.insert("iconP1.alpha".into(), SLV::Float(1.0));
        self.scripts.state.custom_vars.insert("iconP2.alpha".into(), SLV::Float(1.0));
        // Camera follow position
        self.scripts.state.custom_vars.insert("camFollow.x".into(), SLV::Float(self.camera.x as f64));
        self.scripts.state.custom_vars.insert("camFollow.y".into(), SLV::Float(self.camera.y as f64));
        // Unspawn notes length (approximate — total notes remaining)
        self.scripts.state.custom_vars.insert("unspawnNotes.length".into(), SLV::Int(self.game.notes.len() as i64));
        // Conductor timing
        self.scripts.state.custom_vars.insert("crochet".into(), SLV::Float(self.game.conductor.crochet));
        self.scripts.state.custom_vars.insert("stepCrochet".into(), SLV::Float(self.game.conductor.step_crochet));

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
            self.scripts.state.dad_pos = (dad.x(), dad.y());
        }
        if let Some(bf) = &self.char_bf {
            self.scripts.state.bf_anim_name = bf.current_anim_name().to_string();
            self.scripts.state.bf_pos = (bf.x(), bf.y());
        }
        if let Some(gf) = &self.char_gf {
            self.scripts.state.gf_anim_name = gf.current_anim_name().to_string();
            self.scripts.state.gf_pos = (gf.x(), gf.y());
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
                    // Sets GF dance frequency (not implemented yet)
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
                // Nightflaid stage color events (normally handled via runHaxeCode, implemented natively)
                "Nightflaid color tween" if self.stage_name == "nightflaid" => {
                    let color = parse_hex_color(&v1);
                    let dur: f32 = v2.parse().unwrap_or(1.0);
                    self.nightflaid_color_tween_both(color, dur);
                }
                "Nightflaid color tween left-only" if self.stage_name == "nightflaid" => {
                    let color = parse_hex_color(&v1);
                    let dur: f32 = v2.parse().unwrap_or(1.0);
                    self.nightflaid_color_tween_left(color, dur);
                }
                "Nightflaid color tween right-only" if self.stage_name == "nightflaid" => {
                    let color = parse_hex_color(&v1);
                    let dur: f32 = v2.parse().unwrap_or(1.0);
                    self.nightflaid_color_tween_right(color, dur);
                }
                "Nightflaid swap sides" if self.stage_name == "nightflaid" => {
                    let dur: f32 = v1.parse().unwrap_or(0.15);
                    let old_left = self.nightflaid.stage_color_left;
                    let old_right = self.nightflaid.stage_color_right;
                    self.nightflaid_color_tween_left(old_right, dur);
                    self.nightflaid_color_tween_right(old_left, dur);
                }
                "Nightflaid lightings" if self.stage_name == "nightflaid" => {
                    let on = matches!(v1.to_lowercase().as_str(), "on" | "1" | "");
                    self.nightflaid.lights_on = on;
                }
                "NINTENDO" => {
                    // VS Retrospecter custom event: triggers 80sNightflaid phase
                    if v2 == "80snightflaid" && self.stage_name == "nightflaid" {
                        log::info!("80sNightflaid phase activated!");
                        self.nightflaid_activate_pending = true;
                    }
                }
                "Wildcard" => {
                    // VS Retrospecter custom event: calls Lua function by name.
                    // v1 = function name, v2 = argument
                    if self.scripts.has_scripts() {
                        self.scripts.call_lua_function(&v1, &v2);
                        self.process_property_writes();
                    }
                    // setOppAnimation: also set the opponent's animation suffix
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
                        // Color-tween custom health bar per opponent form
                        if let Some(chb) = &mut self.custom_healthbar {
                            let (left, right, health_reset, dur) = match v2.to_uppercase().as_str() {
                                "DAD" => (srgb_color(1.0, 0.004, 0.02), Some(srgb_color(0.39, 1.0, 0.23)), Some(1.0), 1.0),
                                "WHITTY" => (srgb_color(0.81, 0.004, 0.17), Some(chb.saved_player_color), Some(1.0), 1.0),
                                "RUV" => (srgb_color(0.59, 0.55, 0.64), None, Some(1.0), 1.0),
                                "GARCELLO" => (srgb_color(0.004, 1.0, 0.58), None, Some(1.0), 1.0),
                                "TABI" => (srgb_color(0.36, 0.42, 0.51), None, Some(2.0), 1.0),
                                "TRICKY" => (srgb_color(0.99, 0.098, 0.016), None, Some(1.25), 1.0),
                                "SHAGGY" => (srgb_color(0.83, 0.106, 0.114), None, Some(1.0), 1.0),
                                "SONIC" => (srgb_color(0.0, 0.345, 0.71), None, Some(1.0), 1.0),
                                "POKEMON" => (srgb_color(0.49, 0.36, 0.56), None, Some(1.0), 1.0),
                                "NINTENDO" => (srgb_color(0.65, 0.84, 0.96), None, Some(1.0), 1.0),
                                "PRECUT" => ([1.0, 1.0, 1.0, 1.0], None, None, 0.5),
                                _ => (chb.left_color, None, None, 1.0),
                            };
                            chb.tween_colors(left, right, dur);
                            if let Some(h) = health_reset {
                                self.game.score.health = h;
                            }
                        }
                    }
                    // returner: deactivate 80sNightflaid
                    if v1 == "returner" && self.stage_name == "nightflaid" {
                        log::info!("80sNightflaid phase deactivated (returner)");
                        self.nightflaid_deactivate_pending = true;
                    }
                    // preMidsongCutscene: slide out lightning + fade BG
                    if v1 == "preMidsongCutscene" && self.stage_name == "nightflaid" {
                        // GF slides down, unbeatableBG fades out, lightning slides out
                        self.nightflaid.gf_visible_80s = false;
                        self.nightflaid.bg_alpha = 0.0;
                    }
                }
                _ => {}
            }
        }

        // === Process gameplay events ===
        let events = self.game.drain_events();
        for event in events {
            match event {
                GameEvent::NoteHit { lane, rating, combo, note_type, is_sustain, members_index, .. } => {
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
                    // Lua: goodNoteHit(membersIndex, noteData, noteType, isSustainNote)
                    if self.scripts.has_scripts() {
                        self.scripts.call_note_hit("goodNoteHit", members_index, lane, &note_type, is_sustain);
                    }
                }
                GameEvent::NoteMiss { lane, note_type, members_index } => {
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
                    // Lua: noteMiss(membersIndex, noteData, noteType, isSustainNote)
                    if self.scripts.has_scripts() {
                        self.scripts.call_note_hit("noteMiss", members_index, lane, &note_type, false);
                    }
                }
                GameEvent::OpponentNoteHit { lane, note_type, is_sustain, members_index } => {
                    if !self.disable_zooming {
                        self.cam_zooming = true;
                    }
                    if let Some(dad) = &mut self.char_dad {
                        dad.play_sing(lane);
                    }
                    // Lua: opponentNoteHit(membersIndex, noteData, noteType, isSustainNote)
                    if self.scripts.has_scripts() {
                        self.scripts.call_note_hit("opponentNoteHit", members_index, lane, &note_type, is_sustain);
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
                    // Nightflaid step-based stage state changes
                    if self.stage_name == "nightflaid" {
                        match step {
                            1664 => self.nightflaid.side_swap_active = true,
                            2304 => {
                                // Both sides to dark
                                self.nightflaid_color_tween_both(self.nightflaid.dark_color, 0.3);
                                self.nightflaid.side_swap_active = false;
                            }
                            2432 => {
                                // Left to song color, re-enable side swaps
                                self.nightflaid_color_tween_left(self.nightflaid.song_color, 0.3);
                                self.nightflaid.side_swap_active = true;
                            }
                            2944 => {
                                self.nightflaid.side_swap_active = false;
                                self.nightflaid.lights_on = true;
                            }
                            3456 => {
                                self.nightflaid.lights_on = false;
                                self.nightflaid_color_tween_both(self.nightflaid.dark_color, 0.3);
                            }
                            _ => {}
                        }
                    }
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
                        let freq = if gf.has_dance_idle() { 1 } else { 2 };
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
                    // Custom health bar fade-in at beat 16
                    if beat == 16 {
                        if let Some(chb) = &mut self.custom_healthbar {
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
                    // Nightflaid: side-based color swaps (onMoveCamera)
                    if self.stage_name == "nightflaid" && self.nightflaid.side_swap_active {
                        let song_c = self.nightflaid.song_color;
                        let dark_c = self.nightflaid.dark_color;
                        if must_hit {
                            // BF singing: right side gets song color, left goes dark
                            self.nightflaid_color_tween_right(song_c, 0.3);
                            self.nightflaid_color_tween_left(dark_c, 0.3);
                        } else {
                            // Dad singing: left side gets song color, right goes dark
                            self.nightflaid_color_tween_left(song_c, 0.3);
                            self.nightflaid_color_tween_right(dark_c, 0.3);
                        }
                        // Also swap the existing colors
                        let old_left = self.nightflaid.stage_color_left;
                        let old_right = self.nightflaid.stage_color_right;
                        self.nightflaid.color_left_start = old_left;
                        self.nightflaid.color_right_start = old_right;
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

        // Process game-level property writes from Lua scripts
        self.process_property_writes();

        // Process moveCameraSection requests from scripts (runHaxeCode / moveCameraSection)
        let cam_sections: Vec<i32> = self.scripts.state.camera_section_requests.drain(..).collect();
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
        let cam_targets: Vec<String> = self.scripts.state.camera_target_requests.drain(..).collect();
        for target in cam_targets {
            match target.trim().to_lowercase().as_str() {
                "dad" | "opponent" => {
                    self.recompute_camera_targets();
                    self.camera.follow(self.cam_dad[0], self.cam_dad[1]);
                }
                "gf" | "girlfriend" => {
                    // GF camera: use GF midpoint if available, otherwise dad position
                    if let Some(gf) = &self.char_gf {
                        let (mx, my) = gf.midpoint();
                        self.camera.follow(mx, my);
                    }
                }
                _ => {
                    // Default = boyfriend
                    self.recompute_camera_targets();
                    self.camera.follow(self.cam_bf[0], self.cam_bf[1]);
                }
            }
        }

        // Process triggered events from Lua (triggerEvent)
        let events: Vec<(String, String, String)> = self.scripts.state.triggered_events.drain(..).collect();
        for (name, v1, v2) in events {
            match name.as_str() {
                "Add Camera Zoom" => {
                    let game_zoom: f32 = v1.parse().unwrap_or(0.015);
                    let hud_zoom: f32 = v2.parse().unwrap_or(0.03);
                    if !self.disable_zooming && self.camera.zoom < 1.35 {
                        self.camera.zoom += game_zoom;
                        self.hud_zoom += hud_zoom;
                    }
                }
                _ => {
                    log::debug!("Unhandled triggerEvent: {} ({}, {})", name, v1, v2);
                }
            }
        }

        // Process camera shake requests
        for (camera, intensity, duration) in self.scripts.state.camera_shake_requests.drain(..) {
            log::debug!("Camera shake: {} intensity={} duration={}", camera, intensity, duration);
            if camera == "camGame" {
                self.camera.start_shake(intensity, duration);
            }
            // camHUD shake would need HUD shake state — log for now
        }

        // Process camera flash requests
        for (camera, color, duration, alpha) in self.scripts.state.camera_flash_requests.drain(..) {
            log::debug!("Camera flash: {} color={} duration={} alpha={}", camera, color, duration, alpha);
            if camera == "camGame" {
                self.camera.start_flash(&color, duration, alpha);
            }
        }

        // Process subtitle requests (display as log for now; rendering handled by text system)
        for (text, _font, _color, _size, _duration, _border) in self.scripts.state.subtitle_requests.drain(..) {
            if !text.trim().is_empty() {
                log::info!("[Subtitle] {}", text);
            }
        }

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
