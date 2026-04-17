//! Placeholder for the eventual inference network.
//!
//! When we wire up real models, this module will own the forward pass —
//! likely via `ndarray` on CPU for portability, with an optional
//! platform-specific accelerator (MLX on macOS, Vulkan/CL via a feature
//! flag, etc.). For now it's a trait + a no-op impl so the rest of the
//! crate can build and link.

use crate::observe::{Action, Observation};

/// A forward-only model: observation in, action scores out. Training happens
/// out of process — we only ever run inference inside the engine.
pub trait Network {
    /// Score each lane (higher = more likely to press). Length must be 4.
    fn forward(&self, obs: &Observation) -> [f32; 4];
}

/// Returns zeros for every lane — lets callers build on the trait without
/// committing to a model implementation yet.
#[derive(Default)]
pub struct NullNetwork;

impl Network for NullNetwork {
    fn forward(&self, _obs: &Observation) -> [f32; 4] {
        [0.0; 4]
    }
}

/// Greedy-threshold wrapper: press a lane when its score exceeds
/// `threshold`. Handy for smoke-testing the observation→action pipeline
/// without a real policy.
pub fn greedy_action(scores: [f32; 4], threshold: f32) -> Action {
    let mut press = [false; 4];
    for i in 0..4 {
        press[i] = scores[i] >= threshold;
    }
    Action { press }
}
