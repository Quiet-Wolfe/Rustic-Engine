//! Process-wide RL switch. Set once from `main` based on CLI flags and
//! read from PlayScreen when a new song starts. Using a `OnceLock` (rather
//! than threading options through every constructor) keeps Freeplay /
//! Story Mode / Loading paths untouched — every song that spins up a
//! PlayScreen picks the harness up automatically when RL is on.

use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct RlOpts {
    /// If true, the harness steers gameplay (live REINFORCE). If false, it
    /// only records the human's play for BC bootstrap.
    pub control_gameplay: bool,
    /// Epochs of BC warm-up against already-recorded demos at session start.
    /// 0 disables.
    pub bc_warmup_epochs: usize,
    /// Which `ArchConfig` preset the Omni backbone uses. `None` falls back
    /// to `ArchSize::default()` (`Large`).
    pub arch_size: Option<rustic_rl::ArchSize>,
}

static OPTS: OnceLock<RlOpts> = OnceLock::new();

pub fn set(opts: RlOpts) {
    let _ = OPTS.set(opts);
}

pub fn get() -> Option<&'static RlOpts> {
    OPTS.get()
}
