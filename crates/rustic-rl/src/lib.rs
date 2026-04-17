//! Optional RL harness. Gated behind the `rl` feature in rustic-app.
//!
//! Design: inference runs in-engine (cheap, single-threaded CPU forward pass
//! via `ndarray` when we wire up a real model); training runs out-of-engine
//! (Python + whatever framework). The game loop hands an `Observation` to
//! an `RLAgent`, gets back an `Action`, and applies it as if it were input.

pub mod agent;
pub mod network;
pub mod observe;

pub use agent::{Config, IdlePolicy, NetworkPolicy, Policy, PolicyKind, RLAgent, RandomPolicy};
pub use network::{greedy_action, Network, NullNetwork};
pub use observe::{Action, Observation, LOOKAHEAD_NOTES};
