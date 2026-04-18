//! Placeholder for the eventual inference network.
//!
//! When we wire up real models, this module will own the forward pass —
//! likely via `ndarray` on CPU for portability, with an optional
//! platform-specific accelerator (MLX on macOS, Vulkan/CL via a feature
//! flag, etc.). For now it's a trait + a no-op impl so the rest of the
//! crate can build and link.
//!
//! ## Path B: Multi-Label Action Selection
//!
//! Both the MLP trainer and the OmniModel transformer use the same action
//! scheme: 4 independent sigmoid logits, one per lane. A lane is pressed
//! when its sigmoid probability exceeds a threshold (default 0.5). The
//! agent can express "no action" by dropping all 4 logits below threshold,
//! and can press chords by spiking multiple logits above threshold
//! simultaneously. This avoids the "always-pressing-something" problem of
//! a softmax over 4 or 5 classes.

use crate::observe::{Action, Observation};

/// A forward-only model: observation in, raw logits out. Training happens
/// out of process — we only ever run inference inside the engine.
pub trait Network {
    /// Raw logit per lane (before sigmoid). Length must be 4.
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

/// Sigmoid: 1 / (1 + exp(-x)). Inlined to avoid pulling in a tensor library
/// for what's just 4 scalars on the inference fast path.
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Path B action selection: raw logits → sigmoid → threshold.
///
/// Each lane is independent. Press lane i when sigmoid(logit_i) > threshold.
/// - Rest (no action): model drops all 4 logits → all sigmoids below 0.5 → [false, false, false, false]
/// - Chord: model spikes multiple logits → e.g. [true, false, true, false] for a left+up double
pub fn greedy_action(logits: [f32; 4], threshold: f32) -> Action {
    let mut press = [false; 4];
    for i in 0..4 {
        press[i] = sigmoid(logits[i]) >= threshold;
    }
    Action { press }
}

/// Like `greedy_action` but returns the sigmoid probabilities alongside the
/// action — useful for the HUD viewer.
pub fn greedy_action_with_probs(logits: [f32; 4], threshold: f32) -> (Action, [f32; 4]) {
    let mut press = [false; 4];
    let mut probs = [0.0f32; 4];
    for i in 0..4 {
        probs[i] = sigmoid(logits[i]);
        press[i] = probs[i] >= threshold;
    }
    (Action { press }, probs)
}
