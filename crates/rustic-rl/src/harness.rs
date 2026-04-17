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
use crate::trainer::{TrainStats, Trainer};

/// How the harness weighs various signals when computing per-tick reward.
/// All zeros = no reward (useful for record-only sessions).
#[derive(Debug, Clone, Copy)]
pub struct RewardWeights {
    pub score_delta: f32,
    pub health_delta: f32,
    /// Subtracted every tick — discourages mashing when the score isn't moving.
    pub press_penalty: f32,
}

impl Default for RewardWeights {
    fn default() -> Self {
        Self {
            // Psych Engine scores are big ints (hundreds to thousands per
            // note). Rescale so REINFORCE advantages stay in a sane range.
            score_delta: 0.001,
            health_delta: 1.0,
            press_penalty: 0.002,
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
        }
    }
}

pub struct Harness {
    trainer: Trainer,
    recorder: Option<DemoRecorder>,
    cfg: HarnessConfig,
    last_score: i32,
    last_health: f32,
    pending_step: Option<PendingStep>,
    pub last_stats: Option<TrainStats>,
}

struct PendingStep {
    obs: Observation,
    action: Action,
}

impl Harness {
    pub fn new(cfg: HarnessConfig, song: &str, difficulty: &str) -> Result<Self> {
        let trainer = Trainer::new(cfg.hidden, cfg.batch_size, cfg.learning_rate)?;
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
        Ok(Self {
            trainer,
            recorder,
            cfg,
            last_score: 0,
            last_health: 1.0,
            pending_step: None,
            last_stats: None,
        })
    }

    /// Attempt a BC warm-up from any demos already on disk. Silent no-op if
    /// the corpus is empty.
    pub fn bootstrap_from_disk(&mut self, epochs: usize) -> Result<()> {
        use crate::bc;
        let stats = bc::pretrain_from_disk(
            self.trainer.model_ref(),
            self.trainer.varmap_ref(),
            epochs,
            self.cfg.batch_size,
            self.cfg.learning_rate,
            self.trainer.device_ref(),
        )?;
        if stats.examples > 0 {
            log::info!(
                "rustic-rl: BC warm-up on {} demo steps ({} epochs), final loss {:.4}",
                stats.examples,
                stats.epochs,
                stats.final_loss
            );
        } else {
            log::info!("rustic-rl: no demos on disk, skipping BC warm-up");
        }
        Ok(())
    }

    pub fn control_gameplay(&self) -> bool {
        self.cfg.control_gameplay
    }

    /// Sample an action and stash the observation so `end_step` can pair
    /// them with a reward. If `control_gameplay` is false the returned
    /// action is ignored by the caller but still recorded (which makes the
    /// recorded demo represent what the agent *would* have done — cheap
    /// signal for validation).
    pub fn decide(&mut self, obs: &Observation) -> Result<Action> {
        let action = if self.cfg.control_gameplay {
            self.trainer.decide(obs)?
        } else {
            // Pure record path: emit an idle action so no log-probs queue up
            // in the trainer and pollute the trajectory buffer.
            Action::default()
        };
        self.pending_step = Some(PendingStep { obs: obs.clone(), action });
        Ok(action)
    }

    /// Record the outcome of the step started by `decide`. `human_action`,
    /// when present, overrides the agent's action in the demo — use this
    /// to save *what the human actually pressed* during record-only
    /// sessions, which is what BC needs.
    pub fn end_step(&mut self, score: i32, health: f32, human_action: Option<Action>) -> Result<()> {
        let Some(PendingStep { obs, action }) = self.pending_step.take() else {
            return Ok(());
        };

        let ds = (score - self.last_score) as f32;
        let dh = health - self.last_health;
        self.last_score = score;
        self.last_health = health;

        let w = self.cfg.reward_weights;
        let pressed = action.press.iter().filter(|p| **p).count() as f32;
        let reward = w.score_delta * ds + w.health_delta * dh - w.press_penalty * pressed;

        if self.cfg.control_gameplay {
            self.trainer.observe_reward(reward);
            if let Some(stats) = self.trainer.maybe_update()? {
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

        if let Some(rec) = self.recorder.as_mut() {
            let recorded_action = human_action.unwrap_or(action);
            let step = DemoStep { obs, action: recorded_action, reward };
            if let Err(e) = rec.record(&step) {
                log::warn!("rustic-rl: demo record failed ({e}); dropping recorder");
                self.recorder = None;
            }
        }
        Ok(())
    }

    /// Flush any buffered demo lines. Safe to call from song-end / shutdown.
    pub fn flush(&mut self) {
        if let Some(rec) = self.recorder.as_mut() {
            let _ = rec.flush();
        }
    }

    pub fn trajectory_len(&self) -> usize {
        self.trainer.trajectory_len()
    }

    pub fn demo_step_count(&self) -> usize {
        self.recorder.as_ref().map(|r| r.step_count()).unwrap_or(0)
    }
}
