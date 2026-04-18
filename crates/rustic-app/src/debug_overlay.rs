use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use rustic_render::gpu::GpuState;

use crate::{settings, GAME_W};

#[derive(Debug)]
struct FpsState {
    last_sample: Instant,
    fps: f32,
}

impl Default for FpsState {
    fn default() -> Self {
        Self {
            last_sample: Instant::now(),
            fps: 0.0,
        }
    }
}

static FPS_STATE: OnceLock<Mutex<FpsState>> = OnceLock::new();

pub fn finish_frame(gpu: &mut GpuState) {
    if settings::fps_counter_enabled() {
        draw_fps_counter(gpu);
    }
    gpu.end_frame();
}

fn draw_fps_counter(gpu: &mut GpuState) {
    let fps = sample_fps();
    let text = format!("{fps:.0} FPS");
    let x = GAME_W - 110.0;
    gpu.push_colored_quad(x - 8.0, 8.0, 100.0, 32.0, [0.0, 0.0, 0.0, 0.55]);
    gpu.draw_batch(None);
    gpu.draw_text(&text, x, 13.0, 20.0, [0.2, 1.0, 0.45, 1.0]);
}

fn sample_fps() -> f32 {
    let now = Instant::now();
    let state = FPS_STATE.get_or_init(|| Mutex::new(FpsState::default()));
    let Ok(mut state) = state.lock() else {
        return 0.0;
    };

    let dt = now.duration_since(state.last_sample).as_secs_f32();
    state.last_sample = now;
    if dt > 0.0 {
        let instant_fps = 1.0 / dt;
        state.fps = if state.fps <= 0.0 {
            instant_fps
        } else {
            state.fps + (instant_fps - state.fps) * 0.15
        };
    }
    state.fps
}
