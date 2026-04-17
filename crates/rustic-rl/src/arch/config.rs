//! Architecture hyperparameters. Three encoders project into a shared token
//! dimension, get fused as a single stream, and a small backbone drives a
//! 4-logit action head.
//!
//! Intentionally a single config struct — no MatFormer/PLE/MoE knobs. If we
//! ever need those we'll add them behind separate fields rather than
//! reshape the whole config.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchConfig {
    pub vision: VisionConfig,
    pub audio: AudioConfig,
    pub backbone: BackboneConfig,
    /// Dimension every encoder projects into before fusion. Must equal
    /// `backbone.hidden`.
    pub fusion_dim: usize,
    /// Number of lanes the agent outputs a logit for.
    pub action_classes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    /// Square input size (pixels). Images are expected to be this size.
    pub image_size: usize,
    pub patch_size: usize,
    pub layers: usize,
    pub hidden: usize,
    pub heads: usize,
    /// MLP expansion multiplier.
    pub mlp_mult: usize,
    /// Input channels (3 for RGB).
    pub in_channels: usize,
}

impl VisionConfig {
    pub fn tokens_per_image(&self) -> usize {
        let grid = self.image_size / self.patch_size;
        grid * grid
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Mel bins in the input spectrogram (per frame).
    pub mel_bins: usize,
    pub layers: usize,
    pub hidden: usize,
    pub heads: usize,
    pub mlp_mult: usize,
    /// Causal-chunked attention window. Mirrors Gemma 4's scheme — each
    /// chunk attends to `left` frames of history and `right` frames of
    /// future (0 = strictly causal, which is what we need for live play).
    pub chunk_size: usize,
    pub left_context: usize,
    pub right_context: usize,
    /// Channel counts for the two subsampling conv layers (stride-2 each).
    pub subsample_channels: [usize; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackboneConfig {
    pub layers: usize,
    pub hidden: usize,
    pub heads: usize,
    pub mlp_mult: usize,
    /// Max combined token count the backbone is willing to process. Used
    /// for positional embedding sizing.
    pub max_tokens: usize,
}

impl ArchConfig {
    /// ~5-10M params. Good for smoke-tests and early training iterations.
    pub fn tiny() -> Self {
        let hidden = 256;
        Self {
            vision: VisionConfig {
                image_size: 128,
                patch_size: 16,
                layers: 4,
                hidden: 192,
                heads: 4,
                mlp_mult: 4,
                in_channels: 3,
            },
            audio: AudioConfig {
                mel_bins: 80,
                layers: 4,
                hidden: 192,
                heads: 4,
                mlp_mult: 4,
                chunk_size: 12,
                left_context: 13,
                right_context: 0,
                subsample_channels: [64, 32],
            },
            backbone: BackboneConfig {
                layers: 6,
                hidden,
                heads: 8,
                mlp_mult: 4,
                max_tokens: 512,
            },
            fusion_dim: hidden,
            action_classes: 4,
        }
    }

    /// ~30-60M params. Step up from tiny once the pipeline works.
    pub fn small() -> Self {
        let hidden = 384;
        Self {
            vision: VisionConfig {
                image_size: 192,
                patch_size: 16,
                layers: 6,
                hidden: 256,
                heads: 8,
                mlp_mult: 4,
                in_channels: 3,
            },
            audio: AudioConfig {
                mel_bins: 80,
                layers: 6,
                hidden: 256,
                heads: 8,
                mlp_mult: 4,
                chunk_size: 12,
                left_context: 13,
                right_context: 0,
                subsample_channels: [128, 64],
            },
            backbone: BackboneConfig {
                layers: 8,
                hidden,
                heads: 8,
                mlp_mult: 4,
                max_tokens: 1024,
            },
            fusion_dim: hidden,
            action_classes: 4,
        }
    }
}

impl ArchConfig {
    /// Basic sanity check — fusion_dim must equal backbone.hidden (they're
    /// the same space) and both encoders must be projectable into it.
    pub fn validate(&self) -> Result<(), String> {
        if self.fusion_dim != self.backbone.hidden {
            return Err(format!(
                "fusion_dim ({}) must equal backbone.hidden ({})",
                self.fusion_dim, self.backbone.hidden
            ));
        }
        if self.vision.hidden % self.vision.heads != 0 {
            return Err("vision.hidden not divisible by vision.heads".into());
        }
        if self.audio.hidden % self.audio.heads != 0 {
            return Err("audio.hidden not divisible by audio.heads".into());
        }
        if self.backbone.hidden % self.backbone.heads != 0 {
            return Err("backbone.hidden not divisible by backbone.heads".into());
        }
        if self.vision.image_size % self.vision.patch_size != 0 {
            return Err("image_size must be a multiple of patch_size".into());
        }
        Ok(())
    }
}
