//! Shared transformer primitives. All three encoders use the same
//! attention+MLP pattern, so they live here and get parameterized by mask
//! strategy rather than being duplicated.

use candle_core::{Module, Result, Tensor, D};
use candle_nn::{self as nn, LayerNorm, Linear, VarBuilder};

/// Standard pre-norm transformer block: LN → attention → residual →
/// LN → MLP → residual. Attention mask is owned by the caller so we can
/// plug in either "none" (encoder self-attn), "causal" (decoder), or
/// "chunked" (audio encoder) without branching inside the block.
pub struct TransformerBlock {
    norm1: LayerNorm,
    attn: Attention,
    norm2: LayerNorm,
    mlp: Mlp,
}

impl TransformerBlock {
    pub fn new(hidden: usize, heads: usize, mlp_mult: usize, vb: VarBuilder) -> Result<Self> {
        let norm1 = nn::layer_norm(hidden, 1e-6, vb.pp("norm1"))?;
        let attn = Attention::new(hidden, heads, vb.pp("attn"))?;
        let norm2 = nn::layer_norm(hidden, 1e-6, vb.pp("norm2"))?;
        let mlp = Mlp::new(hidden, hidden * mlp_mult, vb.pp("mlp"))?;
        Ok(Self {
            norm1,
            attn,
            norm2,
            mlp,
        })
    }

    /// `mask` is expected to be broadcastable to attention scores shape
    /// `[B, H, T, T]`. `None` means full attention.
    pub fn forward(&self, x: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let h = self.norm1.forward(x)?;
        let h = self.attn.forward(&h, mask)?;
        let x = (x + h)?;
        let h = self.norm2.forward(&x)?;
        let h = self.mlp.forward(&h)?;
        x + h
    }

    /// Like `forward` but also returns the attention weight matrix from this
    /// block's self-attention layer. Shape: `[B, H, T, T]`.
    pub fn forward_with_attn(&self, x: &Tensor, mask: Option<&Tensor>) -> Result<(Tensor, Tensor)> {
        let h = self.norm1.forward(x)?;
        let (h, attn_weights) = self.attn.forward_with_weights(&h, mask)?;
        let x = (x + h)?;
        let h = self.norm2.forward(&x)?;
        let h = self.mlp.forward(&h)?;
        Ok(((x + h)?, attn_weights))
    }
}

pub struct Attention {
    qkv: Linear,
    out: Linear,
    heads: usize,
    head_dim: usize,
    scale: f64,
}

impl Attention {
    pub fn new(hidden: usize, heads: usize, vb: VarBuilder) -> Result<Self> {
        assert!(hidden % heads == 0, "hidden must be divisible by heads");
        let head_dim = hidden / heads;
        // Fused QKV — one linear, split at runtime. Smaller than three
        // separate linears and avoids three matmul launches.
        let qkv = nn::linear_no_bias(hidden, hidden * 3, vb.pp("qkv"))?;
        let out = nn::linear_no_bias(hidden, hidden, vb.pp("out"))?;
        Ok(Self {
            qkv,
            out,
            heads,
            head_dim,
            scale: 1.0 / (head_dim as f64).sqrt(),
        })
    }

    pub fn forward(&self, x: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let (b, t, _c) = x.dims3()?;
        let qkv = self.qkv.forward(x)?; // [B, T, 3*H]
        let qkv = qkv.reshape((b, t, 3, self.heads, self.head_dim))?;
        // [B, T, 3, H, D] → [3, B, H, T, D]
        let qkv = qkv.permute((2, 0, 3, 1, 4))?.contiguous()?;
        let q = qkv.get(0)?;
        let k = qkv.get(1)?;
        let v = qkv.get(2)?;

        // [B, H, T, T] = Q @ K^T * scale
        let attn = q.matmul(&k.transpose(D::Minus2, D::Minus1)?)?;
        let attn = (attn * self.scale)?;
        let attn = match mask {
            Some(m) => attn.broadcast_add(m)?,
            None => attn,
        };
        let attn = candle_nn::ops::softmax_last_dim(&attn)?;

        let out = attn.matmul(&v)?; // [B, H, T, D]
        let out = out.transpose(1, 2)?.contiguous()?.reshape((b, t, ()))?;
        self.out.forward(&out)
    }

    /// Like `forward` but also returns the attention weight matrix `[B, H, T, T]`.
    pub fn forward_with_weights(
        &self,
        x: &Tensor,
        mask: Option<&Tensor>,
    ) -> Result<(Tensor, Tensor)> {
        let (b, t, _c) = x.dims3()?;
        let qkv = self.qkv.forward(x)?;
        let qkv = qkv.reshape((b, t, 3, self.heads, self.head_dim))?;
        let qkv = qkv.permute((2, 0, 3, 1, 4))?.contiguous()?;
        let q = qkv.get(0)?;
        let k = qkv.get(1)?;
        let v = qkv.get(2)?;

        let attn = q.matmul(&k.transpose(D::Minus2, D::Minus1)?)?;
        let attn = (attn * self.scale)?;
        let attn = match mask {
            Some(m) => attn.broadcast_add(m)?,
            None => attn,
        };
        let attn_weights = candle_nn::ops::softmax_last_dim(&attn)?;

        let out = attn_weights.matmul(&v)?;
        let out = out.transpose(1, 2)?.contiguous()?.reshape((b, t, ()))?;
        Ok((self.out.forward(&out)?, attn_weights))
    }
}

pub struct Mlp {
    fc1: Linear,
    fc2: Linear,
}

impl Mlp {
    pub fn new(hidden: usize, intermediate: usize, vb: VarBuilder) -> Result<Self> {
        let fc1 = nn::linear_no_bias(hidden, intermediate, vb.pp("fc1"))?;
        let fc2 = nn::linear_no_bias(intermediate, hidden, vb.pp("fc2"))?;
        Ok(Self { fc1, fc2 })
    }
}

impl Module for Mlp {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let h = self.fc1.forward(x)?;
        let h = h.gelu()?;
        self.fc2.forward(&h)
    }
}

/// Additive mask that blocks positions outside a causal-chunked window —
/// each query at position `t` may attend to keys in
/// `[chunk_start - left, chunk_end + right]`, where the chunk is a
/// contiguous block of `chunk` timesteps. Values are 0 where allowed and
/// a large negative where blocked, so you can `broadcast_add` onto
/// pre-softmax scores.
pub fn chunked_causal_mask(
    seq_len: usize,
    chunk: usize,
    left: usize,
    right: usize,
    device: &candle_core::Device,
) -> Result<Tensor> {
    let mut data = vec![0f32; seq_len * seq_len];
    for t in 0..seq_len {
        let chunk_idx = t / chunk;
        let chunk_start = chunk_idx * chunk;
        let chunk_end = (chunk_start + chunk).min(seq_len);
        let allowed_lo = chunk_start.saturating_sub(left);
        let allowed_hi = (chunk_end + right).min(seq_len);
        for k in 0..seq_len {
            if k < allowed_lo || k >= allowed_hi {
                data[t * seq_len + k] = f32::NEG_INFINITY;
            }
        }
    }
    Tensor::from_vec(data, (seq_len, seq_len), device)
}
