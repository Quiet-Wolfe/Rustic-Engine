use rustic_render::gpu::GpuState;

use super::pause::{PauseMenuItem, PauseMenuMode};
use super::{PlayScreen, GAME_H, GAME_W};

impl PlayScreen {
    /// Draw the pause menu overlay.
    pub(super) fn draw_pause(&self, gpu: &mut GpuState) {
        if let Some(options_menu) = &self.options_menu {
            options_menu.draw(gpu);
            return;
        }
        let Some(pause_menu) = &self.pause_menu else {
            return;
        };

        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        let dark_gray = [0.5, 0.5, 0.5, 1.0];

        gpu.push_colored_quad(
            0.0,
            0.0,
            GAME_W,
            GAME_H,
            [0.0, 0.0, 0.0, pause_menu.overlay_alpha],
        );
        gpu.draw_batch(None);

        let box_x = 80.0;
        let box_y = 160.0;
        let box_w = 400.0;
        let box_h = 340.0;
        gpu.push_colored_quad(box_x, box_y, box_w, box_h, [0.0, 0.0, 0.0, 0.55]);
        gpu.draw_batch(None);

        let song_display = self.song_name.replace('-', " ");
        let info_alpha = ((pause_menu.timer - 0.3) / 0.4).clamp(0.0, 1.0);
        let diff_alpha = ((pause_menu.timer - 0.5) / 0.4).clamp(0.0, 1.0);
        let blue_alpha = ((pause_menu.timer - 0.7) / 0.4).clamp(0.0, 1.0);
        let info_x = GAME_W - 300.0 + (1.0 - info_alpha) * 120.0;
        let diff_x = GAME_W - 300.0 + (1.0 - diff_alpha) * 120.0;
        let blue_x = GAME_W - 300.0 + (1.0 - blue_alpha) * 120.0;
        gpu.draw_text(
            &song_display,
            info_x,
            24.0,
            28.0,
            [1.0, 1.0, 1.0, info_alpha],
        );
        gpu.draw_text(
            &self.difficulty.to_uppercase(),
            diff_x,
            56.0,
            22.0,
            [0.7, 0.7, 0.7, diff_alpha],
        );

        let blueballed = format!("Blueballed: {}", self.death_counter);
        gpu.draw_text(&blueballed, blue_x, 84.0, 20.0, [0.5, 0.5, 0.5, blue_alpha]);

        let title = match pause_menu.mode {
            PauseMenuMode::Main => "PAUSED",
            PauseMenuMode::Difficulty => "SELECT DIFFICULTY",
        };
        gpu.draw_text(title, box_x + 20.0, box_y + 16.0, 32.0, white);

        let item_start_y = box_y + 70.0;
        let item_height = 38.0;
        match pause_menu.mode {
            PauseMenuMode::Main => self.draw_pause_main_items(
                gpu,
                pause_menu,
                box_x,
                box_w,
                item_start_y,
                item_height,
                black,
                white,
            ),
            PauseMenuMode::Difficulty => self.draw_pause_difficulties(
                gpu,
                pause_menu,
                box_x,
                box_w,
                item_start_y,
                item_height,
                black,
                white,
            ),
        }

        let hint = match pause_menu.mode {
            PauseMenuMode::Main => "ESC: Resume  ENTER: Select",
            PauseMenuMode::Difficulty => "ESC: Back",
        };
        gpu.draw_text(hint, box_x + 20.0, box_y + box_h - 32.0, 18.0, dark_gray);
    }

    fn draw_pause_main_items(
        &self,
        gpu: &mut GpuState,
        pause_menu: &super::pause::PauseMenuState,
        box_x: f32,
        box_w: f32,
        item_start_y: f32,
        item_height: f32,
        black: [f32; 4],
        white: [f32; 4],
    ) {
        let items = pause_menu.main_items();
        let song_length_ms = self.get_song_length_ms();
        for (i, item) in items.iter().enumerate() {
            let y = item_start_y + i as f32 * item_height;
            let is_selected = i == pause_menu.selected;
            draw_selection(gpu, is_selected, box_x, box_w, y, item_height);
            let color = if is_selected { black } else { white };
            let prefix = if is_selected { "> " } else { "  " };
            let label = if *item == PauseMenuItem::SkipTime {
                format!(
                    "{}Skip Time  < {} >",
                    prefix,
                    pause_menu.format_skip_time(song_length_ms)
                )
            } else {
                format!("{}{}", prefix, item.label())
            };
            gpu.draw_text(&label, box_x + 24.0, y, 26.0, color);
        }
    }

    fn draw_pause_difficulties(
        &self,
        gpu: &mut GpuState,
        pause_menu: &super::pause::PauseMenuState,
        box_x: f32,
        box_w: f32,
        item_start_y: f32,
        item_height: f32,
        black: [f32; 4],
        white: [f32; 4],
    ) {
        for (i, diff) in pause_menu.difficulty_choices.iter().enumerate() {
            let y = item_start_y + i as f32 * item_height;
            let is_selected = i == pause_menu.selected;
            draw_selection(gpu, is_selected, box_x, box_w, y, item_height);
            let color = if is_selected { black } else { white };
            let prefix = if is_selected { "> " } else { "  " };
            gpu.draw_text(
                &format!("{}{}", prefix, diff.to_uppercase()),
                box_x + 24.0,
                y,
                26.0,
                color,
            );
        }

        let back_idx = pause_menu.difficulty_choices.len();
        let back_y = item_start_y + back_idx as f32 * item_height;
        let is_back_selected = pause_menu.selected == back_idx;
        draw_selection(gpu, is_back_selected, box_x, box_w, back_y, item_height);
        let color = if is_back_selected { black } else { white };
        let prefix = if is_back_selected { "> " } else { "  " };
        gpu.draw_text(
            &format!("{}BACK", prefix),
            box_x + 24.0,
            back_y,
            26.0,
            color,
        );
    }
}

fn draw_selection(
    gpu: &mut GpuState,
    selected: bool,
    box_x: f32,
    box_w: f32,
    y: f32,
    item_height: f32,
) {
    if !selected {
        return;
    }
    gpu.push_colored_quad(
        box_x + 12.0,
        y - 4.0,
        box_w - 24.0,
        item_height - 4.0,
        [1.0, 1.0, 1.0, 0.9],
    );
    gpu.draw_batch(None);
}
