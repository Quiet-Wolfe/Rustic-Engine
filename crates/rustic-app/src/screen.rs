use rustic_audio::AudioEngine;
use rustic_render::gpu::GpuState;
use winit::event::TouchPhase;
use winit::keyboard::KeyCode;

/// Trait for game screens. Each screen is its own module.
pub trait Screen {
    fn init(&mut self, gpu: &GpuState);
    fn handle_key(&mut self, key: KeyCode);
    fn handle_key_release(&mut self, _key: KeyCode) {}
    fn handle_touch(&mut self, _id: u64, _phase: TouchPhase, _x: f64, _y: f64) {}
    fn update(&mut self, dt: f32);
    fn draw(&mut self, gpu: &mut GpuState);
    /// Return a new screen to transition to (e.g. retry).
    fn next_screen(&mut self) -> Option<Box<dyn Screen>> {
        None
    }
    /// Take the shared menu audio engine (freakyMenu) so the next screen can reuse it.
    fn take_audio(&mut self) -> Option<AudioEngine> {
        None
    }
    /// Receive a shared audio engine from the previous screen.
    fn set_audio(&mut self, _audio: AudioEngine) {}
}
