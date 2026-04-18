//! End-to-end glue for live training inside the running game.
//!
//! The harness owns:
//!   * a `Trainer` (the policy + optimizer + trajectory buffer)
//!   * an optional `DemoRecorder` (writes every step to NDJSON on disk)
//!   * a small piece of reward state (previous score + previous health),
//!     so the PlayScreen only has to hand us "what's the score/health now"
//!
//! The game loop calls:
//!   1. `harness.decide(&obs)` — returns the desired per-lane press mask
//!   2. `harness.end_step(score, health)` — after the game advances one
//!      tick, we compute reward and step the REINFORCE buffer
//!
//! Recording and learning are independent: you can record demos without a
//! trainer attached (feature `rl-train` still required, but with the
//! record-only option the trainer simply never sees updates). This lets a
//! human play normally while we collect bootstrap data.

use candle_core::Result;

use crate::demo::{DemoRecorder, DemoStep};
use crate::observe::{Action, Observation};
use crate::omni_trainer::OmniTrainer;
use crate::trainer::{TrainStats, Trainer};

/// Where model weights live on disk. `rustic-rl/` next to the demo corpus.
const WEIGHTS_DIR: &str = "rustic-rl/weights";

/// Which model backend to use for RL training.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelChoice {
    /// Original 2-layer MLP over the flat symbolic observation.
    Mlp,
    /// Gemma 4–inspired transformer backbone: projects per-lane features into
    /// tokens, runs full self-attention, outputs 4 independent sigmoid logits
    /// (Path B: multi-label, threshold at 0.5).
    Omni,
}

impl Default for ModelChoice {
    fn default() -> Self {
        Self::Omni
    }
}

/// Which `ArchConfig` preset the Omni backbone should use. Only applies
/// when `ModelChoice::Omni` is selected — the MLP backend has its own
/// `hidden` field on `HarnessConfig`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchSize {
    /// ~5-10M params. Fast; use for smoke tests.
    Tiny,
    /// ~30-60M params.
    Small,
    /// ~75M params. Default — balances capability with CPU-realtime inference.
    Large,
    /// ~125M params. Heavyweight, borderline on CPU inference.
    Huge,
}

impl ArchSize {
    pub fn tag(&self) -> &'static str {
        match self {
            ArchSize::Tiny => "tiny",
            ArchSize::Small => "small",
            ArchSize::Large => "large",
            ArchSize::Huge => "huge",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "tiny" => Some(ArchSize::Tiny),
            "small" => Some(ArchSize::Small),
            "large" => Some(ArchSize::Large),
            "huge" => Some(ArchSize::Huge),
            _ => None,
        }
    }

    fn to_config(self) -> crate::arch::config::ArchConfig {
        use crate::arch::config::ArchConfig;
        match self {
            ArchSize::Tiny => ArchConfig::tiny(),
            ArchSize::Small => ArchConfig::small(),
            ArchSize::Large => ArchConfig::large(),
            ArchSize::Huge => ArchConfig::huge(),
        }
    }
}

impl Default for ArchSize {
    fn default() -> Self {
        Self::Large
    }
}

/// How the harness weighs various signals when computing per-tick reward.
/// All zeros = no reward (useful for record-only sessions).
#[derive(Debug, Clone, Copy)]
pub struct RewardWeights {
    pub score_delta: f32,
    pub health_delta: f32,
    /// Subtracted every tick for each lane pressed while a note IS within the
    /// hit window (±`hit_window_ms`). Small — pressing at the right time is
    /// how you score, so we don't punish it hard.
    pub press_penalty_near_note: f32,
    /// Subtracted every tick for each lane pressed while NO note is within
    /// `hit_window_ms`. This is the spam killer — needs to be large enough
    /// to dominate the reward signal when the agent mashes blindly.
    pub press_penalty_no_note: f32,
    /// How close (in ms) a note must be to count as "nearby" for the
    /// reduced penalty. Matches Psych Engine's Shit window (166ms) by
    /// default, but slightly generous to avoid penalizing early presses
    /// that would still hit.
    pub hit_window_ms: f32,
}

impl Default for RewardWeights {
    fn default() -> Self {
        Self {
            score_delta: 0.002,
            health_delta: 1.0,
            press_penalty_near_note: 0.000,
            press_penalty_no_note: 0.005,
            hit_window_ms: 200.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HarnessConfig {
    pub hidden: usize,
    pub batch_size: usize,
    pub learning_rate: f64,
    pub reward_weights: RewardWeights,
    /// When true the harness acts on its own action, injecting presses.
    /// When false the harness only records — the human plays.
    pub control_gameplay: bool,
    /// When true we append every step to the on-disk demo corpus.
    pub record_demos: bool,
    /// Which model backend to train.
    pub model: ModelChoice,
    /// Which `ArchConfig` preset to use for the Omni backbone.
    /// Ignored for the MLP backend.
    pub arch_size: ArchSize,
}

impl HarnessConfig {
    pub fn live_train() -> Self {
        Self {
            hidden: 128,
            batch_size: 64,
            learning_rate: 3e-3,
            reward_weights: RewardWeights::default(),
            control_gameplay: true,
            record_demos: true,
            model: ModelChoice::Omni,
            arch_size: ArchSize::default(),
        }
    }

    pub fn live_train_mlp() -> Self {
        Self {
            hidden: 128,
            batch_size: 64,
            learning_rate: 3e-3,
            reward_weights: RewardWeights::default(),
            control_gameplay: true,
            record_demos: true,
            model: ModelChoice::Mlp,
            arch_size: ArchSize::default(),
        }
    }

    pub fn record_only() -> Self {
        Self {
            hidden: 128,
            batch_size: 64,
            learning_rate: 3e-3,
            reward_weights: RewardWeights::default(),
            control_gameplay: false,
            record_demos: true,
            model: ModelChoice::Omni,
            arch_size: ArchSize::default(),
        }
    }
}

/// The active trainer backend — either the original MLP or the Gemma 4–inspired
/// OmniModel transformer. Both share the same REINFORCE loop; only the forward
/// pass and parameter set differ.
enum TrainerBackend {
    Mlp(Trainer),
    Omni(OmniTrainer),
}

pub struct Harness {
    backend: TrainerBackend,
    recorder: Option<DemoRecorder>,
    cfg: HarnessConfig,
    last_score: i32,
    last_health: f32,
    /// What the agent pressed in the previous tick, used to detect rising edges.
    last_press: [bool; 4],
    pending_step: Option<PendingStep>,
    pub last_stats: Option<TrainStats>,
    /// Path to the safetensors weight file for this model+song+difficulty.
    weights_path: std::path::PathBuf,
    /// Results from the last BC warm-up (set by bootstrap_from_disk).
    pub bc_stats: Option<crate::bc::BcStats>,
}

struct PendingStep {
    obs: Observation,
    action: Action,
}

impl Harness {
    pub fn new(cfg: HarnessConfig, song: &str, difficulty: &str) -> Result<Self> {
        let model_tag = match cfg.model {
            ModelChoice::Mlp => "mlp",
            ModelChoice::Omni => "omni",
        };
        // Weights are scoped to (model backend, arch size, song, difficulty)
        // so changing any of those starts with fresh weights instead of
        // choking on shape-mismatches when loading the safetensors.
        let arch_tag = cfg.arch_size.tag();
        let weights_path = std::path::PathBuf::from(WEIGHTS_DIR).join(format!(
            "{model_tag}_{arch_tag}_{song}_{difficulty}.safetensors"
        ));

        let mut backend = match cfg.model {
            ModelChoice::Mlp => {
                TrainerBackend::Mlp(Trainer::new(cfg.hidden, cfg.batch_size, cfg.learning_rate)?)
            }
            ModelChoice::Omni => {
                let arch = cfg.arch_size.to_config();
                TrainerBackend::Omni(OmniTrainer::new(arch, cfg.batch_size, cfg.learning_rate)?)
            }
        };

        // Load any existing weights so training resumes across restarts.
        let loaded = match &mut backend {
            TrainerBackend::Mlp(t) => t.load(&weights_path).unwrap_or(false),
            TrainerBackend::Omni(t) => t.load(&weights_path).unwrap_or(false),
        };

        let recorder = if cfg.record_demos {
            match DemoRecorder::new(song, difficulty) {
                Ok(r) => Some(r),
                Err(e) => {
                    log::warn!("rustic-rl: demo recorder disabled ({e})");
                    None
                }
            }
        } else {
            None
        };
        let param_count = match &backend {
            TrainerBackend::Mlp(t) => t.param_count(),
            TrainerBackend::Omni(t) => t.param_count(),
        };
        log::info!(
            "rustic-rl: harness created with {:?} model ({} params, weights {})",
            cfg.model,
            param_count,
            if loaded {
                "loaded from disk"
            } else {
                "fresh random init"
            },
        );
        Ok(Self {
            backend,
            recorder,
            cfg,
            last_score: 0,
            last_health: 1.0,
            last_press: [false; 4],
            pending_step: None,
            last_stats: None,
            weights_path,
            bc_stats: None,
        })
    }

    /// Attempt a BC warm-up from any demos already on disk. Skips demo
    /// files that have already been BC-trained into this weights file
    /// (tracked via a sibling manifest) so repeated launches don't
    /// overfit on the same corpus.
    pub fn bootstrap_from_disk(&mut self, epochs: usize) -> Result<()> {
        let files = crate::demo::load_demo_files_from(&crate::demo::DemoRecorder::dir())
            .map_err(|e| candle_core::Error::Msg(format!("demo load: {e}")))?;

        let mut manifest = crate::bc::BcManifest::load(self.manifest_path());

        // Filter to files we haven't BC'd on yet.
        let mut fresh = Vec::new();
        let mut fresh_files: Vec<(String, u64)> = Vec::new();
        let mut skipped = 0usize;
        for f in files {
            if manifest.contains(&f.name, f.size) {
                skipped += 1;
                continue;
            }
            fresh_files.push((f.name, f.size));
            fresh.extend(f.steps);
        }

        if fresh.is_empty() {
            if skipped > 0 {
                log::info!(
                    "rustic-rl: BC skipped — all {} demo files on disk already seen",
                    skipped
                );
            } else {
                log::info!("rustic-rl: no demos on disk, skipping BC warm-up");
            }
            self.bc_stats = None;
            return Ok(());
        }

        log::info!(
            "rustic-rl: BC — {} new files ({} steps), {} skipped as already-seen",
            fresh_files.len(),
            fresh.len(),
            skipped,
        );

        let stats = match &mut self.backend {
            TrainerBackend::Mlp(trainer) => {
                use crate::bc;
                bc::pretrain(
                    trainer.model_ref(),
                    trainer.varmap_ref(),
                    &fresh,
                    epochs,
                    self.cfg.batch_size,
                    self.cfg.learning_rate,
                    trainer.device_ref(),
                )?
            }
            TrainerBackend::Omni(trainer) => {
                use crate::bc;
                bc::pretrain(
                    trainer.model_ref(),
                    trainer.varmap_ref(),
                    &fresh,
                    epochs,
                    self.cfg.batch_size,
                    self.cfg.learning_rate,
                    trainer.device_ref(),
                )?
            }
        };

        // Only mark files as seen AFTER the training call succeeded; a crash
        // mid-training shouldn't skip those files on the next launch.
        for (name, size) in fresh_files {
            manifest.insert(name, size);
        }
        if let Err(e) = manifest.save() {
            log::warn!("rustic-rl: failed to persist BC manifest: {e}");
        }

        log::info!(
            "rustic-rl: BC warm-up on {} demo steps ({} epochs), final loss {:.4}",
            stats.examples,
            stats.epochs,
            stats.final_loss
        );
        self.bc_stats = Some(stats);
        Ok(())
    }

    /// Where to keep the "which demo files have we BC'd on" manifest.
    /// Scoped to the same (model, song, difficulty) as the weights so
    /// swapping song/difficulty starts with a fresh seen-set.
    fn manifest_path(&self) -> std::path::PathBuf {
        self.weights_path.with_extension("bc_seen.json")
    }

    pub fn control_gameplay(&self) -> bool {
        self.cfg.control_gameplay
    }

    pub fn model_choice(&self) -> ModelChoice {
        self.cfg.model
    }

    /// Sample an action and stash the observation so `end_step` can pair
    /// them with a reward. If `control_gameplay` is false the returned
    /// action is ignored by the caller but still recorded.
    pub fn decide(&mut self, obs: &Observation) -> Result<Action> {
        let action = if self.cfg.control_gameplay {
            match &mut self.backend {
                TrainerBackend::Mlp(t) => t.decide(obs)?,
                TrainerBackend::Omni(t) => t.decide(obs)?,
            }
        } else {
            Action::default()
        };
        self.pending_step = Some(PendingStep {
            obs: obs.clone(),
            action,
        });
        Ok(action)
    }

    /// Record the outcome of the step started by `decide`. `human_action`,
    /// when present, overrides the agent's action in the demo — use this
    /// to save *what the human actually pressed* during record-only
    /// sessions, which is what BC needs.
    pub fn end_step(
        &mut self,
        score: i32,
        health: f32,
        human_action: Option<Action>,
    ) -> Result<()> {
        let Some(PendingStep { obs, action }) = self.pending_step.take() else {
            return Ok(());
        };

        let ds = (score - self.last_score) as f32;
        let dh = health - self.last_health;
        self.last_score = score;
        self.last_health = health;

        let w = self.cfg.reward_weights;

        // Contextual press penalty: check each pressed lane against whether a
        // note is actually nearby in the observation.
        let mut penalty = 0.0f32;
        for lane in 0..4 {
            if !action.press[lane] {
                continue;
            }

            let nearest = obs.upcoming[lane][0].0;
            let note_nearby = nearest.abs() <= w.hit_window_ms;

            // Spam tax: if the note is a normal note (not a long sustain),
            // it only counts as a valid attempt if it was a rising edge
            // (just pressed). If they just hold the key down, we charge
            // the full spam penalty every tick.
            let is_sustain = obs.upcoming[lane][0].1 > 50.0;
            let just_pressed = !self.last_press[lane];
            let valid_hit_attempt = is_sustain || just_pressed;

            if note_nearby && valid_hit_attempt {
                penalty += w.press_penalty_near_note;
            } else {
                penalty += w.press_penalty_no_note;
            }
        }

        self.last_press = action.press;
        let reward = w.score_delta * ds + w.health_delta * dh - penalty;

        if self.cfg.control_gameplay {
            match &mut self.backend {
                TrainerBackend::Mlp(t) => {
                    t.observe_reward(reward);
                    if let Some(stats) = t.maybe_update()? {
                        log::info!(
                            "rustic-rl update #{}: loss={:.4} mean_reward={:.4} total_reward={:.4}",
                            stats.step,
                            stats.loss,
                            stats.mean_reward,
                            stats.total_reward
                        );
                        self.last_stats = Some(stats);
                    }
                }
                TrainerBackend::Omni(t) => {
                    t.observe_reward(reward);
                    if let Some(stats) = t.maybe_update()? {
                        log::info!(
                            "rustic-rl [omni] update #{}: loss={:.4} mean_reward={:.4} total_reward={:.4}",
                            stats.step, stats.loss, stats.mean_reward, stats.total_reward
                        );
                        self.last_stats = Some(TrainStats {
                            step: stats.step,
                            loss: stats.loss,
                            mean_reward: stats.mean_reward,
                            total_reward: stats.total_reward,
                        });
                    }
                }
            }
        }

        if let Some(rec) = self.recorder.as_mut() {
            let recorded_action = human_action.unwrap_or(action);
            let step = DemoStep {
                obs,
                action: recorded_action,
                reward,
            };
            if let Err(e) = rec.record(&step) {
                log::warn!("rustic-rl: demo record failed ({e}); dropping recorder");
                self.recorder = None;
            }
        }
        Ok(())
    }

    /// Flush demo lines and persist model weights to disk. Called on
    /// song-end, death, and shutdown so training survives restarts.
    pub fn flush(&mut self) {
        if let Some(rec) = self.recorder.as_mut() {
            let _ = rec.flush();
        }
        match &self.backend {
            TrainerBackend::Mlp(t) => {
                if let Err(e) = t.save(&self.weights_path) {
                    log::warn!("rustic-rl: failed to save MLP weights: {e}");
                }
            }
            TrainerBackend::Omni(t) => {
                if let Err(e) = t.save(&self.weights_path) {
                    log::warn!("rustic-rl: failed to save Omni weights: {e}");
                }
            }
        }
    }

    pub fn trajectory_len(&self) -> usize {
        match &self.backend {
            TrainerBackend::Mlp(t) => t.trajectory_len(),
            TrainerBackend::Omni(t) => t.trajectory_len(),
        }
    }

    pub fn batch_size(&self) -> usize {
        self.cfg.batch_size
    }

    pub fn demo_step_count(&self) -> usize {
        self.recorder.as_ref().map(|r| r.step_count()).unwrap_or(0)
    }

    /// Per-lane sigmoid probabilities from the last decide call. Returns
    /// zeros for the MLP backend (which doesn't expose per-lane probs yet).
    pub fn last_probs(&self) -> [f32; 4] {
        match &self.backend {
            TrainerBackend::Omni(t) => t.last_probs(),
            TrainerBackend::Mlp(_) => [0.0; 4],
        }
    }

    /// Attention heatmap from the last OmniModel forward pass: a [T, T]
    /// tensor (5×5 for the symbolic path). Returns None for MLP backend.
    pub fn last_attn_heatmap(&self) -> Option<&candle_core::Tensor> {
        match &self.backend {
            TrainerBackend::Omni(t) => t.last_attn_heatmap(),
            TrainerBackend::Mlp(_) => None,
        }
    }
}
