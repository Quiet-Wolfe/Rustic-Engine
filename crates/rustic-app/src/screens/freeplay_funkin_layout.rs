use rustic_render::gpu::{GpuState, GpuTexture};

use super::super::{GAME_H, GAME_W};
use super::CUTOUT_W;

pub(super) fn draw_backing_text_flow(gpu: &mut GpuState, frame: usize, alpha: f32) {
    let messages = ["BIG SHOES", "YEAH", "YES YES YES", "GET IT"];
    let drift = (frame as f32 * 0.62) % 245.0;
    let text_left = 300.0;
    let text_right = 560.0;
    let spacing = 245.0;
    for row in 0..5 {
        let y = 126.0 + row as f32 * 72.0;
        let row_drift = if row % 2 == 0 { drift } else { spacing - drift };
        for col in 0..4 {
            let msg = messages[(row + col) % messages.len()];
            let size = if msg == "YES YES YES" { 27.0 } else { 31.0 };
            let width = estimate_text_width(msg, size);
            let x = text_left - spacing + col as f32 * spacing + row_drift;
            if x < text_left || x + width > text_right {
                continue;
            }
            let color = if (row + col) % 3 == 0 {
                [1.0, 1.0, 1.0, 0.24 * alpha]
            } else {
                [0.88, 0.45, 0.02, 0.28 * alpha]
            };
            gpu.draw_text(msg, x + 2.0, y + 2.0, size, [0.18, 0.12, 0.0, 0.18 * alpha]);
            gpu.draw_text(msg, x, y, size, color);
        }
    }
}

pub(super) fn pink_back_right_edge_at(screen_y: f32) -> f32 {
    // pinkBack image is 524x760; drawn at screen y=-24 scaled to (CUTOUT_W, GAME_H+48).
    // Its opaque right edge slants from image x=393 at top to x=523 at bottom.
    let scale_x = CUTOUT_W / 524.0;
    let image_y = (screen_y + 24.0) * 760.0 / (GAME_H + 48.0);
    scale_x * (393.0 + (523.0 - 393.0) * image_y / 760.0)
}

pub(super) fn draw_orange_bar(gpu: &mut GpuState, card_x: f32, color: [f32; 4]) {
    let top_y = 440.0;
    let bot_y = 515.0;
    let top_right = card_x + pink_back_right_edge_at(top_y);
    let bot_right = card_x + pink_back_right_edge_at(bot_y);
    gpu.push_raw_quad(
        [
            [card_x + 84.0, top_y],
            [top_right, top_y],
            [bot_right, bot_y],
            [card_x + 84.0, bot_y],
        ],
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        color,
    );
    gpu.draw_batch(None);
}

fn estimate_text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.58
}

pub(super) fn draw_difficulty_dots(
    gpu: &mut GpuState,
    dot: Option<&GpuTexture>,
    selected: usize,
    count: usize,
    alpha: f32,
) {
    let start_x = 276.0 - count as f32 * 13.0;
    let y = 188.0;
    if let Some(dot) = dot {
        for i in 0..count {
            let active = i == selected;
            let size = if active { 18.0 } else { 13.0 };
            let offset = (18.0 - size) * 0.5;
            gpu.push_texture_region(
                dot.width as f32,
                dot.height as f32,
                0.0,
                0.0,
                dot.width as f32,
                dot.height as f32,
                start_x + i as f32 * 27.0 + offset,
                y + offset,
                size,
                size,
                false,
                dot_color(active, alpha),
            );
        }
        gpu.draw_batch(Some(dot));
    } else {
        for i in 0..count {
            let active = i == selected;
            let size = if active { 18.0 } else { 13.0 };
            let cx = start_x + i as f32 * 27.0 + 9.0;
            let cy = y + 9.0;
            gpu.push_raw_quad(
                [
                    [cx, cy - size],
                    [cx + size, cy],
                    [cx, cy + size],
                    [cx - size, cy],
                ],
                [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                dot_color(active, alpha),
            );
        }
        gpu.draw_batch(None);
    }
}

fn dot_color(active: bool, alpha: f32) -> [f32; 4] {
    if active {
        [1.0, 1.0, 1.0, alpha]
    } else {
        [0.28, 0.28, 0.28, 0.8 * alpha]
    }
}

pub(super) fn capsule_x(capsule_index: f32, intro: f32) -> f32 {
    270.0 + 45.0 * capsule_index.sin() + GAME_W * (1.0 - intro)
}

pub(super) fn stable_capsule_frame(
    atlas: &rustic_render::sprites::SpriteAtlas,
    anim: &str,
) -> usize {
    atlas.frame_count(anim).saturating_sub(1).min(4)
}

pub(super) fn truncate_for_capsule(name: &str) -> String {
    const MAX: usize = 23;
    if name.chars().count() <= MAX {
        return name.to_string();
    }
    let mut out: String = name.chars().take(MAX.saturating_sub(1)).collect();
    out.push_str("...");
    out
}
