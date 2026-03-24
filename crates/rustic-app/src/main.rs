mod screen;
mod screens;

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use rustic_render::gpu::GpuState;

use crate::screen::Screen;
use crate::screens::title::TitleScreen;

const GAME_W: f32 = 1280.0;
const GAME_H: f32 = 720.0;

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    current_screen: Box<dyn Screen>,
    last_frame: Instant,
}

impl App {
    fn new(screen: Box<dyn Screen>) -> Self {
        Self {
            window: None,
            gpu: None,
            current_screen: screen,
            last_frame: Instant::now(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("RusticV2")
            .with_inner_size(winit::dpi::LogicalSize::new(GAME_W, GAME_H));

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        self.window = Some(window.clone());

        let gpu = pollster::block_on(GpuState::new(window.clone(), GAME_W, GAME_H));
        self.current_screen.init(&gpu);
        self.gpu = Some(gpu);
        self.last_frame = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size);
                }
            }

            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    physical_key: PhysicalKey::Code(key),
                    state,
                    repeat: false,
                    ..
                },
                ..
            } => match state {
                ElementState::Pressed => {
                    self.current_screen.handle_key(key);
                }
                ElementState::Released => {
                    self.current_screen.handle_key_release(key);
                }
            },

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32();
                self.last_frame = now;

                self.current_screen.update(dt);

                // Screen transitions (retry, etc.)
                if let Some(mut next) = self.current_screen.next_screen() {
                    // Pass shared audio (freakyMenu) between menu screens
                    if let Some(audio) = self.current_screen.take_audio() {
                        next.set_audio(audio);
                    }
                    if let Some(gpu) = &self.gpu {
                        next.init(gpu);
                    }
                    self.current_screen = next;
                }

                if let Some(gpu) = &mut self.gpu {
                    self.current_screen.draw(gpu);
                }
            }

            _ => {}
        }

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = App::new(Box::new(TitleScreen::new()));
    event_loop.run_app(&mut app).unwrap();
}
