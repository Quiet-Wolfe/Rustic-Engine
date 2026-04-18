//! Minimal REINFORCE trainer over the symbolic `Observation`.
//!
//! This is the first end-to-end training path wired into the engine: a
//! small MLP produces independent Bernoulli logits per lane, we sample an
//! action, record the log-probability, the game scores it, and we run a
//! REINFORCE update every N steps. Deliberately simple — we want to see
//! the agent learn *something* before scaling up to the OmniModel with
//! real pixel/audio observations.
//!
//! Scope markers (things we'll want later, not implemented yet):
//! - Pixel + audio observations via `arch::OmniModel` — drop-in replacement
//!   once `PlayScreen` can hand us downsampled frames and mel windows.
//! - Advantage estimation beyond simple reward-to-go baseline (GAE/PPO).
//! - GPU device selection via the `cuda`/`metal` features.

use candle_core::{DType, Device, Module, Result, Tensor, Var};
use candle_nn::{self as nn, Linear, Optimizer, VarBuilder, VarMap};
use rand::Rng;

use crate::observe::{Action, Observation, LOOKAHEAD_NOTES};

/// Flat observation size passed to the MLP.
/// Laid out as:
///   - 4 × LOOKAHEAD_NOTES × 2 (time_until_hit, sustain) — per-lane upcoming notes
///   - 4 — keys_held
///   - 1 — health
///   - 1 — sin(beat phase), 1 — cos(beat phase)
pub const OBS_DIM: usize = 4 * LOOKAHEAD_NOTES * 2 + 4 + 1 + 2;

/// Tiny 2-layer MLP: OBS_DIM → hidden → 4 logits. A single hidden layer
/// is enough to see learning on the symbolic observation; we bump size
/// once this proves itself.
pub(crate) struct Mlp {
    fc1: Linear,
    fc2: Linear,
}

impl Mlp {
    fn new(hidden: usize, vb: VarBuilder) -> Result<Self> {
        let fc1 = nn::linear(OBS_DIM, hidden, vb.pp("fc1"))?;
        let fc2 = nn::linear(hidden, 4, vb.pp("fc2"))?;
        Ok(Self { fc1, fc2 })
    }
}

impl Module for Mlp {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let h = self.fc1.forward(x)?.relu()?;
        self.fc2.forward(&h)
    }
}

impl crate::PolicyModel for Mlp {
    fn build_input_batch(
        &self,
        observations: &[crate::Observation],
        device: &Device,
    ) -> Result<Tensor> {
        let b = observations.len();
        let mut flat_inputs = Vec::with_capacity(b * OBS_DIM);
        for obs in observations {
            flat_inputs.extend(flatten_observation(obs));
        }
        Tensor::from_vec(flat_inputs, (b, OBS_DIM), device)
    }
}

/// One timestep in the trajectory buffer. Stored on-device so we can
/// differentiate through the stored log-probs at update time.
struct Step {
    /// Per-lane log-probability of the action we actually took.
    log_probs: Tensor, // [4]
    /// Immediate reward granted for this action.
    reward: f32,
    /// Per-lane entropy of the policy at this step.
    entropy: Tensor, // [4]
}

#[derive(Debug, Clone, Copy)]
pub struct TrainStats {
    pub step: usize,
    pub loss: f32,
    pub mean_reward: f32,
    pub total_reward: f32,
}

pub struct Trainer {
    varmap: VarMap,
    model: Mlp,
    optimizer: nn::SGD,
    device: Device,
    trajectory: Vec<Step>,
    /// Sampled action log-probs and policy entropy from most recent `decide()`.
    pending_log_probs: Option<(Tensor, Tensor)>,
    /// REINFORCE batches — we run an update every `batch_size` steps.
    batch_size: usize,
    learning_rate: f64,
    /// Running mean-baseline for variance reduction. Simple EMA, no value net.
    reward_baseline: f32,
    baseline_momentum: f32,
    total_updates: usize,
}

impl Trainer {
    pub fn new(hidden: usize, batch_size: usize, learning_rate: f64) -> Result<Self> {
        let device = crate::best_device();
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, DType::F32, &device);
        let model = Mlp::new(hidden, vb)?;
        let vars: Vec<Var> = varmap.all_vars();
        let optimizer = nn::SGD::new(vars, learning_rate)?;
        Ok(Self {
            varmap,
            model,
            optimizer,
            device,
            trajectory: Vec::with_capacity(batch_size),
            pending_log_probs: None,
            batch_size,
            learning_rate,
            reward_baseline: 0.0,
            baseline_momentum: 0.99,
            total_updates: 0,
        })
    }

    /// Forward the observation, sample an action per lane (Bernoulli over
    /// sigmoid(logit)), and stash the per-lane log-probabilities so the
    /// next `observe_reward` call can attach the reward.
    pub fn decide(&mut self, obs: &Observation) -> Result<Action> {
        let flat = flatten_observation(obs);
        let x = Tensor::from_vec(flat, (1, OBS_DIM), &self.device)?;
        let logits = self.model.forward(&x)?.squeeze(0)?; // [4]

        // Sample Bernoulli per lane using probs = sigmoid(logits). Do the
        // sampling out-of-graph with plain rand, then build the log-prob
        // tensor from logits + sampled bits (still differentiable wrt logits).
        let probs_vec: Vec<f32> = candle_nn::ops::sigmoid(&logits)?.to_vec1()?;
        let mut rng = rand::rng();
        let mut press = [false; 4];
        for (i, p) in probs_vec.iter().enumerate() {
            press[i] = rng.random::<f32>() < *p;
        }

        // log-prob of the sampled action vector = sum over lanes of
        //   a*log(p) + (1-a)*log(1-p)
        // = sum(-softplus(-l) if a else -softplus(l))
        // We build this as a differentiable tensor expression.
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
        Ok(Action { press })
    }

    /// Attach the reward for the action returned by the most recent
    /// `decide`. Ignored if `decide` was not called since the last reward.
    pub fn observe_reward(&mut self, reward: f32) {
        if let Some((lp, entropy)) = self.pending_log_probs.take() {
            self.trajectory.push(Step {
                log_probs: lp,
                reward,
                entropy,
            });
        }
    }

    /// If the trajectory buffer is full, run a REINFORCE step and return
    /// stats. Otherwise returns None.
    pub fn maybe_update(&mut self) -> Result<Option<TrainStats>> {
        if self.trajectory.len() < self.batch_size {
            return Ok(None);
        }

        let total_reward: f32 = self.trajectory.iter().map(|s| s.reward).sum();
        let mean_reward = total_reward / self.trajectory.len() as f32;

        // Update running baseline and compute advantages.
        self.reward_baseline = self.baseline_momentum * self.reward_baseline
            + (1.0 - self.baseline_momentum) * mean_reward;
        let baseline = self.reward_baseline;

        // Sum of per-step losses: loss_t = -advantage_t * sum(log_probs_t) - beta * sum(entropy_t)
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

        let stats = TrainStats {
            step: self.total_updates,
            loss: loss.to_scalar::<f32>()?,
            mean_reward,
            total_reward,
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

    pub fn learning_rate(&self) -> f64 {
        self.learning_rate
    }

    /// Total parameter count — handy for logging.
    pub fn param_count(&self) -> usize {
        self.varmap
            .all_vars()
            .iter()
            .map(|v| v.as_tensor().elem_count())
            .sum()
    }

    /// Accessors used by the BC pretrainer to reuse this trainer's model
    /// and its optimizer state.
    pub(crate) fn model_ref(&self) -> &Mlp {
        &self.model
    }

    pub(crate) fn varmap_ref(&self) -> &VarMap {
        &self.varmap
    }

    pub(crate) fn device_ref(&self) -> &Device {
        &self.device
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
        log::info!("rustic-rl: loaded MLP weights from {:?}", path);
        Ok(true)
    }
}

/// Numerically-stable log(sigmoid(x)) = -softplus(-x).
fn log_sigmoid(x: &Tensor) -> Result<Tensor> {
    // -softplus(-x) = -(log(1 + exp(-x)))
    let neg = x.neg()?;
    // log(1 + exp(neg)) done via candle's stable softplus-ish expression:
    // log(1 + exp(z)) = max(z,0) + log(1 + exp(-|z|))
    let zeros = Tensor::zeros_like(&neg)?;
    let max_part = neg.maximum(&zeros)?;
    let abs_neg = neg.abs()?;
    let log_part = ((abs_neg.neg()?.exp()? + 1.0)?).log()?;
    let softplus = (max_part + log_part)?;
    softplus.neg()
}

/// Pack an `Observation` into a flat f32 vector of length `OBS_DIM`.
pub fn flatten_observation(obs: &Observation) -> Vec<f32> {
    let mut v = Vec::with_capacity(OBS_DIM);
    for lane in 0..4 {
        for slot in 0..LOOKAHEAD_NOTES {
            let (t, sus) = obs.upcoming[lane][slot];
            // Large/infinite times → clamp to a sane upper bound in seconds
            // so the net sees a bounded input.
            let t_sec = if t.is_finite() {
                (t / 1000.0).clamp(-2.0, 4.0)
            } else {
                4.0
            };
            let sus_sec = (sus / 1000.0).clamp(0.0, 4.0);
            v.push(t_sec);
            v.push(sus_sec);
        }
    }
    for held in obs.keys_held {
        v.push(if held { 1.0 } else { 0.0 });
    }
    v.push(obs.health);
    // Beat phase derived from song position and BPM.
    let bpm = if obs.bpm > 0.0 { obs.bpm } else { 120.0 };
    let beats = obs.song_pos_ms / (60000.0 / bpm);
    let phase = (beats.fract() as f32) * std::f32::consts::TAU;
    v.push(phase.sin());
    v.push(phase.cos());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_has_expected_length() {
        let obs = Observation::zero();
        assert_eq!(flatten_observation(&obs).len(), OBS_DIM);
    }

    #[test]
    fn trainer_runs_update_when_buffer_full() {
        let mut t = Trainer::new(32, 8, 1e-2).expect("new");
        for i in 0..8 {
            let obs = Observation::zero();
            let _ = t.decide(&obs).expect("decide");
            t.observe_reward(if i % 2 == 0 { 1.0 } else { -1.0 });
        }
        let stats = t.maybe_update().expect("update ok").expect("had stats");
        assert!(stats.loss.is_finite(), "loss not finite: {}", stats.loss);
        assert_eq!(t.trajectory_len(), 0, "buffer cleared after update");
    }

    #[test]
    fn trainer_no_update_below_batch() {
        let mut t = Trainer::new(16, 4, 1e-2).expect("new");
        let obs = Observation::zero();
        let _ = t.decide(&obs).unwrap();
        t.observe_reward(0.5);
        assert!(t.maybe_update().unwrap().is_none());
    }
}
