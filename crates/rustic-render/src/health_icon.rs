use std::path::Path;

use crate::gpu::{GpuState, GpuTexture};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Neutral,
    Losing,
    Winning,
}

pub struct HealthIcon {
    texture: GpuTexture,
    frame_count: usize,
    frame_width: u32,
    frame_height: u32,
    current_frame: usize,
    is_player: bool,
}

impl HealthIcon {
    pub fn load(gpu: &GpuState, path: &Path, is_player: bool) -> Self {
        let texture = gpu.load_texture_from_path(path);
        let frame_count = ((texture.width as f32 / texture.height as f32).round() as usize).max(1);
        let frame_width = (texture.width / frame_count as u32).max(1);
        let frame_height = texture.height.max(1);

        Self {
            texture,
            frame_count,
            frame_width,
            frame_height,
            current_frame: 0,
            is_player,
        }
    }

    pub fn set_state(&mut self, state: IconState) {
        self.current_frame = match (state, self.frame_count) {
            (IconState::Winning, 3) => 2,
            (IconState::Losing, frames) if frames >= 2 => 1,
            _ => 0,
        };
    }

    pub fn draw(&self, gpu: &mut GpuState, x: f32, y: f32, size: f32, color: [f32; 4]) {
        let frame_x = self.current_frame as f32 * self.frame_width as f32;
        gpu.push_texture_region(
            self.texture.width as f32,
            self.texture.height as f32,
            frame_x,
            0.0,
            self.frame_width as f32,
            self.frame_height as f32,
            x,
            y,
            size,
            size,
            self.is_player,
            color,
        );
        gpu.draw_batch(Some(&self.texture));
    }
}
