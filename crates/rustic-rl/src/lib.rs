//! Optional RL harness. Gated behind the `rl` feature in rustic-app.
//!
//! Design: inference runs in-engine (cheap, single-threaded CPU forward pass
//! via `ndarray` when we wire up a real model); training runs out-of-engine
//! (Python + whatever framework). The game loop hands an `Observation` to
//! an `RLAgent`, gets back an `Action`, and applies it as if it were input.

pub mod agent;
pub mod demo;
pub mod network;
pub mod observe;
pub mod observe_build;

#[cfg(feature = "rl-train")]
pub mod arch;

#[cfg(feature = "rl-train")]
pub mod bc;

#[cfg(feature = "rl-train")]
pub mod harness;

#[cfg(feature = "rl-train")]
pub mod trainer;

#[cfg(feature = "rl-train")]
pub mod omni_trainer;

/// Pick the best available candle device at runtime: CUDA → Metal → CPU.
/// `candle_core::utils::{cuda,metal}_is_available()` return false whenever
/// the corresponding backend feature isn't compiled in, so this also works
/// correctly on CPU-only builds.
#[cfg(feature = "rl-train")]
pub fn best_device() -> candle_core::Device {
    if candle_core::utils::cuda_is_available() {
        match candle_core::Device::new_cuda(0) {
            Ok(d) => {
                log::info!("rustic-rl: using CUDA device 0");
                return d;
            }
            Err(e) => log::warn!(
                "rustic-rl: CUDA reported available but init failed ({e}); trying next backend"
            ),
        }
    }
    if candle_core::utils::metal_is_available() {
        match candle_core::Device::new_metal(0) {
            Ok(d) => {
                log::info!("rustic-rl: using Metal device 0");
                return d;
            }
            Err(e) => log::warn!(
                "rustic-rl: Metal reported available but init failed ({e}); falling back to CPU"
            ),
        }
    }
    log::info!("rustic-rl: using CPU device (no GPU backend compiled/available)");
    candle_core::Device::Cpu
}

pub use agent::{Config, IdlePolicy, NetworkPolicy, Policy, PolicyKind, RLAgent, RandomPolicy};
pub use demo::{DemoRecorder, DemoStep};
pub use network::{greedy_action, Network, NullNetwork};
pub use observe::{Action, Observation, LOOKAHEAD_NOTES};
pub use observe_build::{build_observation, UpcomingNote};

/// Trait for RL models that can build their own input tensors from observations.
/// This allows the BC pretraining loop to be agnostic to the model architecture
/// (e.g. flat MLP vs. tokenized Transformer).
#[cfg(feature = "rl-train")]
pub trait PolicyModel: candle_core::Module {
    fn build_input_batch(
        &self,
        observations: &[Observation],
        device: &candle_core::Device,
    ) -> candle_core::Result<candle_core::Tensor>;
}

#[cfg(feature = "rl-train")]
pub use harness::{ArchSize, Harness, HarnessConfig, ModelChoice};
