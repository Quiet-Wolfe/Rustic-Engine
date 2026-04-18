//! Fusion backbone + action head.
//!
//! Vision and audio towers each produce tokens in the shared fusion dim.
//! We concatenate them with learned modality-marker embeddings
//! (BOI/BOA equivalents) and run the combined stream through a small
//! full-attention transformer. Final action logits come from mean-pooling
//! the last layer and projecting to `action_classes`.
//!
//! ## Action scheme (Path B: multi-label sigmoid)
//!
//! The action head outputs 4 raw logits, one per lane. These are passed
//! through sigmoid to get independent probabilities in [0,1]. Each lane
//! is pressed when its probability exceeds a threshold (default 0.5).
//! This lets the agent press any combination of lanes (including none)
//! without a dedicated "no-op" class.

use candle_core::{Module, Result, Tensor, D};
use candle_nn::{self as nn, LayerNorm, Linear, VarBuilder};

use super::block::TransformerBlock;
use super::config::BackboneConfig;

pub struct Backbone {
    /// [BOI, BOA] modality start markers. Inserted before vision and
    /// audio tokens respectively.
    markers: Tensor,
    /// [CLS] token prepended to the sequence for action head pooling.
    cls_token: Tensor,
    pos_embed: Tensor,
    blocks: Vec<TransformerBlock>,
    norm: LayerNorm,
    action_head: Linear,
    max_tokens: usize,
}

impl Backbone {
    pub fn new(cfg: &BackboneConfig, action_classes: usize, vb: VarBuilder) -> Result<Self> {
        let markers = vb.get((2, cfg.hidden), "markers")?;
        let cls_token = vb.get((1, 1, cfg.hidden), "cls_token")?;
        let pos_embed = vb.get((1, cfg.max_tokens, cfg.hidden), "pos_embed")?;

        let mut blocks = Vec::with_capacity(cfg.layers);
        for i in 0..cfg.layers {
            blocks.push(TransformerBlock::new(
                cfg.hidden,
                cfg.heads,
                cfg.mlp_mult,
                vb.pp(format!("blocks.{i}")),
            )?);
        }

        let norm = nn::layer_norm(cfg.hidden, 1e-6, vb.pp("norm"))?;
        let action_head = nn::linear_no_bias(cfg.hidden, action_classes, vb.pp("action_head"))?;

        Ok(Self {
            markers,
            cls_token,
            pos_embed,
            blocks,
            norm,
            action_head,
            max_tokens: cfg.max_tokens,
        })
    }

    /// `vision_tokens`: [B, Nv, H], `audio_tokens`: [B, Na, H]. Returns
    /// `[B, action_classes]` raw logits.
    pub fn forward(&self, vision_tokens: &Tensor, audio_tokens: &Tensor) -> Result<Tensor> {
        let b = vision_tokens.dim(0)?;

        // Expand markers to [B, 1, H] each.
        let boi = self
            .markers
            .get(0)?
            .unsqueeze(0)?
            .unsqueeze(0)?
            .broadcast_as((b, 1, vision_tokens.dim(D::Minus1)?))?;
        let boa = self
            .markers
            .get(1)?
            .unsqueeze(0)?
            .unsqueeze(0)?
            .broadcast_as((b, 1, audio_tokens.dim(D::Minus1)?))?;

        // Expand CLS token to [B, 1, H].
        let cls = self
            .cls_token
            .broadcast_as((b, 1, vision_tokens.dim(D::Minus1)?))?;

        // [CLS, BOI, v0..vn, BOA, a0..am]
        let seq = Tensor::cat(&[&cls, &boi, vision_tokens, &boa, audio_tokens], 1)?;

        let t = seq.dim(1)?;
        if t > self.max_tokens {
            candle_core::bail!(
                "backbone token count {t} exceeds max_tokens {}",
                self.max_tokens
            );
        }

        // Slice the positional embedding to the actual sequence length.
        let pos = self.pos_embed.narrow(1, 0, t)?;
        let mut h = seq.broadcast_add(&pos)?;

        for blk in &self.blocks {
            h = blk.forward(&h, None)?;
        }
        let h = self.norm.forward(&h)?;
        // Use CLS token (index 0) for the action head.
        let pooled = h.narrow(1, 0, 1)?.squeeze(1)?;
        self.action_head.forward(&pooled)
    }

    /// Process a pre-projected token sequence without modality markers.
    /// `tokens`: [B, T, H]. Returns `[B, action_classes]` raw logits.
    /// Used by the symbolic (non-vision/audio) training path.
    pub fn forward_tokens(&self, tokens: &Tensor) -> Result<Tensor> {
        let b = tokens.dim(0)?;
        let cls = self
            .cls_token
            .broadcast_as((b, 1, tokens.dim(D::Minus1)?))?;
        let seq = Tensor::cat(&[&cls, tokens], 1)?;

        let t = seq.dim(1)?;
        if t > self.max_tokens {
            candle_core::bail!(
                "backbone token count {t} exceeds max_tokens {}",
                self.max_tokens
            );
        }
        let pos = self.pos_embed.narrow(1, 0, t)?;
        let mut h = seq.broadcast_add(&pos)?;
        for blk in &self.blocks {
            h = blk.forward(&h, None)?;
        }
        let h = self.norm.forward(&h)?;
        // Use CLS token (index 0) for the action head.
        let pooled = h.narrow(1, 0, 1)?.squeeze(1)?;
        self.action_head.forward(&pooled)
    }

    /// Return a reference to the transformer blocks for attention extraction.
    pub fn blocks(&self) -> &[TransformerBlock] {
        &self.blocks
    }

    /// Return a reference to the positional embedding tensor.
    pub fn pos_embed(&self) -> &Tensor {
        &self.pos_embed
    }
}
