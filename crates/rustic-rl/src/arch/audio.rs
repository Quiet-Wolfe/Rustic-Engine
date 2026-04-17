//! Audio tower. Subsampling conv stack (2× stride-2, so 4× downsampling)
//! followed by transformer blocks under a causal-chunked mask. Borrowed
//! directly from Gemma 4's audio config shape — that chunked mask is what
//! lets audio stream with bounded latency.

use candle_core::{Module, Result, Tensor};
use candle_nn::{self as nn, Conv1d, Conv1dConfig, LayerNorm, Linear, VarBuilder};

use super::block::{chunked_causal_mask, TransformerBlock};
use super::config::AudioConfig;

pub struct AudioTower {
    subsample: Vec<Conv1d>,
    in_proj: Linear,
    blocks: Vec<TransformerBlock>,
    norm: LayerNorm,
    out_proj: Linear,
    chunk: usize,
    left: usize,
    right: usize,
}

impl AudioTower {
    pub fn new(cfg: &AudioConfig, fusion_dim: usize, vb: VarBuilder) -> Result<Self> {
        let mut subsample = Vec::new();
        // First conv: mel_bins → subsample_channels[0], stride 2
        let c0 = cfg.subsample_channels[0];
        let c1 = cfg.subsample_channels[1];
        subsample.push(nn::conv1d(
            cfg.mel_bins,
            c0,
            3,
            Conv1dConfig {
                stride: 2,
                padding: 1,
                ..Default::default()
            },
            vb.pp("sub0"),
        )?);
        subsample.push(nn::conv1d(
            c0,
            c1,
            3,
            Conv1dConfig {
                stride: 2,
                padding: 1,
                ..Default::default()
            },
            vb.pp("sub1"),
        )?);

        // Project subsampled channels → hidden
        let in_proj = nn::linear_no_bias(c1, cfg.hidden, vb.pp("in_proj"))?;

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
        let out_proj = nn::linear_no_bias(cfg.hidden, fusion_dim, vb.pp("out_proj"))?;

        Ok(Self {
            subsample,
            in_proj,
            blocks,
            norm,
            out_proj,
            chunk: cfg.chunk_size,
            left: cfg.left_context,
            right: cfg.right_context,
        })
    }

    /// `mel`: [B, mel_bins, T]. Returns `[B, T', fusion_dim]` after 4×
    /// subsampling.
    pub fn forward(&self, mel: &Tensor) -> Result<Tensor> {
        let mut x = mel.clone();
        for conv in &self.subsample {
            x = conv.forward(&x)?.gelu()?;
        }
        // [B, C, T'] → [B, T', C]
        let (_b, _c, _t) = x.dims3()?;
        let x = x.transpose(1, 2)?.contiguous()?;
        let x = self.in_proj.forward(&x)?;

        let (_b, t, _h) = x.dims3()?;
        // Precompute the causal-chunked additive mask once per forward —
        // it's a function of sequence length, not content.
        let mask = chunked_causal_mask(t, self.chunk, self.left, self.right, x.device())?;

        let mut h = x;
        for blk in &self.blocks {
            h = blk.forward(&h, Some(&mask))?;
        }
        let h = self.norm.forward(&h)?;
        self.out_proj.forward(&h)
    }
}
