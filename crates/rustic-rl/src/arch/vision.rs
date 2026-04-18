//! Vision tower. A plain ViT: `conv(patch)` → flatten → positional
//! embedding → N transformer blocks → project to fusion dim.

use candle_core::{Module, Result, Tensor};
use candle_nn::{self as nn, Conv2d, Conv2dConfig, LayerNorm, Linear, VarBuilder};

use super::block::TransformerBlock;
use super::config::VisionConfig;

pub struct VisionTower {
    patch_embed: Conv2d,
    pos_embed: Tensor,
    blocks: Vec<TransformerBlock>,
    norm: LayerNorm,
    proj: Linear,
    num_patches: usize,
}

impl VisionTower {
    pub fn new(cfg: &VisionConfig, fusion_dim: usize, vb: VarBuilder) -> Result<Self> {
        let grid = cfg.image_size / cfg.patch_size;
        let num_patches = grid * grid;

        let patch_embed = nn::conv2d(
            cfg.in_channels,
            cfg.hidden,
            cfg.patch_size,
            Conv2dConfig {
                stride: cfg.patch_size,
                ..Default::default()
            },
            vb.pp("patch_embed"),
        )?;

        // Learned positional embedding — one vector per patch.
        let pos_embed = vb.get((1, num_patches, cfg.hidden), "pos_embed")?;

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
        let proj = nn::linear_no_bias(cfg.hidden, fusion_dim, vb.pp("proj"))?;

        Ok(Self {
            patch_embed,
            pos_embed,
            blocks,
            norm,
            proj,
            num_patches,
        })
    }

    pub fn num_patches(&self) -> usize {
        self.num_patches
    }

    /// `image`: [B, C, H, W] floats in `[0, 1]` (any normalization lives
    /// upstream). Returns `[B, num_patches, fusion_dim]` tokens.
    pub fn forward(&self, image: &Tensor) -> Result<Tensor> {
        // [B, C, H, W] → [B, hidden, grid, grid]
        let x = self.patch_embed.forward(image)?;
        let (b, c, gh, gw) = x.dims4()?;
        // [B, hidden, grid*grid] → [B, grid*grid, hidden]
        let x = x.reshape((b, c, gh * gw))?.transpose(1, 2)?;
        let x = x.broadcast_add(&self.pos_embed)?;

        let mut h = x;
        for blk in &self.blocks {
            h = blk.forward(&h, None)?;
        }
        let h = self.norm.forward(&h)?;
        self.proj.forward(&h)
    }
}
