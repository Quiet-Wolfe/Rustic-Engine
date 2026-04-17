//! Custom omnimodal architecture (vision + audio → action logits).
//! Pattern-inspired by Gemma 4 E2B — three encoders, token-stream fusion,
//! causal-chunked audio attention — but dropped the parts we don't need
//! (PLE, MoE, sliding-window backbone, 262K-vocab LM head).
//!
//! Gated behind the `rl-train` cargo feature because it pulls in candle.

pub mod audio;
pub mod backbone;
pub mod block;
pub mod config;
pub mod model;
pub mod vision;

pub use audio::AudioTower;
pub use backbone::Backbone;
pub use config::{ArchConfig, AudioConfig, BackboneConfig, VisionConfig};
pub use model::OmniModel;
pub use vision::VisionTower;
