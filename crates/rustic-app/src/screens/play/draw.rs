use rustic_core::note::NoteData;
use rustic_core::rating;
use rustic_render::gpu::GpuState;

use super::{
    PlayScreen, DrawLayer, NoteAssets,
    GAME_W, GAME_H, STRUM_Y, NOTE_WIDTH, NOTE_SCALE,
    NOTE_ANIMS, STRUM_ANIMS, PRESS_ANIMS, CONFIRM_ANIMS,
    HOLD_PIECE_ANIMS, HOLD_END_ANIMS, SPLASH_PREFIXES,
    HEALTH_BAR_W, HEALTH_BAR_H, HEALTH_BAR_Y, HEALTH_BAR_X,
    RATING_SCALE,
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

impl PlayScreen {
    pub(super) fn draw_inner(&mut self, gpu: &mut GpuState) {
        let white = [1.0, 1.0, 1.0, 1.0];

        if !gpu.begin_frame() {
            return;
        }

        // Process any pending Lua sprite adds (may have been queued during update)
        self.process_lua_sprites(gpu);

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
            let pp_params: Vec<_> = self.scripts.state.postprocess_param_requests.drain(..).collect();
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

        // === Lua sprites behind characters ===
        let is_death = self.death.is_some();
        if !is_death {
            self.draw_lua_sprites(gpu, false);
        }

        // === Stage background color overlay (generic, controlled via setStageColor Lua API) ===
        if !is_death {
            let lc = self.stage_overlay.color_left;
            let rc = self.stage_overlay.color_right;
            let has_left = lc[3] > 0.001;
            let has_right = rc[3] > 0.001;
            if has_left || has_right {
                if lc != rc {
                    if has_left {
                        let c = [lc[0], lc[1], lc[2], lc[3] * 0.35];
                        gpu.push_colored_quad(0.0, 0.0, GAME_W / 2.0, GAME_H, c);
                        gpu.draw_batch(None);
                    }
                    if has_right {
                        let c = [rc[0], rc[1], rc[2], rc[3] * 0.35];
                        gpu.push_colored_quad(GAME_W / 2.0, 0.0, GAME_W / 2.0, GAME_H, c);
                        gpu.draw_batch(None);
                    }
                } else {
                    let c = [lc[0], lc[1], lc[2], lc[3] * 0.35];
                    gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, c);
                    gpu.draw_batch(None);
                }
            }
        }

        // === Stage & Characters (draw order from stage objects) ===
        for layer in &self.draw_order {
            match layer {
                DrawLayer::StageBg(i) => {
                    let bg = &self.stage_bg[*i];
                    bg.draw(gpu, &self.camera);
                    gpu.draw_batch(Some(&bg.texture));
                }
                DrawLayer::Gf => if !is_death {
                    if let Some(gf) = &self.char_gf {
                        gf.draw(gpu, &self.camera);
                        gpu.draw_batch(Some(gf.texture()));
                    }
                }
                DrawLayer::Dad => if !is_death {
                    if let Some(dad) = &self.char_dad {
                        // Draw reflection below character first
                        if self.reflections_enabled {
                            dad.draw_reflection(gpu, &self.camera, self.reflection_alpha, self.reflection_dist_y);
                            gpu.draw_batch(Some(dad.texture()));
                        }
                        dad.draw(gpu, &self.camera);
                        gpu.draw_batch(Some(dad.texture()));
                    }
                }
                DrawLayer::Bf => {
                    if let Some(death) = &self.death {
                        death.character.draw(gpu, &self.camera);
                        gpu.draw_batch(Some(death.character.texture()));
                    } else if let Some(bf) = &self.char_bf {
                        // Draw reflection below character first
                        if self.reflections_enabled {
                            bf.draw_reflection(gpu, &self.camera, self.reflection_alpha, self.reflection_dist_y);
                            gpu.draw_batch(Some(bf.texture()));
                        }
                        bf.draw(gpu, &self.camera);
                        gpu.draw_batch(Some(bf.texture()));
                    }
                }
            }
        }

        // === Lua sprites in front of characters ===
        if !is_death {
            self.draw_lua_sprites(gpu, true);
        }

        // Skip HUD during death
        if self.death.is_some() {
            self.draw_death_overlay(gpu);
            gpu.end_frame();
            return;
        }

        // === HUD zoom transform ===
        let hz = self.hud_zoom;
        let hud_x = |x: f32| -> f32 { GAME_W / 2.0 + (x - GAME_W / 2.0) * hz };
        let hud_y = |y: f32| -> f32 { GAME_H / 2.0 + (y - GAME_H / 2.0) * hz };
        let hud_s = |s: f32| -> f32 { s * hz };

        // === Note batch: held tails → strums → unheld tails → note heads ===

        // Compute note Y positions for rendering — use actual strum Y (modcharts move them)
        let note_positions: Vec<(usize, f32)> = (0..self.game.notes.len())
            .map(|i| {
                let nd = &self.game.notes[i];
                let (_sx, sy, _sa, _ang, _sc) = self.strum_pos(nd.lane, nd.must_press);
                (i, self.game.note_y(nd.strum_time, sy))
            })
            .collect();

        // === Opponent side: holds (behind) → strums → holds (front) → note heads ===
        for &(i, y_pos) in &note_positions {
            let nd = &self.game.notes[i];
            if nd.must_press { continue; }
            if nd.sustain_length > 0.0 && !nd.too_late && self.is_hold_active(nd) {
                self.draw_hold_tail(gpu, nd, y_pos);
            }
        }
        for lane in 0..4 {
            self.draw_strum(gpu, lane, false);
        }
        for &(i, y_pos) in &note_positions {
            let nd = &self.game.notes[i];
            if nd.must_press { continue; }
            if nd.sustain_length > 0.0 && !nd.too_late && !self.is_hold_active(nd) {
                self.draw_hold_tail(gpu, nd, y_pos);
            }
        }
        for &(i, y_pos) in &note_positions {
            let nd = &self.game.notes[i];
            if nd.must_press || nd.was_good_hit || nd.too_late { continue; }
            if !nd.visible { continue; }
            if y_pos < -NOTE_WIDTH || y_pos > GAME_H + NOTE_WIDTH { continue; }
            let (sx, _sy, sa, ang, _sc) = self.strum_pos(nd.lane, false);
            let a = (sa * nd.alpha).clamp(0.0, 1.0);
            if a <= 0.0 { continue; }
            let note_ang = ang + nd.angle;
            let note_scale = NOTE_SCALE * (nd.scale_x / 0.7); // scale relative to default 0.7
            let assets = self.opp_note_assets.as_ref().or(self.note_assets.as_ref());
            if let Some(assets) = assets {
                // Center note on lane midpoint (same as strum centering)
                let ref_size = NOTE_WIDTH / NOTE_SCALE;
                let cx = sx + nd.offset_x + ref_size * NOTE_SCALE / 2.0;
                let cy = y_pos + nd.offset_y + ref_size * NOTE_SCALE / 2.0;
                let color = note_color(nd, a);
                if let Some(frame) = assets.atlas.get_frame(NOTE_ANIMS[nd.lane], 0) {
                    let draw_x = cx - frame.frame_w * note_scale / 2.0;
                    let draw_y = cy - frame.frame_h * note_scale / 2.0;
                    if note_ang.abs() > 0.01 {
                        gpu.draw_sprite_frame_rotated(
                            frame, assets.tex_w, assets.tex_h,
                            draw_x, draw_y, note_scale, false, note_ang, color,
                        );
                    } else {
                        gpu.draw_sprite_frame(
                            frame, assets.tex_w, assets.tex_h,
                            draw_x, draw_y, note_scale, false, color,
                        );
                    }
                }
            }
        }
        // Flush opponent batch
        let opp_tex = self.opp_note_assets.as_ref().or(self.note_assets.as_ref());
        if let Some(assets) = opp_tex {
            gpu.draw_batch(Some(&assets.texture));
        }

        // === Player side: holds (behind) → strums → holds (front) → note heads ===
        for &(i, y_pos) in &note_positions {
            let nd = &self.game.notes[i];
            if !nd.must_press { continue; }
            if nd.sustain_length > 0.0 && !nd.too_late && self.is_hold_active(nd) {
                self.draw_hold_tail(gpu, nd, y_pos);
            }
        }
        for lane in 0..4 {
            self.draw_strum(gpu, lane, true);
        }
        for &(i, y_pos) in &note_positions {
            let nd = &self.game.notes[i];
            if !nd.must_press { continue; }
            if nd.sustain_length > 0.0 && !nd.too_late && !self.is_hold_active(nd) {
                self.draw_hold_tail(gpu, nd, y_pos);
            }
        }
        for &(i, y_pos) in &note_positions {
            let nd = &self.game.notes[i];
            if !nd.must_press || nd.was_good_hit || nd.too_late { continue; }
            if !nd.visible { continue; }
            if y_pos < -NOTE_WIDTH || y_pos > GAME_H + NOTE_WIDTH { continue; }
            let (sx, _sy, sa, ang, _sc) = self.strum_pos(nd.lane, true);
            let a = (sa * nd.alpha).clamp(0.0, 1.0);
            if a <= 0.0 { continue; }
            let note_ang = ang + nd.angle;
            let note_scale = NOTE_SCALE * (nd.scale_x / 0.7);
            if let Some(assets) = &self.note_assets {
                // Center note on lane midpoint (same as strum centering)
                let ref_size = NOTE_WIDTH / NOTE_SCALE;
                let cx = sx + nd.offset_x + ref_size * NOTE_SCALE / 2.0;
                let cy = y_pos + nd.offset_y + ref_size * NOTE_SCALE / 2.0;
                let color = note_color(nd, a);
                if let Some(frame) = assets.atlas.get_frame(NOTE_ANIMS[nd.lane], 0) {
                    let draw_x = cx - frame.frame_w * note_scale / 2.0;
                    let draw_y = cy - frame.frame_h * note_scale / 2.0;
                    if note_ang.abs() > 0.01 {
                        gpu.draw_sprite_frame_rotated(
                            frame, assets.tex_w, assets.tex_h,
                            draw_x, draw_y, note_scale, false, note_ang, color,
                        );
                    } else {
                        gpu.draw_sprite_frame(
                            frame, assets.tex_w, assets.tex_h,
                            draw_x, draw_y, note_scale, false, color,
                        );
                    }
                }
            }
        }
        // Flush player batch
        if let Some(assets) = &self.note_assets {
            gpu.draw_batch(Some(&assets.texture));
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
                        frame, splash.tex_w, splash.tex_h,
                        cx, cy, scale, false, [1.0, 1.0, 1.0, 0.6],
                    );
                }
            }
            if !self.splashes.is_empty() {
                gpu.draw_batch(Some(&splash.texture));
            }
        }

        // === Health bar ===
        let health_pct = self.game.score.health_percent();

        if let Some(chb) = &self.custom_healthbar {
            if chb.alpha > 0.001 {
                // Custom health bar: overlay + clipped bar sprites
                let a = chb.alpha;
                let scale = chb.scale * hz;
                let ow = chb.overlay_texture.width as f32 * scale;
                let oh = chb.overlay_texture.height as f32 * scale;
                let bw = chb.bar_texture.width as f32 * scale;
                let bh = chb.bar_texture.height as f32 * scale;

                // Center overlay on screen; top for downscroll, bottom for upscroll
                let overlay_x = (GAME_W * hz - ow) / 2.0;
                let hbar_y = if self.downscroll { GAME_H * 0.02 } else { HEALTH_BAR_Y };
                let overlay_y = hud_y(hbar_y) - oh / 2.0;
                // Bar centered within overlay
                let bar_x = overlay_x + (ow - bw) / 2.0;
                let bar_y = overlay_y + (oh - bh) / 2.0;

                // Smoothed health for fill (capped at 1.7/2 = 0.85)
                let vida = (chb.health_lerp.clamp(0.0, 1.7)) / 2.0;
                let bar_src_w = chb.bar_texture.width as f32;
                let bar_src_h = chb.bar_texture.height as f32;

                // Opponent bar (left side): clips from x=0, width = (1-vida) of bar
                let opp_clip_w = bar_src_w * (1.0 - vida);
                let opp_color = [chb.left_color[0] * a, chb.left_color[1] * a, chb.left_color[2] * a, a];
                gpu.push_texture_region(
                    bar_src_w, bar_src_h,
                    0.0, 0.0, opp_clip_w, bar_src_h,
                    bar_x, bar_y, opp_clip_w * scale, bh,
                    false, opp_color,
                );
                gpu.draw_batch(Some(&chb.bar_texture));

                // Player bar (right side): clips from right edge, width = vida of bar
                let player_clip_w = bar_src_w * vida;
                let player_src_x = bar_src_w - player_clip_w;
                let player_color = [chb.right_color[0] * a, chb.right_color[1] * a, chb.right_color[2] * a, a];
                gpu.push_texture_region(
                    bar_src_w, bar_src_h,
                    player_src_x, 0.0, player_clip_w, bar_src_h,
                    bar_x + bw - player_clip_w * scale, bar_y, player_clip_w * scale, bh,
                    false, player_color,
                );
                gpu.draw_batch(Some(&chb.bar_texture));

                // Overlay on top
                let overlay_color = [a, a, a, a];
                gpu.push_texture_region(
                    chb.overlay_texture.width as f32, chb.overlay_texture.height as f32,
                    0.0, 0.0, chb.overlay_texture.width as f32, chb.overlay_texture.height as f32,
                    overlay_x, overlay_y, ow, oh,
                    false, overlay_color,
                );
                gpu.draw_batch(Some(&chb.overlay_texture));

                // Cut point for icons
                let cut_x = bar_x + bw * (1.0 - vida);

                if self.game.song_ended {
                    gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.0, 0.6]);
                    gpu.draw_batch(None);
                }

                // Icons at cut point
                let bf_losing = health_pct < 0.2;
                let dad_losing = health_pct > 0.8;
                let icon_size = 150.0 * 0.75;
                let icon_spacing = -20.0 * scale;

                if let Some(icon) = &self.icon_dad {
                    let s = self.icon_scale_dad;
                    let draw_size = hud_s(icon_size) * s;
                    let icon_y = overlay_y + oh / 2.0 - draw_size / 2.0;
                    let src_x = if dad_losing { 150.0 } else { 0.0 };
                    gpu.push_texture_region(
                        icon.width as f32, icon.height as f32,
                        src_x, 0.0, 150.0, 150.0,
                        cut_x - draw_size + icon_spacing, icon_y, draw_size, draw_size,
                        false, [a, a, a, a],
                    );
                    gpu.draw_batch(Some(icon));
                }

                if let Some(icon) = &self.icon_bf {
                    let s = self.icon_scale_bf;
                    let draw_size = hud_s(icon_size) * s;
                    let icon_y = overlay_y + oh / 2.0 - draw_size / 2.0;
                    let src_x = if bf_losing { 150.0 } else { 0.0 };
                    gpu.push_texture_region(
                        icon.width as f32, icon.height as f32,
                        src_x, 0.0, 150.0, 150.0,
                        cut_x - icon_spacing, icon_y, draw_size, draw_size,
                        true, [a, a, a, a],
                    );
                    gpu.draw_batch(Some(icon));
                }

                // Score text below overlay
                let grade = self.game.score.grade();
                let score_text = format!(
                    "Score: {} | Misses: {} | Acc: {:.2}% [{}]",
                    self.game.score.score, self.game.score.misses, self.game.score.accuracy(), grade,
                );
                gpu.draw_text(&score_text, overlay_x, overlay_y + oh + 6.0, 16.0, [a, a, a, a]);
            }
        } else {
            // Default health bar (bottom for upscroll, top for downscroll)
            let hbx = hud_x(HEALTH_BAR_X);
            let bar_base_y = if self.downscroll { 80.0 } else { HEALTH_BAR_Y };
            let hby = hud_y(bar_base_y);
            let hbw = hud_s(HEALTH_BAR_W);
            let hbh = hud_s(HEALTH_BAR_H);
            // Black border
            gpu.push_colored_quad(hbx - 2.0, hby - 2.0, hbw + 4.0, hbh + 4.0, [0.0, 0.0, 0.0, 1.0]);
            gpu.draw_batch(None);
            // Colored fills: opponent on left, player on right
            gpu.push_colored_quad(hbx, hby, hbw, hbh, self.hb_color_dad);
            let player_w = hbw * health_pct;
            gpu.push_colored_quad(hbx + hbw - player_w, hby, player_w, hbh, self.hb_color_bf);
            gpu.draw_batch(None);

            if self.game.song_ended {
                gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.0, 0.6]);
                gpu.draw_batch(None);
            }

            // Icons with bop
            let divider_x = hbx + hbw * (1.0 - health_pct);
            let bf_losing = health_pct < 0.2;
            let dad_losing = health_pct > 0.8;
            let icon_size = 150.0 * 0.75;

            if let Some(icon) = &self.icon_dad {
                let s = self.icon_scale_dad;
                let draw_size = hud_s(icon_size) * s;
                let icon_y = hby + hbh / 2.0 - draw_size / 2.0;
                let src_x = if dad_losing { 150.0 } else { 0.0 };
                gpu.push_texture_region(
                    icon.width as f32, icon.height as f32,
                    src_x, 0.0, 150.0, 150.0,
                    divider_x - draw_size + 15.0, icon_y, draw_size, draw_size,
                    false, white,
                );
                gpu.draw_batch(Some(icon));
            }

            if let Some(icon) = &self.icon_bf {
                let s = self.icon_scale_bf;
                let draw_size = hud_s(icon_size) * s;
                let icon_y = hby + hbh / 2.0 - draw_size / 2.0;
                let src_x = if bf_losing { 150.0 } else { 0.0 };
                gpu.push_texture_region(
                    icon.width as f32, icon.height as f32,
                    src_x, 0.0, 150.0, 150.0,
                    divider_x - 15.0, icon_y, draw_size, draw_size,
                    true, white,
                );
                gpu.draw_batch(Some(icon));
            }

            // Score text
            let grade = self.game.score.grade();
            let score_text = format!(
                "Score: {} | Misses: {} | Acc: {:.2}% [{}]",
                self.game.score.score, self.game.score.misses, self.game.score.accuracy(), grade,
            );
            gpu.draw_text(&score_text, hbx, hby + hbh + 6.0, 16.0, white);
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
                        tex.width as f32, tex.height as f32,
                        0.0, 0.0, tex.width as f32, tex.height as f32,
                        rx, ry, w, h, false, color,
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
                                tex.width as f32, tex.height as f32,
                                0.0, 0.0, tex.width as f32, tex.height as f32,
                                nx, ny, w, h, false, color,
                            );
                            gpu.draw_batch(Some(tex));
                        }
                    }
                }
            }
        }

        // === Lua sprites on camHUD ===
        self.draw_lua_sprites_hud(gpu);

        // === Countdown sprite ===
        if self.countdown_alpha > 0.0 {
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
                gpu.push_texture_region(
                    w, h, 0.0, 0.0, w, h,
                    cx, cy, w, h, false, color,
                );
                gpu.draw_batch(Some(tex));
            }
        }

        // === Pause menu ===
        if self.paused {
            gpu.push_colored_quad(0.0, 0.0, GAME_W, GAME_H, [0.0, 0.0, 0.0, 0.6]);
            gpu.draw_batch(None);

            let skip_secs = (self.skip_target_ms / 1000.0).max(0.0);
            let skip_min = (skip_secs / 60.0) as u32;
            let skip_sec = (skip_secs % 60.0) as u32;
            let skip_label = format!("Skip To  < {}:{:02} >", skip_min, skip_sec);
            let items: [&str; 4] = ["Resume", "Restart Song", &skip_label, "Exit to Menu"];
            let menu_y = GAME_H / 2.0 - 80.0;
            for (i, label) in items.iter().enumerate() {
                let y = menu_y + i as f32 * 40.0;
                let color = if i == self.pause_selection {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    [0.6, 0.6, 0.6, 1.0]
                };
                let prefix = if i == self.pause_selection { "> " } else { "  " };
                gpu.draw_text(
                    &format!("{}{}", prefix, label),
                    GAME_W / 2.0 - 120.0, y, 28.0, color,
                );
            }
            gpu.draw_text("PAUSED", GAME_W / 2.0 - 60.0, menu_y - 60.0, 36.0, white);
        }

        // === Song results ===
        if self.game.song_ended && !self.paused {
            let fc = rating::classify_fc(
                self.game.score.sicks, self.game.score.goods,
                self.game.score.bads, self.game.score.shits, self.game.score.misses,
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
                self.game.score.score, self.game.score.accuracy(), fc_str, self.game.score.grade(),
                self.game.score.sicks, self.game.score.goods, self.game.score.bads, self.game.score.shits,
                self.game.score.max_combo, self.game.score.misses,
            );
            gpu.draw_text(&results, GAME_W / 2.0 - 180.0, 200.0, 24.0, white);
        }

        // Touch lane indicators (Android only — subtle FNF-style pads at bottom)
        if cfg!(target_os = "android") && !self.paused && self.death.is_none() && !self.game.song_ended {
            self.draw_touch_ui(gpu);
        }

        gpu.end_frame();
    }

    /// Draw FNF-style semi-transparent arrow touch pads at the bottom of the screen.
    fn draw_touch_ui(&self, gpu: &mut GpuState) {
        let Some(assets) = self.note_assets.as_ref() else { return };
        let col_w = GAME_W / 4.0;
        let pad_scale = NOTE_SCALE * 1.1;
        let ref_size = NOTE_WIDTH / NOTE_SCALE; // 160 standard frame size
        let pad_y = GAME_H - ref_size * pad_scale - 10.0;
        let alpha = 0.25;

        for lane in 0..4usize {
            let anim = STRUM_ANIMS[lane];
            if let Some(frame) = assets.atlas.get_frame(anim, 0) {
                let cx = col_w * lane as f32 + col_w / 2.0;
                let draw_x = cx - frame.frame_w * pad_scale / 2.0;
                let draw_y = pad_y + (ref_size * pad_scale - frame.frame_h * pad_scale) / 2.0;
                let color = [alpha, alpha, alpha, alpha];
                gpu.draw_sprite_frame(
                    frame, assets.tex_w, assets.tex_h,
                    draw_x, draw_y, pad_scale, false, color,
                );
            }
        }
        gpu.draw_batch(Some(&assets.texture));

        // Thin lane dividers
        for lane in 1..4 {
            let x = col_w * lane as f32;
            gpu.push_colored_quad(x - 0.5, pad_y, 1.0, ref_size * pad_scale, [1.0, 1.0, 1.0, 0.06]);
        }
        gpu.draw_batch(None);
    }

    /// Draw all Lua-created sprites in either the behind or in-front layer (game camera only).
    fn draw_lua_sprites(&self, gpu: &mut GpuState, front: bool) {
        let tags = if front { &self.lua_front } else { &self.lua_behind };
        for tag in tags {
            let tex = match self.lua_textures.get(tag) {
                Some(t) => t,
                None => continue,
            };
            let sprite = match self.scripts.state.lua_sprites.get(tag) {
                Some(s) => s,
                None => continue,
            };
            if !sprite.visible || sprite.alpha <= 0.0 { continue; }

            // Skip HUD sprites — they're drawn in draw_lua_sprites_hud
            let is_hud = sprite.camera == "camHUD" || sprite.camera == "hud";
            if is_hud { continue; }

            let a = sprite.alpha.clamp(0.0, 1.0);
            let color = [a, a, a, a];

            let cam = &self.camera;
            let scroll_x = cam.x - GAME_W / 2.0;
            let scroll_y = cam.y - GAME_H / 2.0;
            let zoom = cam.zoom;

            // Animated sprite: render current atlas frame
            if let Some(atlas) = self.lua_atlases.get(tag) {
                if !sprite.current_anim.is_empty() {
                    if let Some(frame) = atlas.get_frame(&sprite.current_anim, sprite.anim_frame) {
                        let (off_x, off_y) = sprite.anim_offsets
                            .get(&sprite.current_anim)
                            .copied()
                            .unwrap_or((0.0, 0.0));
                        let world_x = sprite.x - off_x;
                        let world_y = sprite.y - off_y;
                        let buf_x = world_x - scroll_x * sprite.scroll_x;
                        let buf_y = world_y - scroll_y * sprite.scroll_y;
                        let dx = (buf_x - GAME_W / 2.0) * zoom + GAME_W / 2.0;
                        let dy = (buf_y - GAME_H / 2.0) * zoom + GAME_H / 2.0;
                        let scale = sprite.scale_x * zoom;

                        gpu.draw_sprite_frame(
                            frame, tex.width as f32, tex.height as f32,
                            dx, dy, scale, sprite.flip_x, color,
                        );
                        gpu.draw_batch(Some(tex));
                        continue;
                    }
                }
            }

            // Static sprite: draw full texture
            let w = tex.width as f32 * sprite.scale_x;
            let h = tex.height as f32 * sprite.scale_y;
            let buf_x = sprite.x - scroll_x * sprite.scroll_x;
            let buf_y = sprite.y - scroll_y * sprite.scroll_y;
            let dx = (buf_x - GAME_W / 2.0) * zoom + GAME_W / 2.0;
            let dy = (buf_y - GAME_H / 2.0) * zoom + GAME_H / 2.0;
            let dw = w * zoom;
            let dh = h * zoom;

            if sprite.angle.abs() > 0.01 {
                // Rotated sprite: draw around center
                let cx = dx + dw / 2.0;
                let cy = dy + dh / 2.0;
                gpu.push_quad_rotated(
                    cx, cy, dw, dh,
                    0.0, 0.0, 1.0, 1.0,
                    sprite.angle.to_radians(),
                    sprite.flip_x, color,
                );
            } else {
                gpu.push_texture_region(
                    tex.width as f32, tex.height as f32,
                    0.0, 0.0, tex.width as f32, tex.height as f32,
                    dx, dy, dw, dh,
                    sprite.flip_x, color,
                );
            }
            gpu.draw_batch(Some(tex));
        }
    }

    /// Draw Lua sprites assigned to camHUD (rendered in HUD space with HUD zoom).
    fn draw_lua_sprites_hud(&self, gpu: &mut GpuState) {
        let all_tags = self.lua_behind.iter().chain(self.lua_front.iter());
        for tag in all_tags {
            let tex = match self.lua_textures.get(tag) {
                Some(t) => t,
                None => continue,
            };
            let sprite = match self.scripts.state.lua_sprites.get(tag) {
                Some(s) => s,
                None => continue,
            };
            if !sprite.visible || sprite.alpha <= 0.0 { continue; }
            let is_hud = sprite.camera == "camHUD" || sprite.camera == "hud";
            if !is_hud { continue; }

            let a = sprite.alpha.clamp(0.0, 1.0);
            let color = [a, a, a, a];
            let zoom = self.hud_zoom;

            // Animated sprite
            if let Some(atlas) = self.lua_atlases.get(tag) {
                if !sprite.current_anim.is_empty() {
                    if let Some(frame) = atlas.get_frame(&sprite.current_anim, sprite.anim_frame) {
                        let (off_x, off_y) = sprite.anim_offsets
                            .get(&sprite.current_anim)
                            .copied()
                            .unwrap_or((0.0, 0.0));
                        let dx = (sprite.x - off_x - GAME_W / 2.0) * zoom + GAME_W / 2.0;
                        let dy = (sprite.y - off_y - GAME_H / 2.0) * zoom + GAME_H / 2.0;
                        let scale = sprite.scale_x * zoom;
                        gpu.draw_sprite_frame(
                            frame, tex.width as f32, tex.height as f32,
                            dx, dy, scale, sprite.flip_x, color,
                        );
                        gpu.draw_batch(Some(tex));
                        continue;
                    }
                }
            }

            // Static sprite
            let w = tex.width as f32 * sprite.scale_x;
            let h = tex.height as f32 * sprite.scale_y;
            let dx = (sprite.x - GAME_W / 2.0) * zoom + GAME_W / 2.0;
            let dy = (sprite.y - GAME_H / 2.0) * zoom + GAME_H / 2.0;
            let dw = w * zoom;
            let dh = h * zoom;
            gpu.push_texture_region(
                tex.width as f32, tex.height as f32,
                0.0, 0.0, tex.width as f32, tex.height as f32,
                dx, dy, dw, dh,
                sprite.flip_x, color,
            );
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
        if !nd.was_good_hit { return false; }
        if nd.must_press { self.game.keys_held[nd.lane] } else { true }
    }

    // === Note drawing helpers ===

    fn draw_note_sprite(&self, gpu: &mut GpuState, anim: &str, x: f32, y: f32, scale: f32) {
        self.draw_note_sprite_alpha(gpu, anim, x, y, scale, 1.0);
    }

    fn draw_note_sprite_alpha(&self, gpu: &mut GpuState, anim: &str, x: f32, y: f32, scale: f32, alpha: f32) {
        if let Some(assets) = &self.note_assets {
            if let Some(frame) = assets.atlas.get_frame(anim, 0) {
                gpu.draw_sprite_frame(
                    frame, assets.tex_w, assets.tex_h,
                    x, y, scale, false, [alpha, alpha, alpha, alpha],
                );
            }
        }
    }

    /// Draw a note sprite with alpha and rotation, using specific note assets.
    fn draw_note_sprite_ex(&self, gpu: &mut GpuState, assets: &NoteAssets, anim: &str, x: f32, y: f32, scale: f32, alpha: f32, angle: f32) {
        if let Some(frame) = assets.atlas.get_frame(anim, 0) {
            let a = alpha.clamp(0.0, 1.0);
            if angle.abs() > 0.01 {
                gpu.draw_sprite_frame_rotated(
                    frame, assets.tex_w, assets.tex_h,
                    x, y, scale, false, angle, [a, a, a, a],
                );
            } else {
                gpu.draw_sprite_frame(
                    frame, assets.tex_w, assets.tex_h,
                    x, y, scale, false, [a, a, a, a],
                );
            }
        }
    }

    /// Draw a note sprite with custom RGBA color and rotation.
    fn draw_note_sprite_colored(&self, gpu: &mut GpuState, assets: &NoteAssets, anim: &str, x: f32, y: f32, scale: f32, color: [f32; 4], angle: f32) {
        if let Some(frame) = assets.atlas.get_frame(anim, 0) {
            if angle.abs() > 0.01 {
                gpu.draw_sprite_frame_rotated(
                    frame, assets.tex_w, assets.tex_h,
                    x, y, scale, false, angle, color,
                );
            } else {
                gpu.draw_sprite_frame(
                    frame, assets.tex_w, assets.tex_h,
                    x, y, scale, false, color,
                );
            }
        }
    }

    fn draw_strum(&self, gpu: &mut GpuState, lane: usize, player: bool) {
        let (x, y, alpha, angle, scale) = self.strum_pos(lane, player);
        if alpha <= 0.0 { return; }
        let elapsed = if player { self.game.player_confirm[lane] } else { self.game.opponent_confirm[lane] };
        let (anim, frame_idx) = if elapsed > 0.0 {
            let idx = (elapsed / (1000.0 / 24.0)) as usize;
            (CONFIRM_ANIMS[lane], idx)
        } else if player && self.game.keys_held[lane] {
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
            let clamped = if count > 0 { frame_idx.min(count - 1) } else { 0 };
            if let Some(frame) = assets.atlas.get_frame(anim, clamped) {
                let draw_x = cx - frame.frame_w * scale / 2.0;
                let draw_y = cy - frame.frame_h * scale / 2.0;
                let a = alpha.clamp(0.0, 1.0);
                if angle.abs() > 0.01 {
                    gpu.draw_sprite_frame_rotated(
                        frame, assets.tex_w, assets.tex_h,
                        draw_x, draw_y, scale, false, angle, [a, a, a, a],
                    );
                } else {
                    gpu.draw_sprite_frame(
                        frame, assets.tex_w, assets.tex_h,
                        draw_x, draw_y, scale, false, [a, a, a, a],
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
        let assets = match assets { Some(a) => a, None => return };
        let lane = nd.lane;

        let piece = match assets.atlas.get_frame(HOLD_PIECE_ANIMS[lane], 0) {
            Some(f) => f.clone(), None => return,
        };
        let end = match assets.atlas.get_frame(HOLD_END_ANIMS[lane], 0) {
            Some(f) => f.clone(), None => return,
        };

        let tw = assets.tex_w;
        let th = assets.tex_h;
        let (x, sy, sa, _ang, _sc) = self.strum_pos(lane, nd.must_press);
        let a = sa.clamp(0.0, 1.0);
        let color = [1.0, 1.0, 1.0, a];

        let pw = piece.src.w * NOTE_SCALE;
        let ph = piece.src.h * NOTE_SCALE;
        let ew = end.src.w * NOTE_SCALE;
        let eh = end.src.h * NOTE_SCALE;

        let hold_h = (0.45 * nd.sustain_length * self.game.song_speed) as f32;
        let hold_top = y_pos + NOTE_WIDTH * 0.5;

        let clip_y = if nd.was_good_hit {
            sy + NOTE_WIDTH * 0.5
        } else {
            -999.0
        };

        let px = x + (NOTE_WIDTH - pw) / 2.0;
        let ex = x + (NOTE_WIDTH - ew) / 2.0;
        let end_y = hold_top + hold_h - eh;

        let mut cy = hold_top;
        while cy < end_y {
            let tile_h = ph.min(end_y - cy);
            let vis_top = cy.max(clip_y);
            let vis_h = (cy + tile_h) - vis_top;

            if vis_h > 0.5 && vis_top < GAME_H + 100.0 {
                let clip_frac = if vis_top > cy { (vis_top - cy) / tile_h } else { 0.0 };
                gpu.push_texture_region(
                    tw, th,
                    piece.src.x, piece.src.y + piece.src.h * clip_frac,
                    piece.src.w, piece.src.h * (vis_h / tile_h),
                    px, vis_top, pw, vis_h,
                    false, color,
                );
            }
            cy += ph;
        }

        if end_y + eh > clip_y && end_y < GAME_H + 100.0 {
            let vis_top = end_y.max(clip_y);
            let vis_h = (end_y + eh) - vis_top;
            if vis_h > 0.5 {
                let clip_frac = if vis_top > end_y { (vis_top - end_y) / eh } else { 0.0 };
                gpu.push_texture_region(
                    tw, th,
                    end.src.x, end.src.y + end.src.h * clip_frac,
                    end.src.w, end.src.h * (vis_h / eh),
                    ex, vis_top, ew, vis_h,
                    false, color,
                );
            }
        }
    }
}
