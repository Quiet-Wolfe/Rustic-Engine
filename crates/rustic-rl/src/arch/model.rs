//! Top-level model that wires the three towers together.

use candle_core::{Device, Result, Tensor};
use candle_nn::{VarBuilder, VarMap};

use super::audio::AudioTower;
use super::backbone::Backbone;
use super::config::ArchConfig;
use super::vision::VisionTower;

pub struct OmniModel {
    pub vision: VisionTower,
    pub audio: AudioTower,
    pub backbone: Backbone,
    pub config: ArchConfig,
}

impl OmniModel {
    pub fn new(cfg: ArchConfig, vb: VarBuilder) -> Result<Self> {
        if let Err(e) = cfg.validate() {
            candle_core::bail!("invalid arch config: {e}");
        }
        let vision = VisionTower::new(&cfg.vision, cfg.fusion_dim, vb.pp("vision"))?;
        let audio = AudioTower::new(&cfg.audio, cfg.fusion_dim, vb.pp("audio"))?;
        let backbone = Backbone::new(&cfg.backbone, cfg.action_classes, vb.pp("backbone"))?;
        Ok(Self {
            vision,
            audio,
            backbone,
            config: cfg,
        })
    }

    /// Convenience constructor: allocate a fresh `VarMap` and build the
    /// model with randomly-initialized weights.
    pub fn fresh(cfg: ArchConfig, device: &Device) -> Result<(Self, VarMap)> {
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, candle_core::DType::F32, device);
        let model = Self::new(cfg, vb)?;
        Ok((model, varmap))
    }

    /// `image`: [B, C, H, W]. `mel`: [B, mel_bins, T]. Returns `[B, 4]`
    /// raw action logits.
    pub fn forward(&self, image: &Tensor, mel: &Tensor) -> Result<Tensor> {
        let v = self.vision.forward(image)?;
        let a = self.audio.forward(mel)?;
        self.backbone.forward(&v, &a)
    }
}
