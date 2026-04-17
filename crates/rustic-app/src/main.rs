use rustic_app::screens::title::TitleScreen;
use rustic_app::App;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    parse_rl_flags(&args);

    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = App::new(Box::new(TitleScreen::new()));
    event_loop.run_app(&mut app).unwrap();
}

/// Parse `--rustic-rl=ON|OFF` and `--rl-config=<name>`. When the `rl`
/// feature is disabled these flags are recognized but ignored with a log
/// warning, so invocations don't fail on builds without the harness.
fn parse_rl_flags(args: &[String]) {
    let mut enabled = false;
    let mut config_name = "smol".to_string();

    for arg in args.iter().skip(1) {
        if let Some(val) = arg.strip_prefix("--rustic-rl=") {
            enabled = matches!(val.to_ascii_uppercase().as_str(), "ON" | "TRUE" | "1");
        } else if let Some(val) = arg.strip_prefix("--rl-config=") {
            config_name = val.to_string();
        }
    }

    if !enabled {
        return;
    }

    #[cfg(feature = "rl")]
    {
        let cfg = rustic_rl::Config::load(&config_name);
        log::info!("rustic-rl: enabled (config={config_name}, policy={:?})", cfg.policy);
        // Agent lives in main for now — wiring it into PlayScreen is a
        // follow-up; the skeleton just proves the plumbing links.
        let _agent = rustic_rl::RLAgent::new(cfg);
    }

    #[cfg(not(feature = "rl"))]
    {
        let _ = config_name;
        log::warn!(
            "rustic-rl: --rustic-rl=ON passed but this binary was built without the `rl` feature; ignoring"
        );
    }
}
