use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rustic_core::prefs::Preferences;

static FPS_COUNTER_ENABLED: AtomicBool = AtomicBool::new(false);
static FPS_CAP: AtomicU32 = AtomicU32::new(120);

pub fn apply_preferences(prefs: &Preferences) {
    FPS_COUNTER_ENABLED.store(prefs.fps_counter, Ordering::Relaxed);
    FPS_CAP.store(prefs.fps_cap, Ordering::Relaxed);
}

pub fn fps_counter_enabled() -> bool {
    FPS_COUNTER_ENABLED.load(Ordering::Relaxed)
}

pub fn fps_cap() -> u32 {
    FPS_CAP.load(Ordering::Relaxed)
}

pub fn sleep_until_next_frame(last_frame: Instant) {
    let cap = fps_cap();
    if cap == 0 {
        return;
    }

    let target = Duration::from_secs_f64(1.0 / cap as f64);
    let elapsed = last_frame.elapsed();
    if elapsed < target {
        thread::sleep(target - elapsed);
    }
}
