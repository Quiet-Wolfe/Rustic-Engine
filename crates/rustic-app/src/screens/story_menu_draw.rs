use super::StoryMenuScreen;

impl StoryMenuScreen {
    pub(super) fn draw_inner(&mut self, gpu: &mut rustic_render::gpu::GpuState) {
        if self.pending_selection_assets {
            self.reload_selection_assets(gpu);
        }
        if !gpu.begin_frame() {
            return;
        }

        gpu.push_colored_quad(0.0, 56.0, 1280.0, 386.0, [0.95, 0.81, 0.32, 1.0]);
        gpu.draw_batch(None);

        if let Some(bg) = &self.current_background {
            let bw = bg.width as f32;
            let bh = bg.height as f32;
            let scale = (386.0f32 / bh).max(1.0);
            let draw_w = bw * scale;
            let draw_h = bh * scale;
            let x = (1280.0 - draw_w) * 0.5;
            gpu.push_texture_region(
                bw, bh, 0.0, 0.0, bw, bh,
                x, 56.0, draw_w, draw_h,
                false, [1.0, 1.0, 1.0, 1.0],
            );
            gpu.draw_batch(Some(bg));
        }

        gpu.push_colored_quad(0.0, 0.0, 1280.0, 56.0, [0.0, 0.0, 0.0, 1.0]);
        gpu.draw_batch(None);

        let score_text = format!("WEEK SCORE: {}", self.displayed_score.floor() as i32);
        gpu.draw_text(&score_text, 10.0, 10.0, 32.0, [1.0, 1.0, 1.0, 1.0]);

        if let Some(week) = self.current_week() {
            gpu.draw_text(&week.week.week_name.to_uppercase(), 1040.0, 10.0, 32.0, [1.0, 1.0, 1.0, 0.7]);
        }

        for character in self.current_characters.iter().flatten() {
            character.draw(gpu);
        }

        for (idx, week) in self.weeks.iter().enumerate() {
            let y = 480.0 + (idx as f32 - self.selection_lerp) * 120.0;
            if !(360.0..=720.0).contains(&y) {
                continue;
            }
            let alpha = if idx == self.selected_week && !week.locked { 1.0 } else { 0.6 };
            let flashing = self.confirming
                && idx == self.selected_week
                && ((self.confirm_timer * 60.0 * 6.0) as i32 % 2 == 0);
            let color = if flashing {
                [0.2, 1.0, 1.0, 1.0]
            } else {
                [1.0, 1.0, 1.0, alpha]
            };
            if let Some(texture) = &self.week_images[idx] {
                let draw_x = (1280.0 - texture.width as f32) * 0.5;
                gpu.push_texture_region(
                    texture.width as f32,
                    texture.height as f32,
                    0.0,
                    0.0,
                    texture.width as f32,
                    texture.height as f32,
                    draw_x,
                    y,
                    texture.width as f32,
                    texture.height as f32,
                    false,
                    color,
                );
                gpu.draw_batch(Some(texture));
            } else {
                gpu.draw_text(&week.week.file_name.to_uppercase(), 480.0, y + 20.0, 38.0, color);
            }

            if week.locked {
                if let Some(ui) = &self.ui {
                    if let Some(frame) = ui.atlas.get_frame("lock", 0) {
                        gpu.draw_sprite_frame(
                            frame,
                            ui.tex_w,
                            ui.tex_h,
                            900.0,
                            y + 20.0,
                            1.0,
                            false,
                            [1.0, 1.0, 1.0, 1.0],
                        );
                        gpu.draw_batch(Some(&ui.texture));
                    }
                }
            }
        }

        if let Some(ui) = &self.ui {
            if let Some(frame) = ui.atlas.get_frame(&ui.left_arrow.current_anim, ui.left_arrow.frame_index) {
                gpu.draw_sprite_frame(frame, ui.tex_w, ui.tex_h, 850.0, 510.0, 1.0, false, [1.0, 1.0, 1.0, 1.0]);
                gpu.draw_batch(Some(&ui.texture));
            }
            if let Some(frame) = ui.atlas.get_frame(&ui.right_arrow.current_anim, ui.right_arrow.frame_index) {
                gpu.draw_sprite_frame(frame, ui.tex_w, ui.tex_h, 1226.0, 510.0, 1.0, false, [1.0, 1.0, 1.0, 1.0]);
                gpu.draw_batch(Some(&ui.texture));
            }
        }

        if let Some(diff) = &self.current_difficulty_texture {
            gpu.push_texture_region(
                diff.width as f32,
                diff.height as f32,
                0.0,
                0.0,
                diff.width as f32,
                diff.height as f32,
                930.0,
                self.difficulty_y,
                diff.width as f32,
                diff.height as f32,
                false,
                [1.0, 1.0, 1.0, self.difficulty_alpha],
            );
            gpu.draw_batch(Some(diff));
        } else {
            gpu.draw_text(
                &format!("< {} >", self.current_difficulty().to_uppercase()),
                900.0,
                self.difficulty_y,
                26.0,
                [1.0, 1.0, 1.0, self.difficulty_alpha],
            );
        }

        if let Some(track_header) = &self.track_header {
            let x = 90.0;
            let y = 481.0;
            gpu.push_texture_region(
                track_header.width as f32,
                track_header.height as f32,
                0.0,
                0.0,
                track_header.width as f32,
                track_header.height as f32,
                x,
                y,
                track_header.width as f32,
                track_header.height as f32,
                false,
                [1.0, 1.0, 1.0, 1.0],
            );
            gpu.draw_batch(Some(track_header));
        } else {
            gpu.draw_text("TRACKS", 120.0, 505.0, 30.0, [1.0, 1.0, 1.0, 1.0]);
        }

        if let Some(week) = self.current_week() {
            let track_list = week
                .week
                .songs
                .iter()
                .map(|song| song.name.to_uppercase())
                .collect::<Vec<_>>()
                .join("\n");
            gpu.draw_text(&track_list, 100.0, 560.0, 24.0, [0.9, 0.34, 0.47, 1.0]);
            gpu.draw_text(&week.week.story_name.to_uppercase(), 920.0, 60.0, 30.0, [1.0, 1.0, 1.0, 0.85]);
        }

        if let Some(reset_modal) = &mut self.reset_modal {
            reset_modal.draw(gpu);
        }
        if let Some(gameplay_changers) = &self.gameplay_changers {
            gameplay_changers.draw(gpu);
        }

        crate::debug_overlay::finish_frame(gpu);
    }
}
