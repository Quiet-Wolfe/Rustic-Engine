pub mod debug_overlay;
pub mod screen;
pub mod screens;
pub mod settings;

#[cfg(feature = "rl")]
pub mod rl_boot;

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use rustic_core::prefs::Preferences;
use rustic_render::gpu::GpuState;

use crate::screen::Screen;

pub const GAME_W: f32 = 1280.0;
pub const GAME_H: f32 = 720.0;

pub struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    current_screen: Box<dyn Screen>,
    last_frame: Instant,
}

impl App {
    pub fn new(screen: Box<dyn Screen>) -> Self {
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

        let prefs = Preferences::load();
        crate::settings::apply_preferences(&prefs);
        let mut attrs = Window::default_attributes()
            .with_title("RusticV2")
            .with_inner_size(winit::dpi::LogicalSize::new(GAME_W, GAME_H));
        if prefs.fullscreen {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }

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
                event:
                    KeyEvent {
                        physical_key,
                        logical_key,
                        state,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                // Android back button → Escape
                if logical_key == Key::Named(NamedKey::GoBack)
                    || logical_key == Key::Named(NamedKey::BrowserBack)
                {
                    if state == ElementState::Pressed {
                        self.current_screen
                            .handle_key(winit::keyboard::KeyCode::Escape);
                    }
                } else if let PhysicalKey::Code(key) = physical_key {
                    match state {
                        ElementState::Pressed => {
                            self.current_screen.handle_key(key);
                        }
                        ElementState::Released => {
                            self.current_screen.handle_key_release(key);
                        }
                    }
                }
            }

            WindowEvent::Touch(touch) => {
                if let Some(gpu) = &self.gpu {
                    if let Some((gx, gy)) = gpu.physical_to_game(touch.location.x, touch.location.y)
                    {
                        self.current_screen.handle_touch(
                            touch.id,
                            touch.phase,
                            gx as f64,
                            gy as f64,
                        );
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                crate::settings::sleep_until_next_frame(self.last_frame);
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f64().min(0.05) as f32; // f64 precision, clamp to 50ms max
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

// === Android entry point ===

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("RusticV2"),
    );

    log::info!("RusticV2 android_main starting");

    use winit::platform::android::EventLoopBuilderExtAndroid;
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_android_app(app)
        .build()
        .unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = App::new(Box::new(crate::screens::title::TitleScreen::new()));
    event_loop.run_app(&mut app).unwrap();
}
