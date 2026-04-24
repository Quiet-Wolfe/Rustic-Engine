use rustic_core::note::NoteData;
use rustic_core::rating;
use rustic_render::gpu::{GpuState, GpuTexture};

use crate::screens::video::VideoPlayer;

use super::{
    DrawLayer, NoteAssets, PlayScreen, CONFIRM_ANIMS, GAME_H, GAME_W, HEALTH_BAR_H, HEALTH_BAR_W,
    HEALTH_BAR_X, HEALTH_BAR_Y, HOLD_END_ANIMS, HOLD_PIECE_ANIMS, NOTE_ANIMS, NOTE_SCALE,
    NOTE_WIDTH, PRESS_ANIMS, RATING_SCALE, SPLASH_PREFIXES, STRUM_ANIMS,
};

/// Compute RGBA color for a note with color transform offsets applied.
fn note_color(nd: &NoteData, alpha: f32) -> [f32; 4] {
    if nd.color_r_offset == 0.0 && nd.color_g_offset == 0.0 && nd.color_b_offset == 0.0 {
        [alpha, alpha, alpha, alpha]
    } else {
        // Color offsets are -255..255 range, normalize to 0..1
        let r = (1.0 + nd.color_r_offset / 255.0).clamp(0.0, 1.0) * alpha;
        let g = (1.0 + nd.color_g_offset / 255.0).clamp(0.0, 1.0) * alpha;
        let b = (1.0 + nd.color_b_offset / 255.0).clamp(0.0, 1.0) * alpha;
        [r, g, b, alpha]
    }
}

fn lua_sprite_color(sprite: &rustic_scripting::LuaSprite) -> [f32; 4] {
    let alpha = sprite.alpha.clamp(0.0, 1.0);
    let channel =
        |base: u8, offset: f32| ((base as f32 + offset).clamp(0.0, 255.0) / 255.0) * alpha;
    [
        channel(sprite.color[0], sprite.color_red_offset),
        channel(sprite.color[1], sprite.color_green_offset),
        channel(sprite.color[2], sprite.color_blue_offset),
        alpha,
    ]
}

fn lua_camera_is_game(camera: &str) -> bool {
    let camera = camera.trim();
    camera.is_empty()
        || camera.eq_ignore_ascii_case("camGame")
        || camera.eq_ignore_ascii_case("game")
}

fn health_icon_frame(icon: &GpuTexture, losing: bool) -> (f32, f32, f32) {
    let height = icon.height.max(1) as f32;
    let frames = ((icon.width as f32 / height).round() as usize).max(1);
    let frame_w = icon.width as f32 / frames as f32;
    let src_x = if losing && frames > 1 { frame_w } else { 0.0 };
    (src_x, frame_w, height)
}

fn draw_stage_color_overlay(gpu: &mut GpuState, left: [f32; 4], right: [f32; 4]) {
    let has_left = left[3] > 0.001;
    let has_right = right[3] > 0.001;
    if !has_left && !has_right {
        return;
    }

    if left != right {
        if has_left {
            let c = [left[0], left[1], left[2], left[3] * 0.35];
            gpu.push_colored_quad(0.0, 0.0, GAME_W / 2.0, GAME_H, c);
            gpu.draw_batch(None);
        }
        if has_right {
            let c = [right[0], right[1], right[2], right[3] * 0.35];
            gpu.push_colored_quad(GAME_W / 2.0, 0.0, GAME_W / 2.0, GAME_H, c);
            gpu.draw_batch(None);
        }
    } else {
        let c = [left[0], left[1], left[2], left[3] * 0.35];
        gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, c);
        gpu.draw_batch(None);
    }
}

impl PlayScreen {
    pub(super) fn draw_inner(&mut self, gpu: &mut GpuState) {
        let white = [1.0, 1.0, 1.0, 1.0];

        if !gpu.begin_frame() {
            return;
        }

        self.process_precache_requests(gpu);

        // Process video playback requests from Lua (needs GPU for texture creation)
        let video_reqs: Vec<_> = self.scripts.state.video_requests.drain(..).collect();
        for (filename, callback, blocks_gameplay) in video_reqs {
            if self.cutscene.is_some() {
                break;
            }
            let video_path = self.paths.video(&filename);
            if let Some(path) = video_path {
                match VideoPlayer::new(&path, &gpu.device, &gpu.texture_layout, &gpu.sampler) {
                    Ok(player) => {
                        log::info!("Starting video: {:?} (blocking={})", path, blocks_gameplay);
                        let mut player = player;
                        if let Some(cb) = callback {
                            player.set_on_finish(cb);
                        }
                        // Mid-song (non-blocking) videos aren't user-skippable —
                        // they're tied to chart timing.
                        self.start_video_cutscene(player, blocks_gameplay, blocks_gameplay);
                    }
                    Err(e) => {
                        log::warn!("Failed to load video '{}': {}", filename, e);
                    }
                }
            } else {
                log::warn!("Video file not found: {}", filename);
            }
        }

        // === Video playback: upload frame (drawn as overlay later) ===
        if let Some(super::CutsceneState::Video { player: video, .. }) = &mut self.cutscene {
            video.upload(&gpu.queue);
        }

        // Process any pending Lua sprite adds (may have been queued during update)
        self.process_lua_characters(gpu);
        self.process_lua_sprites(gpu);

        // Load any pending custom note skins (queued from Lua registerNoteType)
        if !self.pending_note_skin_loads.is_empty() {
            let skin_loads: Vec<_> = self.pending_note_skin_loads.drain(..).collect();
            for (type_name, skin_path, note_anims, strum_anims, confirm_anims) in skin_loads {
                if self.custom_note_assets.contains_key(&type_name) {
                    continue;
                }
                if let Some(loaded) = self.load_note_skin(
                    gpu,
                    &self.paths,
                    &skin_path,
                    note_anims.as_ref(),
                    strum_anims.as_ref(),
                    confirm_anims.as_ref(),
                ) {
                    log::info!(
                        "Loaded custom note skin '{}' for type '{}'",
                        skin_path,
                        type_name
                    );
                    self.custom_note_assets.insert(type_name, loaded);
                } else {
                    log::warn!(
                        "Failed to load note skin '{}' for type '{}'",
                        skin_path,
                        type_name
                    );
                }
            }
        }

        // Process pending character changes (needs GPU for texture loading)
        self.process_char_changes(gpu);

        // Process post-processing requests from Lua (needs gpu)
        {
            let pp_reqs: Vec<_> = self.scripts.state.postprocess_requests.drain(..).collect();
            for (enabled, dur) in pp_reqs {
                if enabled {
                    gpu.set_postprocess_active(true);
                    gpu.postprocess.uniforms.enabled = 1;
                } else if dur <= 0.0 {
                    gpu.postprocess.uniforms.enabled = 0;
                    gpu.set_postprocess_active(false);
                } else {
                    // Fade out over duration — for now just disable immediately
                    // TODO: tween fade-out
                    gpu.postprocess.uniforms.enabled = 0;
                    gpu.set_postprocess_active(false);
                }
            }
            let pp_params: Vec<_> = self
                .scripts
                .state
                .postprocess_param_requests
                .drain(..)
                .collect();
            for (param, value) in pp_params {
                match param.as_str() {
                    "scanline" => gpu.postprocess.uniforms.scanline_intensity = value,
                    "distortion" => gpu.postprocess.uniforms.distortion_mult = value,
                    "chromatic" => gpu.postprocess.uniforms.chromatic_aberration = value,
                    "vignette" => gpu.postprocess.uniforms.vignette_intensity = value,
                    "time" => gpu.postprocess.uniforms.time = value,
                    "enabled" => {
                        if value > 0.5 {
                            gpu.postprocess.uniforms.enabled = 1;
                            gpu.set_postprocess_active(true);
                        } else {
                            gpu.postprocess.uniforms.enabled = 0;
                            gpu.set_postprocess_active(false);
                        }
                    }
                    _ => log::warn!("Unknown postprocess param: {}", param),
                }
            }
        }

        let is_death = self.death.is_some();

        // === Stage & Characters (draw order from stage objects or Lua scripts) ===
        for layer in &self.draw_order {
            match layer {
                DrawLayer::StageBg(i) => {
                    let bg = &self.stage_bg[*i];
                    bg.draw(gpu, &self.camera);
                    gpu.draw_batch(Some(&bg.texture));
                }
                DrawLayer::Gf => {
                    if !is_death {
                        if let Some(gf) = &self.char_gf {
                            gf.draw(gpu, &self.camera);
                            gpu.draw_batch(Some(gf.texture()));
                        }
                    }
                }
                DrawLayer::Dad => {
                    if !is_death {
                        if let Some(dad) = &self.char_dad {
                            // Draw reflection below character first
                            if self.reflections_enabled {
                                dad.draw_reflection(
                                    gpu,
                                    &self.camera,
                                    self.reflection_alpha,
                                    self.reflection_dist_y,
                                );
                                gpu.draw_batch(Some(dad.texture()));
                            }
                            dad.draw(gpu, &self.camera);
                            gpu.draw_batch(Some(dad.texture()));
                        }
                    }
                }
                DrawLayer::Bf => {
                    if let Some(death) = &self.death {
                        death.character.draw(gpu, &self.camera);
                        gpu.draw_batch(Some(death.character.texture()));
                    } else if let Some(bf) = &self.char_bf {
                        // Draw reflection below character first
                        if self.reflections_enabled {
                            bf.draw_reflection(
                                gpu,
                                &self.camera,
                                self.reflection_alpha,
                                self.reflection_dist_y,
                            );
                            gpu.draw_batch(Some(bf.texture()));
                        }
                        bf.draw(gpu, &self.camera);
                        gpu.draw_batch(Some(bf.texture()));
                    }
                }
                DrawLayer::LuaSprite(tag) => {
                    if !is_death {
                        self.draw_single_lua_sprite(gpu, tag);
                    }
                }
                DrawLayer::LuaCharacter(tag) => {
                    if !is_death {
                        if let Some(instance) = self.lua_characters.get(tag) {
                            if instance.visible {
                                instance.character.draw(gpu, &self.camera);
                                gpu.draw_batch(Some(instance.character.texture()));
                            }
                        }
                    }
                }
            }
        }

        // === Stage color overlay ===
        // Draw after game-camera layers so the tint is visible, but before
        // notes/HUD so gameplay UI remains readable.
        if !is_death {
            draw_stage_color_overlay(
                gpu,
                self.stage_overlay.color_left,
                self.stage_overlay.color_right,
            );
        }

        // === Mid-song (non-blocking) video overlay: above stage/characters, below HUD ===
        if let Some(super::CutsceneState::Video {
            player: video,
            blocks_gameplay: false,
            ..
        }) = &mut self.cutscene
        {
            let (vw, vh) = video.dimensions();
            let scale = (GAME_W / vw as f32).min(GAME_H / vh as f32);
            let dw = vw as f32 * scale;
            let dh = vh as f32 * scale;
            let dx = (GAME_W - dw) / 2.0;
            let dy = (GAME_H - dh) / 2.0;
            gpu.push_texture_region(
                vw as f32, vh as f32, 0.0, 0.0, vw as f32, vh as f32, dx, dy, dw, dh, false, white,
            );
            gpu.draw_batch_with_bind_group(video.bind_group());
        }

        // Skip HUD during death
        if self.death.is_some() {
            self.draw_death_overlay(gpu);
            crate::debug_overlay::finish_frame(gpu);
            return;
        }

        // === HUD zoom transform ===
        let hz = self.hud_zoom;
        let hud_x = |x: f32| -> f32 { GAME_W / 2.0 + (x - GAME_W / 2.0) * hz };
        let hud_y = |y: f32| -> f32 { GAME_H / 2.0 + (y - GAME_H / 2.0) * hz };
        let hud_s = |s: f32| -> f32 { s * hz };

        // === Note batch: held tails → strums → unheld tails → note heads ===

        // Compute note Y positions for rendering — use actual strum Y (modcharts move them)
        // Per-note `is_reversing_scroll` flips direction relative to the strum's downscroll setting.
        let note_positions: Vec<(usize, f32)> = (0..self.game.notes.len())
            .map(|i| {
                let nd = &self.game.notes[i];
                let (_sx, sy, _sa, _ang, _sc) = self.strum_pos(nd.lane, nd.must_press);
                let ds = self.is_strum_downscroll(nd.lane, nd.must_press) ^ nd.is_reversing_scroll;
                (i, self.game.note_y(nd.strum_time, sy, ds))
            })
            .collect();

        // === Note batches (ordered by play_as_opponent for z-index priority) ===
        let draw_order = if self.game.play_as_opponent {
            [true, false] // player behind, opponent front
        } else {
            [false, true] // opponent behind, player front
        };

        for &drawing_player in &draw_order {
            for &(i, y_pos) in &note_positions {
                let nd = &self.game.notes[i];
                if nd.must_press != drawing_player {
                    continue;
                }
                if nd.sustain_length > 0.0 && !nd.too_late && self.is_hold_active(nd) {
                    self.draw_hold_tail(gpu, nd, y_pos);
                }
            }
            for lane in 0..4 {
                self.draw_strum(gpu, lane, drawing_player);
            }
            for &(i, y_pos) in &note_positions {
                let nd = &self.game.notes[i];
                if nd.must_press != drawing_player {
                    continue;
                }
                if nd.sustain_length > 0.0 && !nd.too_late && !self.is_hold_active(nd) {
                    self.draw_hold_tail(gpu, nd, y_pos);
                }
            }
            for &(i, y_pos) in &note_positions {
                let nd = &self.game.notes[i];
                if nd.must_press != drawing_player || nd.was_good_hit || nd.too_late {
                    continue;
                }
                if !nd.visible {
                    continue;
                }
                if y_pos < -NOTE_WIDTH || y_pos > GAME_H + NOTE_WIDTH {
                    continue;
                }
                let (sx, _sy, sa, ang, _sc) = self.strum_pos(nd.lane, drawing_player);
                let a = (sa * nd.alpha).clamp(0.0, 1.0);
                if a <= 0.0 {
                    continue;
                }
                let note_ang = ang + nd.angle;
                let note_scale = NOTE_SCALE * (nd.scale_x / 0.7); // scale relative to default 0.7
                let (assets, anim, tint) = self.resolve_note_assets(nd, drawing_player);
                let uses_custom_tex = self.note_uses_custom_texture(nd);

                if let (Some(assets), Some(anim)) = (assets, anim) {
                    // If switching to a custom texture, flush default batch first
                    if uses_custom_tex {
                        let def_tex = if drawing_player {
                            self.note_assets.as_ref()
                        } else {
                            self.opp_note_assets.as_ref().or(self.note_assets.as_ref())
                        };
                        if let Some(def) = def_tex {
                            gpu.draw_batch(Some(&def.texture));
                        }
                    }
                    // Center note on lane midpoint (same as strum centering)
                    let ref_size = NOTE_WIDTH / NOTE_SCALE;
                    let cx = sx + nd.offset_x + ref_size * NOTE_SCALE / 2.0;
                    let cy = y_pos + nd.offset_y + ref_size * NOTE_SCALE / 2.0;
                    let base = note_color(nd, a);
                    let color = [
                        base[0] * tint[0],
                        base[1] * tint[1],
                        base[2] * tint[2],
                        base[3] * tint[3],
                    ];
                    if let Some(frame) = assets.atlas.get_frame(&anim, 0) {
                        let draw_x = cx - frame.frame_w * note_scale / 2.0;
                        let draw_y = cy - frame.frame_h * note_scale / 2.0;
                        if note_ang.abs() > 0.01 {
                            gpu.draw_sprite_frame_rotated(
                                frame,
                                assets.tex_w,
                                assets.tex_h,
                                draw_x,
                                draw_y,
                                note_scale,
                                false,
                                note_ang,
                                color,
                            );
                        } else {
                            gpu.draw_sprite_frame(
                                frame,
                                assets.tex_w,
                                assets.tex_h,
                                draw_x,
                                draw_y,
                                note_scale,
                                false,
                                color,
                            );
                        }
                    }
                    // Flush custom-texture notes immediately
                    if uses_custom_tex {
                        gpu.draw_batch(Some(&assets.texture));
                    }
                }
            }
            // Flush remaining default batch
            let tex_to_flush = if drawing_player {
                self.note_assets.as_ref()
            } else {
                self.opp_note_assets.as_ref().or(self.note_assets.as_ref())
            };
            if let Some(assets) = tex_to_flush {
                gpu.draw_batch(Some(&assets.texture));
            }
        }

        // Note splashes (separate batch)
        if let Some(splash) = &self.splash_atlas {
            for s in &self.splashes {
                let (sx, sy, _sa, _ang, _sc) = self.strum_pos(s.lane, s.player);
                let anim = SPLASH_PREFIXES[s.lane];
                if let Some(frame) = splash.atlas.get_frame(anim, s.frame) {
                    let scale = NOTE_SCALE * 1.3;
                    let cx = hud_x(sx + NOTE_WIDTH / 2.0) - frame.frame_w * scale / 2.0;
                    let cy = hud_y(sy + NOTE_WIDTH / 2.0) - frame.frame_h * scale / 2.0;
                    gpu.draw_sprite_frame(
                        frame,
                        splash.tex_w,
                        splash.tex_h,
                        cx,
                        cy,
                        scale,
                        false,
                        [1.0, 1.0, 1.0, 0.6],
                    );
                }
            }
            if !self.splashes.is_empty() {
                gpu.draw_batch(Some(&splash.texture));
            }
        }

        // === Health bar ===
        let health_pct = self.game.score.health_percent();
        let hud_prop = |name: &str, default: f32| -> f32 {
            match self.scripts.state.custom_vars.get(name) {
                Some(rustic_scripting::LuaValue::Float(v)) => *v as f32,
                Some(rustic_scripting::LuaValue::Int(v)) => *v as f32,
                Some(rustic_scripting::LuaValue::String(v)) => v.parse::<f32>().unwrap_or(default),
                _ => default,
            }
        };

        if let Some(chb) = &self.custom_healthbar {
            if chb.alpha > 0.001 {
                // Custom health bar: overlay + clipped bar sprites
                let a = chb.alpha;
                let scale = chb.scale * hz;
                let ow = chb.overlay_texture.width as f32 * scale;
                let oh = chb.overlay_texture.height as f32 * scale;
                let bw = chb.bar_texture.width as f32 * scale;
                let bh = chb.bar_texture.height as f32 * scale;

                let base_overlay_x = (GAME_W - chb.overlay_texture.width as f32 * chb.scale) / 2.0;
                let base_overlay_y =
                    HEALTH_BAR_Y - chb.overlay_texture.height as f32 * chb.scale / 2.0;
                let base_bar_x = base_overlay_x
                    + (chb.overlay_texture.width as f32 * chb.scale
                        - chb.bar_texture.width as f32 * chb.scale)
                        / 2.0;
                let base_bar_y = base_overlay_y
                    + (chb.overlay_texture.height as f32 * chb.scale
                        - chb.bar_texture.height as f32 * chb.scale)
                        / 2.0;
                let overlay_x = hud_x(hud_prop("bar.overlay.x", base_overlay_x));
                let overlay_y = hud_y(hud_prop("bar.overlay.y", base_overlay_y));
                let left_bar_x = hud_x(hud_prop("bar.leftBar.x", base_bar_x));
                let left_bar_y = hud_y(hud_prop("bar.leftBar.y", base_bar_y));
                let right_bar_x = hud_x(hud_prop("bar.rightBar.x", base_bar_x));
                let right_bar_y = hud_y(hud_prop("bar.rightBar.y", base_bar_y));

                // Smoothed health for fill (capped at 1.7/2 = 0.85)
                let vida = (chb.health_lerp.clamp(0.0, 1.7)) / 2.0;
                let bar_src_w = chb.bar_texture.width as f32;
                let bar_src_h = chb.bar_texture.height as f32;

                // Opponent bar (left side): clips from x=0, width = (1-vida) of bar
                let opp_clip_w = bar_src_w * (1.0 - vida);
                let opp_color = [
                    chb.left_color[0] * a,
                    chb.left_color[1] * a,
                    chb.left_color[2] * a,
                    a,
                ];
                gpu.push_texture_region(
                    bar_src_w,
                    bar_src_h,
                    0.0,
                    0.0,
                    opp_clip_w,
                    bar_src_h,
                    left_bar_x,
                    left_bar_y,
                    opp_clip_w * scale,
                    bh,
                    false,
                    opp_color,
                );
                gpu.draw_batch(Some(&chb.bar_texture));

                // Player bar (right side): clips from right edge, width = vida of bar
                let player_clip_w = bar_src_w * vida;
                let player_src_x = bar_src_w - player_clip_w;
                let player_color = [
                    chb.right_color[0] * a,
                    chb.right_color[1] * a,
                    chb.right_color[2] * a,
                    a,
                ];
                gpu.push_texture_region(
                    bar_src_w,
                    bar_src_h,
                    player_src_x,
                    0.0,
                    player_clip_w,
                    bar_src_h,
                    right_bar_x + bw - player_clip_w * scale,
                    right_bar_y,
                    player_clip_w * scale,
                    bh,
                    false,
                    player_color,
                );
                gpu.draw_batch(Some(&chb.bar_texture));

                // Overlay on top
                let overlay_color = [a, a, a, a];
                gpu.push_texture_region(
                    chb.overlay_texture.width as f32,
                    chb.overlay_texture.height as f32,
                    0.0,
                    0.0,
                    chb.overlay_texture.width as f32,
                    chb.overlay_texture.height as f32,
                    overlay_x,
                    overlay_y,
                    ow,
                    oh,
                    false,
                    overlay_color,
                );
                gpu.draw_batch(Some(&chb.overlay_texture));

                // Cut point for icons
                let cut_x = right_bar_x + bw * (1.0 - vida);

                if self.game.song_ended {
                    gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.0, 0.6]);
                    gpu.draw_batch(None);
                }

                // Icons at cut point
                let bf_losing = health_pct < 0.2;
                let dad_losing = health_pct > 0.8;
                let icon_size = 150.0 * 0.75;
                let icon_spacing = -20.0 * scale;
                let icon_alpha_bf = hud_prop("iconP1.alpha", 1.0).clamp(0.0, 1.0) * a;
                let icon_alpha_dad = hud_prop("iconP2.alpha", 1.0).clamp(0.0, 1.0) * a;

                if let Some(icon) = &self.icon_dad {
                    let s = self.icon_scale_dad;
                    let draw_size = hud_s(icon_size) * s;
                    let default_icon_y = base_overlay_y
                        + chb.overlay_texture.height as f32 * chb.scale / 2.0
                        - icon_size / 2.0;
                    let icon_y = hud_y(hud_prop("iconP2.y", default_icon_y));
                    let icon_x = hud_x(hud_prop("iconP2.x", cut_x - draw_size + icon_spacing));
                    let (src_x, src_w, src_h) = health_icon_frame(icon, dad_losing);
                    gpu.push_texture_region(
                        icon.width as f32,
                        icon.height as f32,
                        src_x,
                        0.0,
                        src_w,
                        src_h,
                        icon_x,
                        icon_y,
                        draw_size,
                        draw_size,
                        false,
                        [
                            icon_alpha_dad,
                            icon_alpha_dad,
                            icon_alpha_dad,
                            icon_alpha_dad,
                        ],
                    );
                    gpu.draw_batch(Some(icon));
                }

                if let Some(icon) = &self.icon_bf {
                    let s = self.icon_scale_bf;
                    let draw_size = hud_s(icon_size) * s;
                    let default_icon_y = base_overlay_y
                        + chb.overlay_texture.height as f32 * chb.scale / 2.0
                        - icon_size / 2.0;
                    let icon_y = hud_y(hud_prop("iconP1.y", default_icon_y));
                    let icon_x = hud_x(hud_prop("iconP1.x", cut_x - icon_spacing));
                    let (src_x, src_w, src_h) = health_icon_frame(icon, bf_losing);
                    gpu.push_texture_region(
                        icon.width as f32,
                        icon.height as f32,
                        src_x,
                        0.0,
                        src_w,
                        src_h,
                        icon_x,
                        icon_y,
                        draw_size,
                        draw_size,
                        true,
                        [icon_alpha_bf, icon_alpha_bf, icon_alpha_bf, icon_alpha_bf],
                    );
                    gpu.draw_batch(Some(icon));
                }

                // Score text below overlay
                let grade = self.game.score.grade();
                let score_text = format!(
                    "Score: {} | Misses: {} | Acc: {:.2}% [{}]",
                    self.game.score.score,
                    self.game.score.misses,
                    self.game.score.accuracy(),
                    grade,
                );
                gpu.draw_text(
                    &score_text,
                    overlay_x,
                    overlay_y + oh + 6.0,
                    16.0,
                    [a, a, a, a],
                );
            }
        } else {
            // Default health bar: always at the bottom so it never covers the note path in downscroll.
            let hbx = hud_x(HEALTH_BAR_X);
            let hby = hud_y(HEALTH_BAR_Y);
            let hbw = hud_s(HEALTH_BAR_W);
            let hbh = hud_s(HEALTH_BAR_H);
            // Black border
            gpu.push_colored_quad(
                hbx - 2.0,
                hby - 2.0,
                hbw + 4.0,
                hbh + 4.0,
                [0.0, 0.0, 0.0, 1.0],
            );
            gpu.draw_batch(None);
            // Colored fills: opponent on left, player on right
            let (left_color, right_color) = if self.game.play_as_opponent {
                (self.hb_color_bf, self.hb_color_dad)
            } else {
                (self.hb_color_dad, self.hb_color_bf)
            };
            gpu.push_colored_quad(hbx, hby, hbw, hbh, left_color);
            let player_w = hbw * health_pct;
            gpu.push_colored_quad(hbx + hbw - player_w, hby, player_w, hbh, right_color);
            gpu.draw_batch(None);

            if self.game.song_ended {
                gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.0, 0.6]);
                gpu.draw_batch(None);
            }

            // Icons with bop
            let divider_x = hbx + hbw * (1.0 - health_pct);
            let (bf_losing, dad_losing) = if self.game.play_as_opponent {
                (health_pct > 0.8, health_pct < 0.2)
            } else {
                (health_pct < 0.2, health_pct > 0.8)
            };
            let icon_size = 150.0 * 0.75;
            let white = [1.0, 1.0, 1.0, 1.0];

            let draw_left_icon =
                |gpu: &mut GpuState, icon: &GpuTexture, scale: f32, losing: bool| {
                    let draw_size = hud_s(icon_size) * scale;
                    let icon_y = hby + hbh / 2.0 - draw_size / 2.0;
                    let (src_x, src_w, src_h) = health_icon_frame(icon, losing);
                    gpu.push_texture_region(
                        icon.width as f32,
                        icon.height as f32,
                        src_x,
                        0.0,
                        src_w,
                        src_h,
                        divider_x - draw_size + 15.0,
                        icon_y,
                        draw_size,
                        draw_size,
                        false,
                        white,
                    );
                    gpu.draw_batch(Some(icon));
                };

            let draw_right_icon =
                |gpu: &mut GpuState, icon: &GpuTexture, scale: f32, losing: bool| {
                    let draw_size = hud_s(icon_size) * scale;
                    let icon_y = hby + hbh / 2.0 - draw_size / 2.0;
                    let (src_x, src_w, src_h) = health_icon_frame(icon, losing);
                    gpu.push_texture_region(
                        icon.width as f32,
                        icon.height as f32,
                        src_x,
                        0.0,
                        src_w,
                        src_h,
                        divider_x - 15.0,
                        icon_y,
                        draw_size,
                        draw_size,
                        true,
                        white,
                    );
                    gpu.draw_batch(Some(icon));
                };

            if self.game.play_as_opponent {
                if let Some(icon) = &self.icon_bf {
                    draw_left_icon(gpu, icon, self.icon_scale_bf, bf_losing);
                }
                if let Some(icon) = &self.icon_dad {
                    draw_right_icon(gpu, icon, self.icon_scale_dad, dad_losing);
                }
            } else {
                if let Some(icon) = &self.icon_dad {
                    draw_left_icon(gpu, icon, self.icon_scale_dad, dad_losing);
                }
                if let Some(icon) = &self.icon_bf {
                    draw_right_icon(gpu, icon, self.icon_scale_bf, bf_losing);
                }
            }

            // Score text
            let grade = self.game.score.grade();
            let score_text = format!(
                "Score: {} | Misses: {} | Acc: {:.2}% [{}]",
                self.game.score.score,
                self.game.score.misses,
                self.game.score.accuracy(),
                grade,
            );
            gpu.draw_text(&score_text, hbx, hby + hbh + 6.0, 16.0, white);

            // Botplay / Practice mode indicators (always near top, out of the note path)
            if self.botplay {
                gpu.draw_text(
                    "BOTPLAY",
                    GAME_W / 2.0 - 50.0,
                    60.0,
                    32.0,
                    [1.0, 1.0, 1.0, 0.6],
                );
            }
            if self.practice_mode {
                gpu.draw_text(
                    "PRACTICE MODE",
                    GAME_W / 2.0 - 80.0,
                    90.0,
                    24.0,
                    [1.0, 1.0, 1.0, 0.5],
                );
            }
        }

        // === Rating popups ===
        if let Some(assets) = &self.rating_assets {
            let placement = GAME_W * 0.35;
            for popup in &self.rating_popups {
                let a = popup.alpha.clamp(0.0, 1.0);
                let color = [a, a, a, a];
                let rating_tex = match popup.rating_name.as_str() {
                    "sick" => Some(&assets.sick),
                    "good" => Some(&assets.good),
                    "bad" => Some(&assets.bad),
                    "shit" => Some(&assets.shit),
                    _ => None,
                };
                let rx = placement - 40.0;
                let ry = GAME_H / 2.0 - 60.0 + popup.y;
                if let Some(tex) = rating_tex {
                    let w = tex.width as f32 * RATING_SCALE;
                    let h = tex.height as f32 * RATING_SCALE;
                    gpu.push_texture_region(
                        tex.width as f32,
                        tex.height as f32,
                        0.0,
                        0.0,
                        tex.width as f32,
                        tex.height as f32,
                        rx,
                        ry,
                        w,
                        h,
                        false,
                        color,
                    );
                    gpu.draw_batch(Some(tex));
                }
                if popup.combo >= 0 {
                    let digits = format!("{:03}", popup.combo);
                    let num_scale = 0.5;
                    let ny = GAME_H / 2.0 + 80.0 + popup.y;
                    for (i, ch) in digits.chars().enumerate() {
                        if let Some(d) = ch.to_digit(10) {
                            let tex = &assets.nums[d as usize];
                            let w = tex.width as f32 * num_scale;
                            let h = tex.height as f32 * num_scale;
                            let nx = placement + 43.0 * i as f32 - 90.0;
                            gpu.push_texture_region(
                                tex.width as f32,
                                tex.height as f32,
                                0.0,
                                0.0,
                                tex.width as f32,
                                tex.height as f32,
                                nx,
                                ny,
                                w,
                                h,
                                false,
                                color,
                            );
                            gpu.draw_batch(Some(tex));
                        }
                    }
                }
            }
        }

        // === Lua sprites on screen-space cameras ===
        if self.hud_visible {
            self.draw_lua_sprites_screen(gpu);
        }

        // === Countdown sprite ===
        if self.hud_visible && self.countdown_alpha > 0.0 {
            let cd_tex = match self.countdown_swag {
                1 => self.countdown_ready.as_ref(),
                2 => self.countdown_set.as_ref(),
                3 => self.countdown_go.as_ref(),
                _ => None,
            };
            if let Some(tex) = cd_tex {
                let a = self.countdown_alpha.clamp(0.0, 1.0);
                let color = [a, a, a, a];
                let w = tex.width as f32;
                let h = tex.height as f32;
                let cx = GAME_W / 2.0 - w / 2.0;
                let cy = GAME_H / 2.0 - h / 2.0;
                gpu.push_texture_region(w, h, 0.0, 0.0, w, h, cx, cy, w, h, false, color);
                gpu.draw_batch(Some(tex));
            }
        }

        if self.pause_menu.is_some() {
            self.draw_pause(gpu);
        }

        // === Song results ===
        if self.game.song_ended && self.pause_menu.is_none() {
            let fc = rating::classify_fc(
                self.game.score.sicks,
                self.game.score.goods,
                self.game.score.bads,
                self.game.score.shits,
                self.game.score.misses,
            );
            let fc_str = match fc {
                rating::FcClassification::Sfc => " [SFC]",
                rating::FcClassification::Gfc => " [GFC]",
                rating::FcClassification::Fc => " [FC]",
                rating::FcClassification::Sdcb => " [SDCB]",
                rating::FcClassification::Clear => "",
            };
            let results = format!(
                "SONG COMPLETE!\n\nScore: {}\nAccuracy: {:.2}%{}\nGrade: {}\n\n\
                 Sicks: {} | Goods: {} | Bads: {} | Shits: {}\nCombo: {} | Misses: {}",
                self.game.score.score,
                self.game.score.accuracy(),
                fc_str,
                self.game.score.grade(),
                self.game.score.sicks,
                self.game.score.goods,
                self.game.score.bads,
                self.game.score.shits,
                self.game.score.max_combo,
                self.game.score.misses,
            );
            gpu.draw_text(&results, GAME_W / 2.0 - 180.0, 200.0, 24.0, white);
        }

        // Touch lane indicators (Android only — subtle FNF-style pads at bottom)
        if cfg!(target_os = "android")
            && self.pause_menu.is_none()
            && self.death.is_none()
            && !self.game.song_ended
        {
            self.draw_touch_ui(gpu);
        }

        // === Video overlay (drawn on top of everything) — only for blocking cutscenes ===
        if let Some(super::CutsceneState::Video {
            player: video,
            skippable,
            blocks_gameplay: true,
            ..
        }) = &mut self.cutscene
        {
            let (vw, vh) = video.dimensions();
            let scale = (GAME_W / vw as f32).min(GAME_H / vh as f32);
            let dw = vw as f32 * scale;
            let dh = vh as f32 * scale;
            let dx = (GAME_W - dw) / 2.0;
            let dy = (GAME_H - dh) / 2.0;
            gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.0, 0.85]);
            gpu.draw_batch(None);
            gpu.push_texture_region(
                vw as f32, vh as f32, 0.0, 0.0, vw as f32, vh as f32, dx, dy, dw, dh, false, white,
            );
            gpu.draw_batch_with_bind_group(video.bind_group());
            if *skippable {
                gpu.draw_text(
                    "Press ENTER to skip",
                    GAME_W - 270.0,
                    GAME_H - 38.0,
                    22.0,
                    white,
                );
            }
        }

        #[cfg(feature = "rl")]
        self.draw_rl_hud(gpu);

        crate::debug_overlay::finish_frame(gpu);
    }

    /// Draw FNF-style semi-transparent arrow touch pads at the bottom of the screen.
    fn draw_touch_ui(&self, gpu: &mut GpuState) {
        let Some(assets) = self.note_assets.as_ref() else {
            return;
        };
        let col_w = GAME_W / 4.0;
        let pad_scale = NOTE_SCALE * 1.1;
        let ref_size = NOTE_WIDTH / NOTE_SCALE; // 160 standard frame size
        let pad_y = GAME_H - ref_size * pad_scale - 10.0;
        let alpha = 0.25;

        for lane in 0..4usize {
            let anim = STRUM_ANIMS[lane];
            if let Some(frame) = assets.atlas.get_frame(&anim, 0) {
                let cx = col_w * lane as f32 + col_w / 2.0;
                let draw_x = cx - frame.frame_w * pad_scale / 2.0;
                let draw_y = pad_y + (ref_size * pad_scale - frame.frame_h * pad_scale) / 2.0;
                let color = [alpha, alpha, alpha, alpha];
                gpu.draw_sprite_frame(
                    frame,
                    assets.tex_w,
                    assets.tex_h,
                    draw_x,
                    draw_y,
                    pad_scale,
                    false,
                    color,
                );
            }
        }
        gpu.draw_batch(Some(&assets.texture));

        // Thin lane dividers
        for lane in 1..4 {
            let x = col_w * lane as f32;
            gpu.push_colored_quad(
                x - 0.5,
                pad_y,
                1.0,
                ref_size * pad_scale,
                [1.0, 1.0, 1.0, 0.06],
            );
        }
        gpu.draw_batch(None);
    }

    /// Draw a single Lua-created sprite (game camera only).
    fn draw_single_lua_sprite(&self, gpu: &mut GpuState, tag: &str) {
        let tex = match self.lua_textures.get(tag) {
            Some(t) => t,
            None => return,
        };
        let sprite = match self.scripts.state.lua_sprites.get(tag) {
            Some(s) => s,
            None => return,
        };
        if !sprite.visible || sprite.alpha <= 0.0 {
            return;
        }

        // Non-game cameras (camHUD, camOther, custom FlxCameras) use screen space.
        if !lua_camera_is_game(&sprite.camera) {
            return;
        }

        let color = lua_sprite_color(sprite);

        let cam = &self.camera;
        let scroll_x = cam.x - GAME_W / 2.0;
        let scroll_y = cam.y - GAME_H / 2.0;
        let zoom = cam.zoom;

        // Animated sprite: render a frame from the atlas. Some Psych scripts add an
        // animation but never explicitly play it; HaxeFlixel still shows a frame,
        // not the whole packed atlas.
        if let Some(atlas) = self.lua_atlases.get(tag) {
            let anim = if sprite.current_anim.is_empty() {
                atlas.anim_names().into_iter().next()
            } else {
                Some(sprite.current_anim.as_str())
            };
            if let Some(anim) = anim {
                if let Some(frame) = atlas.get_frame(anim, sprite.anim_frame) {
                    let (off_x, off_y) =
                        sprite.anim_offsets.get(anim).copied().unwrap_or((0.0, 0.0));
                    let world_x = sprite.x - sprite.offset_x - off_x;
                    let world_y = sprite.y - sprite.offset_y - off_y;
                    let buf_x = world_x - scroll_x * sprite.scroll_x;
                    let buf_y = world_y - scroll_y * sprite.scroll_y;
                    let dx = (buf_x - GAME_W / 2.0) * zoom + GAME_W / 2.0;
                    let dy = (buf_y - GAME_H / 2.0) * zoom + GAME_H / 2.0;
                    let scale = sprite.scale_x * zoom;

                    gpu.draw_sprite_frame(
                        frame,
                        tex.width as f32,
                        tex.height as f32,
                        dx,
                        dy,
                        scale,
                        sprite.flip_x,
                        color,
                    );
                    gpu.draw_batch(Some(tex));
                    return;
                }
            }
            return;
        }

        // Static sprite: draw full texture or HaxeFlixel-style clipRect region.
        let tex_w = tex.width as f32;
        let tex_h = tex.height as f32;
        let sx = sprite.clip_x.min(tex_w).max(0.0);
        let sy = sprite.clip_y.min(tex_h).max(0.0);
        let sw = sprite.clip_w.unwrap_or(tex_w - sx).min(tex_w - sx).max(0.0);
        let sh = sprite.clip_h.unwrap_or(tex_h - sy).min(tex_h - sy).max(0.0);
        if sw <= 0.0 || sh <= 0.0 {
            return;
        }
        let w = sw * sprite.scale_x;
        let h = sh * sprite.scale_y;
        let buf_x = sprite.x - sprite.offset_x + sx * sprite.scale_x - scroll_x * sprite.scroll_x;
        let buf_y = sprite.y - sprite.offset_y + sy * sprite.scale_y - scroll_y * sprite.scroll_y;
        let dx = (buf_x - GAME_W / 2.0) * zoom + GAME_W / 2.0;
        let dy = (buf_y - GAME_H / 2.0) * zoom + GAME_H / 2.0;
        let dw = w * zoom;
        let dh = h * zoom;

        if sprite.angle.abs() > 0.01 {
            let origin_x = sprite.origin_x.unwrap_or(sx + sw / 2.0);
            let origin_y = sprite.origin_y.unwrap_or(sy + sh / 2.0);
            let object_dx = dx - sx * sprite.scale_x * zoom;
            let object_dy = dy - sy * sprite.scale_y * zoom;
            let pivot_x = object_dx + origin_x * sprite.scale_x * zoom;
            let pivot_y = object_dy + origin_y * sprite.scale_y * zoom;
            let rel_x = (sx + sw / 2.0 - origin_x) * sprite.scale_x * zoom;
            let rel_y = (sy + sh / 2.0 - origin_y) * sprite.scale_y * zoom;
            let (sin, cos) = sprite.angle.to_radians().sin_cos();
            let cx = pivot_x + rel_x * cos - rel_y * sin;
            let cy = pivot_y + rel_x * sin + rel_y * cos;
            gpu.push_quad_rotated(
                cx,
                cy,
                dw,
                dh,
                sx / tex_w,
                sy / tex_h,
                (sx + sw) / tex_w,
                (sy + sh) / tex_h,
                sprite.angle.to_radians(),
                sprite.flip_x,
                color,
            );
        } else {
            gpu.push_texture_region(
                tex_w,
                tex_h,
                sx,
                sy,
                sw,
                sh,
                dx,
                dy,
                dw,
                dh,
                sprite.flip_x,
                color,
            );
        }
        gpu.draw_batch(Some(tex));
    }

    /// Draw Lua sprites assigned to screen-space cameras with HUD zoom.
    fn draw_lua_sprites_screen(&self, gpu: &mut GpuState) {
        for layer in &self.draw_order {
            let tag = match layer {
                DrawLayer::LuaSprite(tag) => tag,
                DrawLayer::LuaCharacter(_) => continue,
                _ => continue,
            };
            let tex = match self.lua_textures.get(tag) {
                Some(t) => t,
                None => continue,
            };
            let sprite = match self.scripts.state.lua_sprites.get(tag) {
                Some(s) => s,
                None => continue,
            };
            if !sprite.visible || sprite.alpha <= 0.0 {
                continue;
            }
            if lua_camera_is_game(&sprite.camera) {
                continue;
            }

            let color = lua_sprite_color(sprite);
            let zoom = self.hud_zoom;

            // Animated sprite. If no current animation was explicitly selected,
            // draw the first registered animation frame instead of the full atlas.
            if let Some(atlas) = self.lua_atlases.get(tag) {
                let anim = if sprite.current_anim.is_empty() {
                    atlas.anim_names().into_iter().next()
                } else {
                    Some(sprite.current_anim.as_str())
                };
                if let Some(anim) = anim {
                    if let Some(frame) = atlas.get_frame(anim, sprite.anim_frame) {
                        let (off_x, off_y) =
                            sprite.anim_offsets.get(anim).copied().unwrap_or((0.0, 0.0));
                        let dx = (sprite.x - sprite.offset_x - off_x - GAME_W / 2.0) * zoom
                            + GAME_W / 2.0;
                        let dy = (sprite.y - sprite.offset_y - off_y - GAME_H / 2.0) * zoom
                            + GAME_H / 2.0;
                        let scale = sprite.scale_x * zoom;
                        gpu.draw_sprite_frame(
                            frame,
                            tex.width as f32,
                            tex.height as f32,
                            dx,
                            dy,
                            scale,
                            sprite.flip_x,
                            color,
                        );
                        gpu.draw_batch(Some(tex));
                        continue;
                    }
                }
                continue;
            }

            // Static sprite
            let tex_w = tex.width as f32;
            let tex_h = tex.height as f32;
            let sx = sprite.clip_x.min(tex_w).max(0.0);
            let sy = sprite.clip_y.min(tex_h).max(0.0);
            let sw = sprite.clip_w.unwrap_or(tex_w - sx).min(tex_w - sx).max(0.0);
            let sh = sprite.clip_h.unwrap_or(tex_h - sy).min(tex_h - sy).max(0.0);
            if sw <= 0.0 || sh <= 0.0 {
                continue;
            }
            let w = sw * sprite.scale_x;
            let h = sh * sprite.scale_y;
            let dx = (sprite.x - sprite.offset_x + sx * sprite.scale_x - GAME_W / 2.0) * zoom
                + GAME_W / 2.0;
            let dy = (sprite.y - sprite.offset_y + sy * sprite.scale_y - GAME_H / 2.0) * zoom
                + GAME_H / 2.0;
            let dw = w * zoom;
            let dh = h * zoom;
            if sprite.angle.abs() > 0.01 {
                let origin_x = sprite.origin_x.unwrap_or(sx + sw / 2.0);
                let origin_y = sprite.origin_y.unwrap_or(sy + sh / 2.0);
                let object_dx = dx - sx * sprite.scale_x * zoom;
                let object_dy = dy - sy * sprite.scale_y * zoom;
                let pivot_x = object_dx + origin_x * sprite.scale_x * zoom;
                let pivot_y = object_dy + origin_y * sprite.scale_y * zoom;
                let rel_x = (sx + sw / 2.0 - origin_x) * sprite.scale_x * zoom;
                let rel_y = (sy + sh / 2.0 - origin_y) * sprite.scale_y * zoom;
                let (sin, cos) = sprite.angle.to_radians().sin_cos();
                let cx = pivot_x + rel_x * cos - rel_y * sin;
                let cy = pivot_y + rel_x * sin + rel_y * cos;
                gpu.push_quad_rotated(
                    cx,
                    cy,
                    dw,
                    dh,
                    sx / tex_w,
                    sy / tex_h,
                    (sx + sw) / tex_w,
                    (sy + sh) / tex_h,
                    sprite.angle.to_radians(),
                    sprite.flip_x,
                    color,
                );
            } else {
                gpu.push_texture_region(
                    tex_w,
                    tex_h,
                    sx,
                    sy,
                    sw,
                    sh,
                    dx,
                    dy,
                    dw,
                    dh,
                    sprite.flip_x,
                    color,
                );
            }
            gpu.draw_batch(Some(tex));
        }
    }

    fn draw_death_overlay(&self, gpu: &mut GpuState) {
        if let Some(death) = &self.death {
            let tint = (death.timer as f32 / 500.0).min(1.0);
            gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.1 * tint, 0.5 * tint]);
            gpu.draw_batch(None);

            if death.fade_alpha > 0.0 {
                let a = death.fade_alpha.min(1.0);
                gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.0, a]);
                gpu.draw_batch(None);
            }
        }
    }

    /// Whether a hold note is actively being held (should render behind strum).
    fn is_hold_active(&self, nd: &NoteData) -> bool {
        if !nd.was_good_hit {
            return false;
        }
        if nd.must_press {
            self.game.keys_held[nd.lane]
        } else {
            true
        }
    }

    /// Check whether a note uses a custom texture atlas (different from the default note skin).
    fn note_uses_custom_texture(&self, nd: &NoteData) -> bool {
        use rustic_core::note::NoteKind;
        match &nd.kind {
            NoteKind::Custom(name) => self.custom_note_assets.contains_key(name.as_str()),
            NoteKind::Hurt => self.custom_note_assets.contains_key("Hurt Note"),
            _ => false,
        }
    }

    /// Resolve which note assets, animation name, and color tint to use for a given note.
    /// Returns (assets, anim_name, rgba_tint). Tint is [1,1,1,1] for normal notes.
    fn resolve_note_assets<'a>(
        &'a self,
        nd: &NoteData,
        player: bool,
    ) -> (Option<&'a NoteAssets>, Option<String>, [f32; 4]) {
        use rustic_core::note::NoteKind;
        let white = [1.0, 1.0, 1.0, 1.0];
        let default_anim = NOTE_ANIMS[nd.lane].to_string();

        let default_assets = if player {
            self.note_assets.as_ref()
        } else {
            self.opp_note_assets.as_ref().or(self.note_assets.as_ref())
        };

        match &nd.kind {
            NoteKind::Custom(name) => {
                let custom_assets = self.custom_note_assets.get(name.as_str());
                let config = rustic_core::note::get_note_type_config(name);
                let assets = custom_assets.or(default_assets);
                // Use custom anim names if registered, otherwise fall back to defaults
                let anim = config
                    .as_ref()
                    .and_then(|c| c.note_anims.as_ref())
                    .map(|anims| anims[nd.lane].clone())
                    .unwrap_or(default_anim);
                // Tint harmful custom notes red if no custom skin loaded
                let tint = if custom_assets.is_none() && nd.kind.is_harmful() {
                    [1.0, 0.35, 0.35, 1.0]
                } else {
                    white
                };
                (assets, Some(anim), tint)
            }
            NoteKind::Hurt => {
                // Use custom hurt assets if loaded, otherwise tint red
                let hurt_assets = self.custom_note_assets.get("Hurt Note");
                let assets = hurt_assets.or(default_assets);
                let tint = if hurt_assets.is_none() {
                    [1.0, 0.35, 0.35, 1.0] // red tint when no dedicated skin
                } else {
                    white
                };
                (assets, Some(default_anim), tint)
            }
            _ => {
                // Normal, Alt, Hey, GfSing, NoAnim — use default assets
                (default_assets, Some(default_anim), white)
            }
        }
    }

    fn draw_strum(&self, gpu: &mut GpuState, lane: usize, player: bool) {
        let (x, y, alpha, angle, scale) = self.strum_pos(lane, player);
        if alpha <= 0.0 {
            return;
        }
        let elapsed = if player {
            self.game.player_confirm[lane]
        } else {
            self.game.opponent_confirm[lane]
        };
        let is_playable_side = player != self.game.play_as_opponent;
        let (anim, frame_idx) = if elapsed > 0.0 {
            let idx = (elapsed / (1000.0 / 24.0)) as usize;
            (CONFIRM_ANIMS[lane], idx)
        } else if is_playable_side && self.game.keys_held[lane] {
            (PRESS_ANIMS[lane], 0)
        } else {
            (STRUM_ANIMS[lane], 0)
        };
        // Use opponent note skin for opponent strums
        let assets = if !player {
            self.opp_note_assets.as_ref().or(self.note_assets.as_ref())
        } else {
            self.note_assets.as_ref()
        };
        if let Some(assets) = assets {
            // Center on the lane's midpoint (fixed reference size, skin-independent)
            let ref_size = NOTE_WIDTH / NOTE_SCALE; // 160 — standard Psych frame size
            let cx = x + ref_size * NOTE_SCALE / 2.0;
            let cy = y + ref_size * NOTE_SCALE / 2.0;

            let count = assets.atlas.frame_count(anim);
            let clamped = if count > 0 {
                frame_idx.min(count - 1)
            } else {
                0
            };

            let mut frame = assets.atlas.get_frame(anim, clamped);
            let mut draw_scale = scale;
            let mut color_mult = 1.0;
            if frame.is_none() && (anim == PRESS_ANIMS[lane] || anim == CONFIRM_ANIMS[lane]) {
                frame = assets.atlas.get_frame(STRUM_ANIMS[lane], 0);
                if anim == PRESS_ANIMS[lane] {
                    draw_scale = scale * 0.9; // shrink slightly
                    color_mult = 0.5; // darken
                } else {
                    draw_scale = scale * 1.05; // slightly larger for confirm
                    color_mult = 1.0;
                }
            }

            if let Some(f) = frame {
                let draw_x = cx - f.frame_w * draw_scale / 2.0;
                let draw_y = cy - f.frame_h * draw_scale / 2.0;
                let a = alpha.clamp(0.0, 1.0);
                let c = a * color_mult;
                let color = [c, c, c, a];

                if angle.abs() > 0.01 {
                    gpu.draw_sprite_frame_rotated(
                        f,
                        assets.tex_w,
                        assets.tex_h,
                        draw_x,
                        draw_y,
                        draw_scale,
                        false,
                        angle,
                        color,
                    );
                } else {
                    gpu.draw_sprite_frame(
                        f,
                        assets.tex_w,
                        assets.tex_h,
                        draw_x,
                        draw_y,
                        draw_scale,
                        false,
                        color,
                    );
                }
            }
        }
    }

    fn draw_hold_tail(&self, gpu: &mut GpuState, nd: &NoteData, y_pos: f32) {
        // Use opponent note skin for opponent holds
        let assets = if !nd.must_press {
            self.opp_note_assets.as_ref().or(self.note_assets.as_ref())
        } else {
            self.note_assets.as_ref()
        };
        let assets = match assets {
            Some(a) => a,
            None => return,
        };
        let lane = nd.lane;

        let piece = match assets.atlas.get_frame(HOLD_PIECE_ANIMS[lane], 0) {
            Some(f) => f.clone(),
            None => return,
        };
        // End cap is optional — some assets have typos (e.g. "pruple end hold")
        let end = assets.atlas.get_frame(HOLD_END_ANIMS[lane], 0).cloned();

        let tw = assets.tex_w;
        let th = assets.tex_h;
        let (x, sy, sa, _ang, _sc) = self.strum_pos(lane, nd.must_press);
        let a = sa.clamp(0.0, 1.0);
        let color = [1.0, 1.0, 1.0, a];

        let pw = piece.src.w * NOTE_SCALE;
        let ph = piece.src.h * NOTE_SCALE;
        let (ew, eh) = end
            .as_ref()
            .map_or((pw, ph), |e| (e.src.w * NOTE_SCALE, e.src.h * NOTE_SCALE));

        let hold_h = (0.45 * nd.sustain_length * self.game.song_speed) as f32;

        // Effective downscroll for this note (strum setting XOR per-note reverse)
        let ds = self.is_strum_downscroll(lane, nd.must_press) ^ nd.is_reversing_scroll;
        // Flip hold piece textures vertically in downscroll (or when flip_y is set)
        let flip_v = ds ^ nd.flip_y;

        let px = x + (NOTE_WIDTH - pw) / 2.0;
        let ex = x + (NOTE_WIDTH - ew) / 2.0;

        if ds {
            // === Downscroll: hold extends UPWARD from note head ===
            let hold_bottom = y_pos + NOTE_WIDTH * 0.5 - nd.correction_offset;
            let hold_top_y = hold_bottom - hold_h;

            // Clip: once hit, hide the part that passed the strum line
            let clip_bottom = if nd.was_good_hit {
                sy + NOTE_WIDTH * 0.5
            } else {
                GAME_H + 999.0
            };

            // End cap at top
            let end_cap_y = hold_top_y;
            if let Some(end) = &end {
                if end_cap_y < clip_bottom && end_cap_y + eh > -100.0 {
                    let vis_bottom = (end_cap_y + eh).min(clip_bottom);
                    let vis_h = vis_bottom - end_cap_y;
                    if vis_h > 0.5 {
                        let clip_frac = if vis_bottom < end_cap_y + eh {
                            (eh - vis_h) / eh
                        } else {
                            0.0
                        };
                        Self::push_region_flip_v(
                            gpu,
                            tw,
                            th,
                            end.src.x,
                            end.src.y,
                            end.src.w,
                            end.src.h,
                            ex,
                            end_cap_y,
                            ew,
                            vis_h,
                            clip_frac,
                            vis_h / eh,
                            flip_v,
                            color,
                        );
                    }
                }
            }

            // Tile hold pieces from end_cap bottom to hold_bottom
            let tile_start = end_cap_y + eh;
            let mut cy = tile_start;
            while cy < hold_bottom {
                let tile_h = ph.min(hold_bottom - cy);
                let vis_bottom = (cy + tile_h).min(clip_bottom);
                let vis_h = vis_bottom - cy;
                if vis_h > 0.5 && cy < GAME_H + 100.0 {
                    Self::push_region_flip_v(
                        gpu,
                        tw,
                        th,
                        piece.src.x,
                        piece.src.y,
                        piece.src.w,
                        piece.src.h,
                        px,
                        cy,
                        pw,
                        vis_h,
                        0.0,
                        vis_h / ph,
                        flip_v,
                        color,
                    );
                }
                cy += ph;
            }
        } else {
            // === Upscroll: hold extends DOWNWARD from note head ===
            let hold_top = y_pos + NOTE_WIDTH * 0.5 + nd.correction_offset;

            let clip_y = if nd.was_good_hit {
                sy + NOTE_WIDTH * 0.5
            } else {
                -999.0
            };

            let end_y = hold_top + hold_h - eh;

            let mut cy = hold_top;
            while cy < end_y {
                let tile_h = ph.min(end_y - cy);
                let vis_top = cy.max(clip_y);
                let vis_h = (cy + tile_h) - vis_top;

                if vis_h > 0.5 && vis_top < GAME_H + 100.0 {
                    let clip_frac = if vis_top > cy {
                        (vis_top - cy) / tile_h
                    } else {
                        0.0
                    };
                    Self::push_region_flip_v(
                        gpu,
                        tw,
                        th,
                        piece.src.x,
                        piece.src.y,
                        piece.src.w,
                        piece.src.h,
                        px,
                        vis_top,
                        pw,
                        vis_h,
                        clip_frac,
                        vis_h / ph,
                        flip_v,
                        color,
                    );
                }
                cy += ph;
            }

            if let Some(end) = &end {
                if end_y + eh > clip_y && end_y < GAME_H + 100.0 {
                    let vis_top = end_y.max(clip_y);
                    let vis_h = (end_y + eh) - vis_top;
                    if vis_h > 0.5 {
                        let clip_frac = if vis_top > end_y {
                            (vis_top - end_y) / eh
                        } else {
                            0.0
                        };
                        Self::push_region_flip_v(
                            gpu,
                            tw,
                            th,
                            end.src.x,
                            end.src.y,
                            end.src.w,
                            end.src.h,
                            ex,
                            vis_top,
                            ew,
                            vis_h,
                            clip_frac,
                            vis_h / eh,
                            flip_v,
                            color,
                        );
                    }
                }
            }
        }
    }

    /// Push a texture region with optional vertical flip.
    /// `clip_frac`: fraction of the source to skip from the leading edge (0.0 = no clip).
    /// `frac`: fraction of the source actually drawn (dst_h / full_tile_h).
    fn push_region_flip_v(
        gpu: &mut GpuState,
        tw: f32,
        th: f32,
        src_x: f32,
        src_y: f32,
        src_w: f32,
        src_h: f32,
        dst_x: f32,
        dst_y: f32,
        dst_w: f32,
        dst_h: f32,
        clip_frac: f32,
        frac: f32,
        flip_v: bool,
        color: [f32; 4],
    ) {
        let u0 = src_x / tw;
        let u1 = (src_x + src_w) / tw;
        let v0 = src_y / th; // top of source region
        let v1 = (src_y + src_h) / th; // bottom of source region
        let frac = frac.min(1.0);

        if flip_v {
            // Flipped: draw bottom of source at top of dest, top of source at bottom.
            // clip_frac skips from the bottom of the source (now the leading/top edge).
            let v_start = v1 - (v1 - v0) * clip_frac; // start after clip
            let v_end = v_start - (v1 - v0) * frac; // only draw frac of source
            gpu.push_quad(
                dst_x, dst_y, dst_w, dst_h, u0, v_start, u1, v_start, u1, v_end, u0, v_end, color,
            );
        } else {
            // Normal: clip_frac skips from the top of the source.
            let v_start = v0 + (v1 - v0) * clip_frac;
            let v_end = v_start + (v1 - v0) * frac;
            gpu.push_quad(
                dst_x, dst_y, dst_w, dst_h, u0, v_start, u1, v_start, u1, v_end, u0, v_end, color,
            );
        }
    }
}
