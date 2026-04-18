use rustic_render::gpu::{GpuState, GpuTexture};

use super::super::{GAME_H, GAME_W};

pub(super) fn draw_backing_text_flow(gpu: &mut GpuState, frame: usize, alpha: f32) {
    let messages = ["BIG SHOES", "YEAH", "YES YES YES", "GET IT"];
    let drift = (frame as f32 * 0.62) % 245.0;
    let text_left = 34.0;
    let text_right = 500.0;
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

pub(super) fn draw_transition_wedge(gpu: &mut GpuState, cutout_w: f32, alpha: f32) {
    let seam_x = cutout_w * 0.74;
    let positions = [
        [seam_x - 2.0, 0.0],
        [seam_x + 150.0, 0.0],
        [seam_x - 88.0, GAME_H],
        [seam_x - 2.0, GAME_H],
    ];
    gpu.push_raw_quad(
        positions,
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        [1.0, 0.85, 0.39, alpha],
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
