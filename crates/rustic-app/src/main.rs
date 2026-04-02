use rustic_app::screens::title::TitleScreen;
use rustic_app::App;

fn main() {
    env_logger::init();

    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = App::new(Box::new(TitleScreen::new()));
    event_loop.run_app(&mut app).unwrap();
}
