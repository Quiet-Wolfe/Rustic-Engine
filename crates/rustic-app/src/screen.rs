use rustic_render::gpu::GpuState;
use winit::keyboard::KeyCode;

/// Trait for game screens. Each screen is its own module.
pub trait Screen {
    fn init(&mut self, gpu: &GpuState);
    fn handle_key(&mut self, key: KeyCode);
    fn update(&mut self, dt: f32);
    fn draw(&mut self, gpu: &mut GpuState);
}
