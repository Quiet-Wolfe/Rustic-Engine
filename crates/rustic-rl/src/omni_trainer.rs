//! Transformer-based trainer using the OmniModel backbone.
//!
//! Projects the symbolic `Observation` into per-lane tokens (one token per lane
//! carrying upcoming notes + key state) plus a context token (health, beat
//! phase), then runs through the backbone's transformer blocks. Outputs 4
//! logits, one per lane, passed through sigmoid for Path B (multi-label,
//! independent Bernoulli per lane, threshold at 0.5).
//!
//! Architecture: OBS per-lane (9 values) → Linear → fusion_dim, plus context
//! (3 values) → Linear → fusion_dim. Total: 5 tokens. Full self-attention lets
//! the model learn cross-lane patterns (chords, rests).

use candle_core::{DType, Device, Module, Result, Tensor, Var};
use candle_nn::{self as nn, Linear, Optimizer, VarBuilder, VarMap};
use rand::Rng;

use crate::arch::backbone::Backbone;
use crate::arch::config::ArchConfig;
use crate::observe::{Action, Observation, LOOKAHEAD_NOTES};

/// Per-lane feature count: LOOKAHEAD_NOTES * 2 (time, sustain) + 1 (key_held)
const LANE_FEAT: usize = LOOKAHEAD_NOTES * 2 + 1;
/// Context feature count: health + sin(beat) + cos(beat)
const CTX_FEAT: usize = 3;
/// Total tokens: 4 lane tokens + 1 context token
const NUM_TOKENS: usize = 5;

pub(crate) struct OmniPolicy {
    /// Projects per-lane features to fusion_dim.
    lane_proj: Linear,
    /// Projects context features to fusion_dim.
    ctx_proj: Linear,
    /// The transformer backbone (shared with the full OmniModel).
    backbone: Backbone,
}

impl OmniPolicy {
    fn new(cfg: &ArchConfig, vb: VarBuilder) -> Result<Self> {
        let fusion_dim = cfg.fusion_dim;
        let lane_proj = nn::linear(LANE_FEAT, fusion_dim, vb.pp("lane_proj"))?;
        let ctx_proj = nn::linear(CTX_FEAT, fusion_dim, vb.pp("ctx_proj"))?;
        let backbone = Backbone::new(&cfg.backbone, cfg.action_classes, vb.pp("backbone"))?;
        Ok(Self {
            lane_proj,
            ctx_proj,
            backbone,
        })
    }

    /// Build the token sequence from an observation.
    /// Returns [B, NUM_TOKENS, fusion_dim].
    fn build_tokens(&self, obs: &Observation, device: &Device) -> Result<Tensor> {
        let mut tokens = Vec::with_capacity(NUM_TOKENS);

        // 4 lane tokens: [time0, sus0, time1, sus1, ..., key_held]
        for lane in 0..4 {
            let mut feat = Vec::with_capacity(LANE_FEAT);
            for slot in 0..LOOKAHEAD_NOTES {
                let (t, sus) = obs.upcoming[lane][slot];
                let t_sec = if t.is_finite() {
                    (t / 1000.0).clamp(-2.0, 4.0)
                } else {
                    4.0
                };
                let sus_sec = (sus / 1000.0).clamp(0.0, 4.0);
                feat.push(t_sec);
                feat.push(sus_sec);
            }
            feat.push(if obs.keys_held[lane] { 1.0 } else { 0.0 });
            let tok = Tensor::from_vec(feat, (1, LANE_FEAT), device)?;
            tokens.push(self.lane_proj.forward(&tok)?);
        }

        // 1 context token: [health, sin(beat), cos(beat)]
        {
            let bpm = if obs.bpm > 0.0 { obs.bpm } else { 120.0 };
            let beats = obs.song_pos_ms / (60000.0 / bpm);
            let phase = (beats.fract() as f32) * std::f32::consts::TAU;
            let mut feat = Vec::with_capacity(CTX_FEAT);
            feat.push(obs.health);
            feat.push(phase.sin());
            feat.push(phase.cos());
            let tok = Tensor::from_vec(feat, (1, CTX_FEAT), device)?;
            tokens.push(self.ctx_proj.forward(&tok)?);
        }

        // Stack: each token is [1, fusion_dim] → cat along dim 0 → [NUM_TOKENS, fusion_dim]
        // Then unsqueeze batch → [1, NUM_TOKENS, fusion_dim]
        let stacked = Tensor::cat(&tokens, 0)?;
        stacked.unsqueeze(0)
    }
}

impl Module for OmniPolicy {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        self.backbone.forward_tokens(x)
    }
}

impl crate::PolicyModel for OmniPolicy {
    fn build_input_batch(&self, observations: &[Observation], device: &Device) -> Result<Tensor> {
        let b = observations.len();
        let mut tokens = Vec::with_capacity(NUM_TOKENS);

        // 4 lane tokens
        for lane in 0..4 {
            let mut flat_feat = Vec::with_capacity(b * LANE_FEAT);
            for obs in observations {
                for slot in 0..LOOKAHEAD_NOTES {
                    let (t, sus) = obs.upcoming[lane][slot];
                    let t_sec = if t.is_finite() {
                        (t / 1000.0).clamp(-2.0, 4.0)
                    } else {
                        4.0
                    };
                    let sus_sec = (sus / 1000.0).clamp(0.0, 4.0);
                    flat_feat.push(t_sec);
                    flat_feat.push(sus_sec);
                }
                flat_feat.push(if obs.keys_held[lane] { 1.0 } else { 0.0 });
            }
            let tok = Tensor::from_vec(flat_feat, (b, LANE_FEAT), device)?;
            tokens.push(self.lane_proj.forward(&tok)?.unsqueeze(1)?); // [B, 1, H]
        }

        // 1 context token
        let mut flat_ctx = Vec::with_capacity(b * CTX_FEAT);
        for obs in observations {
            let bpm = if obs.bpm > 0.0 { obs.bpm } else { 120.0 };
            let beats = obs.song_pos_ms / (60000.0 / bpm);
            let phase = (beats.fract() as f32) * std::f32::consts::TAU;
            flat_ctx.push(obs.health);
            flat_ctx.push(phase.sin());
            flat_ctx.push(phase.cos());
        }
        let tok = Tensor::from_vec(flat_ctx, (b, CTX_FEAT), device)?;
        tokens.push(self.ctx_proj.forward(&tok)?.unsqueeze(1)?); // [B, 1, H]

        Tensor::cat(&tokens, 1) // [B, 5, H]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OmniTrainStats {
    pub step: usize,
    pub loss: f32,
    pub mean_reward: f32,
    pub total_reward: f32,
    /// Per-lane sigmoid probabilities from the last decide call.
    pub last_probs: [f32; 4],
}

/// Per-lane sigmoid probabilities from the most recent forward pass.
/// Stored so the HUD can display what the model is "thinking."
struct LastForward {
    probs: [f32; 4],
    /// Attention weights from the last backbone block: [H, T, T], mean over heads.
    attn_heatmap: Option<Tensor>,
}

pub struct OmniTrainer {
    varmap: VarMap,
    policy: OmniPolicy,
    optimizer: nn::AdamW,
    device: Device,
    trajectory: Vec<Step>,
    /// Sampled action log-probs and policy entropy from most recent `decide()`.
    pending_log_probs: Option<(Tensor, Tensor)>,
    batch_size: usize,
    reward_baseline: f32,
    baseline_momentum: f32,
    total_updates: usize,
    last_forward: Option<LastForward>,
}

struct Step {
    log_probs: Tensor,
    reward: f32,
    entropy: Tensor,
}

impl OmniTrainer {
    pub fn new(cfg: ArchConfig, batch_size: usize, learning_rate: f64) -> Result<Self> {
        let device = crate::best_device();
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, DType::F32, &device);
        let policy = OmniPolicy::new(&cfg, vb)?;
        let vars: Vec<Var> = varmap.all_vars();
        let params = candle_nn::ParamsAdamW {
            lr: learning_rate,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
            weight_decay: 0.0,
        };
        let optimizer = nn::AdamW::new(vars, params)?;
        Ok(Self {
            varmap,
            policy,
            optimizer,
            device,
            trajectory: Vec::with_capacity(batch_size),
            pending_log_probs: None,
            batch_size,
            reward_baseline: 0.0,
            baseline_momentum: 0.99,
            total_updates: 0,
            last_forward: None,
        })
    }

    /// Forward the observation, sample per-lane Bernoulli (Path B), stash log-probs.
    pub fn decide(&mut self, obs: &Observation) -> Result<Action> {
        let tokens = self.policy.build_tokens(obs, &self.device)?;
        let logits = self.policy.backbone.forward_tokens(&tokens)?.squeeze(0)?; // [4]

        // Path B: sigmoid per lane, independent Bernoulli sampling
        let probs_vec: Vec<f32> = candle_nn::ops::sigmoid(&logits)?.to_vec1()?;
        let mut rng = rand::rng();
        let mut press = [false; 4];
        let mut probs = [0.0f32; 4];
        for (i, p) in probs_vec.iter().enumerate() {
            probs[i] = *p;
            press[i] = rng.random::<f32>() < *p;
        }

        // Compute attention heatmap from last backbone block for the viewer.
        let attn_heatmap = self.extract_attn_heatmap(&tokens).ok();

        // log-prob of sampled action: sum over lanes of a*log(p) + (1-a)*log(1-p)
        let mask = Tensor::from_vec(
            press
                .iter()
                .map(|b| if *b { 1f32 } else { 0f32 })
                .collect::<Vec<_>>(),
            (4,),
            &self.device,
        )?;
        let one = Tensor::ones((4,), DType::F32, &self.device)?;
        let log_p = log_sigmoid(&logits)?;
        let log_1mp = log_sigmoid(&logits.neg()?)?;
        let lp = (mask.mul(&log_p)? + (one - &mask)?.mul(&log_1mp)?)?;

        // Entropy H = -[p*log(p) + (1-p)*log(1-p)]
        let p = candle_nn::ops::sigmoid(&logits)?;
        let one_p = candle_nn::ops::sigmoid(&logits.neg()?)?;
        let entropy = (p.mul(&log_p)? + one_p.mul(&log_1mp)?)?.neg()?;

        self.pending_log_probs = Some((lp, entropy));
        self.last_forward = Some(LastForward {
            probs,
            attn_heatmap,
        });
        Ok(Action { press })
    }

    /// Extract a [T, T] attention heatmap from the last backbone block by
    /// running a second forward through just that block. Cheap enough for
    /// 5 tokens.
    fn extract_attn_heatmap(&self, tokens: &Tensor) -> Result<Tensor> {
        let blocks = self.policy.backbone.blocks();
        if blocks.is_empty() {
            return Ok(Tensor::zeros(
                (NUM_TOKENS, NUM_TOKENS),
                DType::F32,
                &self.device,
            )?);
        }
        // Run through all blocks except the last normally, then extract attn from the last.
        let last_idx = blocks.len() - 1;
        let mut h = tokens.clone();
        for (i, blk) in blocks.iter().enumerate() {
            if i < last_idx {
                h = blk.forward(&h, None)?;
            }
        }
        let (_out, attn_weights) = blocks[last_idx].forward_with_attn(&h, None)?;
        // attn_weights: [B, H, T, T] → mean over heads → [B, T, T] → squeeze batch
        let heatmap = attn_weights.mean(1)?.squeeze(0)?;
        Ok(heatmap)
    }

    pub fn observe_reward(&mut self, reward: f32) {
        if let Some((lp, entropy)) = self.pending_log_probs.take() {
            self.trajectory.push(Step {
                log_probs: lp,
                reward,
                entropy,
            });
        }
    }

    pub fn maybe_update(&mut self) -> Result<Option<OmniTrainStats>> {
        if self.trajectory.len() < self.batch_size {
            return Ok(None);
        }

        let total_reward: f32 = self.trajectory.iter().map(|s| s.reward).sum();
        let mean_reward = total_reward / self.trajectory.len() as f32;

        self.reward_baseline = self.baseline_momentum * self.reward_baseline
            + (1.0 - self.baseline_momentum) * mean_reward;
        let baseline = self.reward_baseline;

        let mut loss = Tensor::zeros((), DType::F32, &self.device)?;
        let entropy_beta = 0.01f64;

        for step in &self.trajectory {
            let advantage = step.reward - baseline;
            let step_lp_sum = step.log_probs.sum_all()?;
            let step_entropy_sum = step.entropy.sum_all()?;

            let scaled_lp = (step_lp_sum * (-advantage as f64))?;
            let scaled_entropy = (step_entropy_sum * (-entropy_beta))?;

            loss = (loss + scaled_lp)?;
            loss = (loss + scaled_entropy)?;
        }
        let loss = (loss / self.trajectory.len() as f64)?;

        let grads = loss.backward()?;
        self.optimizer.step(&grads)?;

        let stats = OmniTrainStats {
            step: self.total_updates,
            loss: loss.to_scalar::<f32>()?,
            mean_reward,
            total_reward,
            last_probs: self
                .last_forward
                .as_ref()
                .map(|f| f.probs)
                .unwrap_or([0.0; 4]),
        };
        self.total_updates += 1;
        self.trajectory.clear();
        Ok(Some(stats))
    }

    pub fn trajectory_len(&self) -> usize {
        self.trajectory.len()
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    pub fn param_count(&self) -> usize {
        self.varmap
            .all_vars()
            .iter()
            .map(|v| v.as_tensor().elem_count())
            .sum()
    }

    /// Per-lane sigmoid probabilities from the last decide call.
    pub fn last_probs(&self) -> [f32; 4] {
        self.last_forward
            .as_ref()
            .map(|f| f.probs)
            .unwrap_or([0.0; 4])
    }

    /// Attention heatmap [T, T] from the last decide call, if available.
    pub fn last_attn_heatmap(&self) -> Option<&Tensor> {
        self.last_forward
            .as_ref()
            .and_then(|f| f.attn_heatmap.as_ref())
    }

    pub(crate) fn device_ref(&self) -> &Device {
        &self.device
    }

    pub(crate) fn model_ref(&self) -> &OmniPolicy {
        &self.policy
    }

    pub(crate) fn varmap_ref(&self) -> &VarMap {
        &self.varmap
    }

    /// Save all model weights to a safetensors file. Creates parent dirs.
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = self.varmap.data().lock().unwrap();
        let tensors: Vec<(&str, &Tensor)> = data
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_tensor()))
            .collect();
        let save_map: std::collections::HashMap<String, Tensor> = tensors
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).clone()))
            .collect();
        candle_core::safetensors::save(&save_map, path)
    }

    /// Load model weights from a safetensors file. Silently no-op if the
    /// file doesn't exist (first run). Returns false when skipped.
    pub fn load(&mut self, path: &std::path::Path) -> Result<bool> {
        if !path.exists() {
            return Ok(false);
        }
        let tensors = candle_core::safetensors::load(path, &self.device)?;
        let mut data = self.varmap.data().lock().unwrap();
        for (name, tensor) in tensors {
            if let Some(var) = data.get_mut(&name) {
                var.set(&tensor)?;
            }
        }
        log::info!("rustic-rl: loaded Omni weights from {:?}", path);
        Ok(true)
    }
}

/// Numerically-stable log(sigmoid(x)) = -softplus(-x).
fn log_sigmoid(x: &Tensor) -> Result<Tensor> {
    let neg = x.neg()?;
    let zeros = Tensor::zeros_like(&neg)?;
    let max_part = neg.maximum(&zeros)?;
    let abs_neg = neg.abs()?;
    let log_part = ((abs_neg.neg()?.exp()? + 1.0)?).log()?;
    let softplus = (max_part + log_part)?;
    softplus.neg()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omni_trainer_smoke() {
        let cfg = ArchConfig::tiny();
        let mut t = OmniTrainer::new(cfg, 4, 1e-3).expect("new");
        let obs = Observation::zero();
        let action = t.decide(&obs).expect("decide");
        assert_eq!(action.press.len(), 4);
        // Probs should be around 0.5 for random init
        let probs = t.last_probs();
        for p in probs {
            assert!(p > 0.01 && p < 0.99, "prob {p} out of range");
        }
    }

    #[test]
    fn omni_trainer_update() {
        let cfg = ArchConfig::tiny();
        let mut t = OmniTrainer::new(cfg, 4, 1e-3).expect("new");
        for _ in 0..4 {
            let obs = Observation::zero();
            let _ = t.decide(&obs).expect("decide");
            t.observe_reward(1.0);
        }
        let stats = t.maybe_update().expect("update").expect("stats");
        assert!(stats.loss.is_finite());
        assert_eq!(t.trajectory_len(), 0);
    }

    #[test]
    fn attention_heatmap_shape() {
        let cfg = ArchConfig::tiny();
        let mut t = OmniTrainer::new(cfg, 4, 1e-3).expect("new");
        let obs = Observation::zero();
        let _ = t.decide(&obs).expect("decide");
        let heatmap = t.last_attn_heatmap().expect("heatmap");
        let (rows, cols) = heatmap.dims2().expect("dims");
        assert_eq!(rows, NUM_TOKENS);
        assert_eq!(cols, NUM_TOKENS);
    }

    #[test]
    fn bc_pretrain_on_omni_reduces_loss() {
        let cfg = ArchConfig::tiny();
        let mut t = OmniTrainer::new(cfg, 4, 1e-3).expect("new");

        // Lane 1 always pressed
        let step = crate::demo::DemoStep {
            obs: Observation::zero(),
            action: Action {
                press: [false, true, false, false],
            },
            reward: 0.0,
        };
        let steps = vec![step; 16];

        let stats = crate::bc::pretrain(
            t.model_ref(),
            t.varmap_ref(),
            &steps,
            10, // epochs
            4,  // batch
            1e-2,
            t.device_ref(),
        )
        .expect("pretrain");

        assert!(
            stats.final_loss < 0.7,
            "loss should drop from random init (~0.69)"
        );

        // Verify model now favors lane 1
        let obs = Observation::zero();
        let _ = t.decide(&obs).unwrap();
        let probs = t.last_probs();
        assert!(probs[1] > 0.6, "lane 1 prob should be high: {}", probs[1]);
        assert!(probs[0] < 0.4, "lane 0 prob should be low: {}", probs[0]);
    }
}
