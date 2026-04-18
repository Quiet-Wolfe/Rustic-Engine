use rustic_app::screens::title::TitleScreen;
use rustic_app::App;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let cli = parse_cli(&args);

    // Wire up the RL boot switch before any screen runs, so PlayScreen
    // init can pick it up on attach.
    #[cfg(feature = "rl")]
    if cli.rl_enabled {
        let arch_size = cli.rl_arch.as_deref().and_then(rustic_rl::ArchSize::parse);
        if cli.rl_arch.is_some() && arch_size.is_none() {
            log::warn!(
                "rustic-rl: unknown --rl-arch='{}' (valid: tiny|small|large|huge); falling back to default (large)",
                cli.rl_arch.as_deref().unwrap_or("")
            );
        }
        rustic_app::rl_boot::set(rustic_app::rl_boot::RlOpts {
            control_gameplay: cli.rl_control,
            bc_warmup_epochs: cli.rl_bc_epochs,
            arch_size,
        });
        log::info!(
            "rustic-rl: enabled (control={}, bc_epochs={}, config={}, arch={})",
            cli.rl_control,
            cli.rl_bc_epochs,
            cli.rl_config,
            arch_size.map(|a| a.tag()).unwrap_or("default(large)"),
        );
    }
    #[cfg(not(feature = "rl"))]
    if cli.rl_enabled {
        log::warn!(
            "rustic-rl: --rustic-rl=ON passed but this binary was built without the `rl` feature; ignoring"
        );
    }

    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let initial_screen: Box<dyn rustic_app::screen::Screen> = if let Some(song) = cli.rl_song {
        let diff = cli.rl_difficulty.unwrap_or_else(|| "normal".to_string());
        log::info!("rustic-rl: direct-booting into song '{song}' ({diff})");
        Box::new(rustic_app::screens::loading::LoadingScreen::song(
            song, diff, /* play_as_opponent */ false, /* practice_mode */ false,
            /* botplay */ false,
        ))
    } else {
        Box::new(TitleScreen::new())
    };

    let mut app = App::new(initial_screen);
    event_loop.run_app(&mut app).unwrap();
}

#[cfg_attr(not(feature = "rl"), allow(dead_code))]
struct Cli {
    rl_enabled: bool,
    rl_control: bool,
    rl_bc_epochs: usize,
    rl_song: Option<String>,
    rl_difficulty: Option<String>,
    rl_config: String,
    rl_arch: Option<String>,
}

fn parse_cli(args: &[String]) -> Cli {
    let mut rl_enabled = false;
    let mut rl_control = true;
    let mut rl_bc_epochs = 2;
    let mut rl_song = None;
    let mut rl_difficulty = None;
    let mut rl_config = "smol".to_string();
    let mut rl_arch = None;

    for arg in args.iter().skip(1) {
        if let Some(val) = arg.strip_prefix("--rustic-rl=") {
            rl_enabled = matches!(val.to_ascii_uppercase().as_str(), "ON" | "TRUE" | "1");
        } else if let Some(val) = arg.strip_prefix("--rl-config=") {
            rl_config = val.to_string();
        } else if let Some(val) = arg.strip_prefix("--rl-song=") {
            rl_song = Some(val.to_string());
        } else if let Some(val) = arg.strip_prefix("--rl-difficulty=") {
            rl_difficulty = Some(val.to_string());
        } else if let Some(val) = arg.strip_prefix("--rl-mode=") {
            // `agent` = drive gameplay + learn; `record` = human plays, we save demos
            rl_control = !matches!(val.to_ascii_lowercase().as_str(), "record" | "bc" | "demo");
        } else if let Some(val) = arg.strip_prefix("--rl-bc-epochs=") {
            rl_bc_epochs = val.parse().unwrap_or(rl_bc_epochs);
        } else if let Some(val) = arg.strip_prefix("--rl-arch=") {
            rl_arch = Some(val.to_string());
        }
    }

    Cli {
        rl_enabled,
        rl_control,
        rl_bc_epochs,
        rl_song,
        rl_difficulty,
        rl_config,
        rl_arch,
    }
}
