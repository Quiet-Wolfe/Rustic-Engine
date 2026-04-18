use rustic_render::gpu::GpuState;
use rustic_render::health_icon::IconState;

use super::super::{FreeplayScreen, DIFFICULTIES, GAME_H, GAME_W};
use super::freeplay_funkin_assets::{CAPSULE_SELECTED, CAPSULE_UNSELECTED};
use super::{CUTOUT_W, DJ_X, DJ_Y};

impl FreeplayScreen {
    pub(in crate::screens::freeplay) fn draw_funkin(&mut self, gpu: &mut GpuState) {
        if !gpu.begin_frame() {
            return;
        }

        self.draw_funkin_background(gpu);
        self.draw_funkin_dj(gpu);
        self.draw_funkin_album(gpu);
        self.draw_funkin_capsules(gpu);
        self.draw_funkin_hud(gpu);
        self.draw_funkin_overlays(gpu);

        crate::debug_overlay::finish_frame(gpu);
    }

    fn draw_funkin_background(&self, gpu: &mut GpuState) {
        let c = self.bg_color;
        let intro = self.funkin_ui.intro_amount();
        let confirm = self.funkin_ui.confirm_amount();
        let card_x = -CUTOUT_W * (1.0 - intro);
        gpu.push_colored_quad(
            0.0,
            0.0,
            GAME_W,
            GAME_H,
            [c[0] * 0.35, c[1] * 0.35, c[2] * 0.35, 1.0],
        );
        if let Some(pink) = &self.funkin_ui.pink_back {
            let card_color = [
                1.0 - confirm * 0.72,
                0.85 - confirm * 0.74,
                0.39 - confirm * 0.21,
                1.0,
            ];
            gpu.push_texture_region(
                pink.width as f32,
                pink.height as f32,
                0.0,
                0.0,
                pink.width as f32,
                pink.height as f32,
                card_x,
                -24.0,
                CUTOUT_W,
                GAME_H + 48.0,
                false,
                card_color,
            );
        } else {
            gpu.push_colored_quad(card_x, 0.0, CUTOUT_W, GAME_H, [1.0, 0.85, 0.39, 1.0]);
        }
        gpu.push_colored_quad(card_x + 84.0, 440.0, CUTOUT_W, 75.0, [1.0, 0.85, 0.0, 1.0]);
        gpu.push_colored_quad(card_x, 440.0, 100.0, 75.0, [1.0, 0.83, 0.0, 1.0]);
        gpu.draw_batch(None);

        if let Some(bg) = &self.funkin_ui.background {
            let scale = (GAME_H / bg.height as f32).max((GAME_W * 0.55) / bg.width as f32);
            let w = bg.width as f32 * scale;
            let h = bg.height as f32 * scale;
            let final_x = GAME_W * 0.38;
            let x = final_x + (GAME_W - final_x) * (1.0 - intro);
            let y = (GAME_H - h) * 0.5;
            gpu.push_texture_region(
                bg.width as f32,
                bg.height as f32,
                0.0,
                0.0,
                bg.width as f32,
                bg.height as f32,
                x,
                y,
                w,
                h,
                false,
                [
                    1.0 - confirm * 0.55,
                    1.0 - confirm * 0.55,
                    1.0 - confirm * 0.55,
                    1.0,
                ],
            );
            gpu.draw_batch(Some(bg));
        } else if let Some(bg) = &self.bg_tex {
            let bw = bg.width as f32;
            let bh = bg.height as f32;
            gpu.push_texture_region(
                bw,
                bh,
                0.0,
                0.0,
                bw,
                bh,
                (GAME_W - bw) / 2.0,
                (GAME_H - bh) / 2.0,
                bw,
                bh,
                false,
                [c[0], c[1], c[2], 0.45],
            );
            gpu.draw_batch(Some(bg));
        }

        if let Some(glow) = &self.funkin_ui.card_glow {
            let alpha = ((1.0 - intro) * 0.7 + confirm * 0.65).clamp(0.0, 0.7);
            if alpha > 0.01 {
                gpu.push_texture_region(
                    glow.width as f32,
                    glow.height as f32,
                    0.0,
                    0.0,
                    glow.width as f32,
                    glow.height as f32,
                    card_x - 30.0,
                    -30.0,
                    CUTOUT_W,
                    GAME_H + 48.0,
                    false,
                    [1.0, 1.0, 1.0, alpha],
                );
                gpu.draw_batch(Some(glow));
            }
        }
    }

    fn draw_funkin_dj(&self, gpu: &mut GpuState) {
        if let Some(dj) = &self.funkin_ui.dj {
            dj.draw(gpu, DJ_X, DJ_Y, 1.0, self.funkin_ui.intro_amount());
        } else {
            gpu.draw_text("BF", 140.0, 235.0, 120.0, [1.0, 1.0, 1.0, 0.8]);
            gpu.draw_text("DJ", 170.0, 340.0, 120.0, [1.0, 1.0, 1.0, 0.8]);
        }
    }

    fn draw_funkin_album(&self, gpu: &mut GpuState) {
        let Some(&song_idx) = self.filtered.get(self.cur_selected) else {
            return;
        };
        let Some(album) = self.funkin_ui.album_for_song(&self.songs[song_idx].song_id) else {
            return;
        };
        let hud = self.funkin_ui.hud_amount();
        let x = 910.0 + 330.0 * (1.0 - hud);
        let y = 205.0;
        gpu.push_colored_quad(
            x + 12.0,
            y + 16.0,
            262.0,
            262.0,
            [0.0, 0.0, 0.0, 0.38 * hud],
        );
        gpu.draw_batch(None);
        gpu.push_texture_region(
            album.art.width as f32,
            album.art.height as f32,
            0.0,
            0.0,
            album.art.width as f32,
            album.art.height as f32,
            x,
            y,
            262.0,
            262.0,
            false,
            [1.0, 1.0, 1.0, hud],
        );
        gpu.draw_batch(Some(&album.art));

        if let Some(title) = &album.title {
            let scale = (300.0 / title.texture.width as f32).min(1.0);
            let switching = self.funkin_ui.album_switch_timer.is_some();
            let anim = if switching && title.atlas.has_anim("switch") {
                "switch"
            } else {
                "idle"
            };
            let frame_idx = self
                .funkin_ui
                .album_switch_timer
                .map(|timer| (timer * 24.0) as usize)
                .unwrap_or(self.funkin_ui.capsule_frame)
                % title.atlas.frame_count(anim).max(1);
            if let Some(frame) = title.atlas.get_frame(anim, frame_idx) {
                let bob = (self.funkin_ui.capsule_frame as f32 * 0.18).sin() * 2.0;
                gpu.draw_sprite_frame(
                    frame,
                    title.texture.width as f32,
                    title.texture.height as f32,
                    x - 18.0 + album.title_offset[0] * scale,
                    y + 288.0 + album.title_offset[1] * scale + bob,
                    scale,
                    false,
                    [1.0, 1.0, 1.0, hud],
                );
                gpu.draw_batch(Some(&title.texture));
            }
        }
    }

    fn draw_funkin_capsules(&mut self, gpu: &mut GpuState) {
        let draw_dist = 5;
        let spacing = 116.0;
        let intro = self.funkin_ui.hud_amount();

        if let Some(capsule) = &self.funkin_ui.capsule {
            let tex_w = capsule.texture.width as f32;
            let tex_h = capsule.texture.height as f32;
            for (i, _) in self.filtered.iter().enumerate() {
                let target_y = i as f32 - self.lerp_selected;
                if target_y.abs() > draw_dist as f32 {
                    continue;
                }
                let capsule_index = target_y + 1.0;
                let selected = i == self.cur_selected;
                let anim = if selected {
                    CAPSULE_SELECTED
                } else {
                    CAPSULE_UNSELECTED
                };
                let frame_idx = stable_capsule_frame(&capsule.atlas, anim);
                if let Some(frame) = capsule.atlas.get_frame(anim, frame_idx) {
                    let x = capsule_x(capsule_index, intro);
                    let y = 120.0 + capsule_index * spacing;
                    let scale = if selected { 0.82 } else { 0.8 };
                    let alpha = if selected { 1.0 } else { 0.55 } * intro;
                    gpu.draw_sprite_frame(
                        frame,
                        tex_w,
                        tex_h,
                        x,
                        y,
                        scale,
                        false,
                        [1.0, 1.0, 1.0, alpha],
                    );
                }
            }
            gpu.draw_batch(Some(&capsule.texture));
        } else {
            for (i, _) in self.filtered.iter().enumerate() {
                let target_y = i as f32 - self.lerp_selected;
                if target_y.abs() > draw_dist as f32 {
                    continue;
                }
                let selected = i == self.cur_selected;
                let capsule_index = target_y + 1.0;
                let x = capsule_x(capsule_index, intro);
                let y = 128.0 + capsule_index * spacing;
                let alpha = if selected { 0.95 } else { 0.45 } * intro;
                gpu.push_colored_quad(
                    x,
                    y,
                    if selected { 535.0 } else { 500.0 },
                    68.0,
                    [1.0, 1.0, 1.0, alpha],
                );
            }
            gpu.draw_batch(None);
        }

        for (i, &song_idx) in self.filtered.iter().enumerate() {
            let target_y = i as f32 - self.lerp_selected;
            if target_y.abs() > draw_dist as f32 {
                continue;
            }

            let selected = i == self.cur_selected;
            let capsule_index = target_y + 1.0;
            let x = capsule_x(capsule_index, intro);
            let y = 120.0 + capsule_index * spacing;
            let alpha = if selected { 1.0 } else { 0.58 } * intro;
            let song = &mut self.songs[song_idx];
            let label_size = if selected { 29.0 } else { 25.0 };
            let label_x = x + 155.0;
            let label_y = y + if selected { 36.0 } else { 39.0 };
            let label = truncate_for_capsule(&song.name);
            let text_color = if selected {
                [0.92, 1.0, 1.0, alpha]
            } else {
                [0.72, 0.78, 0.82, alpha]
            };
            gpu.draw_text(&label, label_x, label_y, label_size, text_color);

            if let Some(icon) = &mut song.icon {
                icon.set_state(if selected {
                    IconState::Winning
                } else {
                    IconState::Neutral
                });
                icon.draw(
                    gpu,
                    x - 58.0,
                    y + 20.0,
                    if selected { 72.0 } else { 64.0 },
                    [1.0, 1.0, 1.0, alpha],
                );
            }
        }
    }

    fn draw_funkin_hud(&self, gpu: &mut GpuState) {
        let hud = self.funkin_ui.hud_amount();
        let overhang_y = -164.0 + 64.0 * hud;
        gpu.push_colored_quad(0.0, overhang_y, GAME_W, 164.0, [0.0, 0.0, 0.0, 1.0]);
        gpu.draw_batch(None);

        let diff = DIFFICULTIES[self.cur_difficulty];
        if let Some(tex) = self.funkin_ui.difficulty_texture(diff) {
            let scale = 0.74;
            let w = tex.width as f32 * scale;
            let h = tex.height as f32 * scale;
            gpu.push_texture_region(
                tex.width as f32,
                tex.height as f32,
                0.0,
                0.0,
                tex.width as f32,
                tex.height as f32,
                66.0,
                58.0 - 90.0 * (1.0 - hud),
                w,
                h,
                false,
                [1.0, 1.0, 1.0, hud],
            );
            gpu.draw_batch(Some(tex));
        } else {
            gpu.draw_text(
                &format!("< {} >", diff.to_uppercase()),
                116.0,
                42.0,
                30.0,
                [1.0, 1.0, 1.0, 1.0],
            );
        }

        gpu.draw_text(
            "FREEPLAY",
            8.0,
            8.0 - 100.0 * (1.0 - hud),
            48.0,
            [1.0, 1.0, 1.0, hud],
        );
        gpu.draw_text(
            "ORIGINAL OST",
            940.0,
            10.0 - 100.0 * (1.0 - hud),
            44.0,
            [1.0, 1.0, 1.0, hud],
        );
        if let Some(highscore) = &self.funkin_ui.highscore {
            gpu.push_texture_region(
                highscore.width as f32,
                highscore.height as f32,
                0.0,
                0.0,
                289.0,
                43.0,
                860.0 + 460.0 * (1.0 - hud),
                72.0,
                260.0,
                39.0,
                false,
                [1.0, 1.0, 1.0, hud],
            );
            gpu.draw_batch(Some(highscore));
        }
        gpu.draw_text(
            &self.current_score_text(),
            820.0 + 460.0 * (1.0 - hud),
            116.0,
            21.0,
            [1.0, 1.0, 1.0, hud],
        );

        let status = if self.previewing_song.is_some() {
            "SPACE Stop Preview"
        } else {
            "SPACE Preview"
        };
        let count_text = if self.search.is_empty() {
            format!(
                "{} songs  |  {}  |  ENTER Play  |  CTRL Changers  |  TAB Opponent: {}",
                self.filtered.len(),
                status,
                if self.play_as_opponent { "ON" } else { "OFF" }
            )
        } else {
            format!(
                "{}/{} songs  |  ESC Clear Search  |  ENTER Play  |  TAB Opponent: {}",
                self.filtered.len(),
                self.songs.len(),
                if self.play_as_opponent { "ON" } else { "OFF" }
            )
        };
        gpu.draw_text(
            &count_text,
            28.0,
            GAME_H - 44.0 + 50.0 * (1.0 - hud),
            18.0,
            [1.0, 1.0, 1.0, 0.92 * hud],
        );

        if !self.search.is_empty() {
            gpu.push_colored_quad(610.0, 96.0, 475.0, 42.0, [0.0, 0.0, 0.0, 0.62]);
            gpu.draw_batch(None);
            gpu.draw_text(
                &format!("Search: {}_", self.search),
                625.0,
                106.0,
                22.0,
                [1.0, 0.92, 0.34, 1.0],
            );
        }

        draw_difficulty_dots(gpu, self.cur_difficulty, DIFFICULTIES.len(), hud);
    }

    fn draw_funkin_overlays(&mut self, gpu: &mut GpuState) {
        if cfg!(target_os = "android") {
            let strip_top = 86.0;
            let strip_h = GAME_H - 170.0;
            let letter_h = strip_h / 26.0;
            gpu.push_colored_quad(8.0, strip_top, 30.0, strip_h, [0.0, 0.0, 0.0, 0.35]);
            gpu.draw_batch(None);
            for i in 0..26u8 {
                let letter = (b'A' + i) as char;
                let y = strip_top + i as f32 * letter_h;
                gpu.draw_text(
                    &letter.to_string(),
                    15.0,
                    y,
                    letter_h.min(18.0),
                    [1.0, 1.0, 1.0, 0.7],
                );
            }
        }

        if let Some(gameplay_changers) = &self.gameplay_changers {
            gameplay_changers.draw(gpu);
        }
        if let Some(reset_modal) = &mut self.reset_modal {
            reset_modal.draw(gpu);
        }
    }
}

fn draw_difficulty_dots(gpu: &mut GpuState, selected: usize, count: usize, alpha: f32) {
    let start_x = 96.0;
    let y = 162.0;
    for i in 0..count {
        let active = i == selected;
        let size = if active { 16.0 } else { 10.0 };
        let offset = if active { 0.0 } else { 3.0 };
        gpu.push_colored_quad(
            start_x + i as f32 * 26.0,
            y + offset,
            size,
            size,
            if active {
                [1.0, 1.0, 1.0, alpha]
            } else {
                [0.0, 0.0, 0.0, 0.55 * alpha]
            },
        );
    }
    gpu.draw_batch(None);
}

fn capsule_x(capsule_index: f32, intro: f32) -> f32 {
    270.0 + 45.0 * capsule_index.sin() + GAME_W * (1.0 - intro)
}

fn stable_capsule_frame(atlas: &rustic_render::sprites::SpriteAtlas, anim: &str) -> usize {
    atlas.frame_count(anim).saturating_sub(1).min(4)
}

fn truncate_for_capsule(name: &str) -> String {
    const MAX: usize = 23;
    if name.chars().count() <= MAX {
        return name.to_string();
    }
    let mut out: String = name.chars().take(MAX.saturating_sub(1)).collect();
    out.push_str("...");
    out
}
