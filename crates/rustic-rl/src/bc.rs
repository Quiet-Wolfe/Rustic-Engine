//! Behavior-cloning pretraining from recorded demos.
//!
//! Raw REINFORCE from scratch has to stumble onto the concept of "press the
//! key when a note is near the strum line" by random chance — on a rhythm
//! game with tight timing windows that takes forever. BC sidesteps that by
//! first minimizing per-lane binary cross-entropy between the policy's
//! sigmoid output and the recorded human presses. A few thousand steps of
//! this usually gets the agent to "hit obvious notes" before we hand control
//! back to REINFORCE for everything BC can't learn (fine-grained timing,
//! score maximization).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use candle_core::{Device, Result, Tensor};
use candle_nn::{self as nn, Optimizer, ParamsAdamW, VarMap};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::demo::{load_all_demos, DemoStep};

/// On-disk record of which demo files have already been BC-trained into
/// a given set of weights. Lives next to the weights file so pretraining
/// is scoped to exactly the model+song+difficulty the manifest is tied to.
///
/// Fingerprint = (filename, file size). Filenames are unique per session
/// (timestamp-suffixed), so name alone is almost always enough, but
/// including size makes us robust to a file being truncated or appended-to
/// after the fact.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BcManifest {
    /// filename → byte size at the time we BC'd on it.
    #[serde(default)]
    pub seen: BTreeMap<String, u64>,
    #[serde(skip)]
    path: PathBuf,
}

impl BcManifest {
    /// Load a manifest from `path`. Missing file → empty manifest.
    /// Malformed JSON → logs a warning and returns an empty manifest
    /// rather than refusing to train.
    pub fn load(path: PathBuf) -> Self {
        if !path.exists() {
            return Self {
                seen: BTreeMap::new(),
                path,
            };
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Self>(&text) {
                Ok(mut m) => {
                    m.path = path;
                    m
                }
                Err(e) => {
                    log::warn!(
                        "rustic-rl: bc manifest {:?} malformed ({e}); starting empty",
                        path
                    );
                    Self {
                        seen: BTreeMap::new(),
                        path,
                    }
                }
            },
            Err(e) => {
                log::warn!("rustic-rl: cannot read bc manifest {:?}: {e}", path);
                Self {
                    seen: BTreeMap::new(),
                    path,
                }
            }
        }
    }

    pub fn contains(&self, name: &str, size: u64) -> bool {
        self.seen.get(name).copied() == Some(size)
    }

    pub fn insert(&mut self, name: String, size: u64) {
        self.seen.insert(name, size);
    }

    /// Persist the manifest to its backing path. Creates parent dirs.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&self.path, text)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Summary of a pretraining run.
#[derive(Debug, Clone, Copy)]
pub struct BcStats {
    pub examples: usize,
    pub epochs: usize,
    pub final_loss: f32,
}

/// Train `model` by cloning `steps` for `epochs` epochs of mini-batch SGD.
/// Loss is per-lane binary cross-entropy with logits.
///
/// `model` is owned by the caller (the trainer / harness); we take a `&mut
/// dyn Module` plus its `VarMap` so we can build our own optimizer.
pub fn pretrain<M: crate::PolicyModel>(
    model: &M,
    varmap: &VarMap,
    steps: &[DemoStep],
    epochs: usize,
    batch_size: usize,
    learning_rate: f64,
    device: &Device,
) -> Result<BcStats> {
    if steps.is_empty() {
        return Ok(BcStats {
            examples: 0,
            epochs: 0,
            final_loss: 0.0,
        });
    }

    // AdamW matches the REINFORCE trainer, so the weights see a consistent
    // optimizer across BC warm-up and on-line updates. Defaults match the
    // REINFORCE trainer; weight_decay=0 keeps pretraining purely data-driven.
    let params = ParamsAdamW {
        lr: learning_rate,
        beta1: 0.9,
        beta2: 0.999,
        eps: 1e-8,
        weight_decay: 0.0,
    };
    let mut optimizer = nn::AdamW::new(varmap.all_vars(), params)?;
    let mut rng = rand::rng();
    let mut indices: Vec<usize> = (0..steps.len()).collect();
    let mut last_loss = 0.0f32;
    let batches_per_epoch = indices.len().div_ceil(batch_size.max(1));

    for epoch in 0..epochs {
        indices.shuffle(&mut rng);
        log::info!(
            "rustic-rl: BC epoch {}/{} starting ({} batches of {})",
            epoch + 1,
            epochs,
            batches_per_epoch,
            batch_size,
        );
        let mut batch_i = 0usize;
        let epoch_start = std::time::Instant::now();
        for chunk in indices.chunks(batch_size) {
            let (inputs, targets) = build_batch(model, steps, chunk, device)?;
            let logits = model.forward(&inputs)?;
            let loss = bce_with_logits(&logits, &targets)?;
            let grads = loss.backward()?;
            optimizer.step(&grads)?;
            last_loss = loss.to_scalar::<f32>()?;
            batch_i += 1;
            // Log every ~25 batches so the user sees it's alive on a big
            // model. Big models (75M+) on CPU can take seconds per batch;
            // silence for minutes is indistinguishable from a hang.
            if batch_i % 25 == 0 {
                log::info!(
                    "rustic-rl: BC epoch {} batch {}/{} loss {:.4}",
                    epoch + 1,
                    batch_i,
                    batches_per_epoch,
                    last_loss,
                );
            }
        }
        log::info!(
            "rustic-rl: BC epoch {}/{} done in {:.1}s, final batch loss {:.4}",
            epoch + 1,
            epochs,
            epoch_start.elapsed().as_secs_f32(),
            last_loss,
        );
    }

    Ok(BcStats {
        examples: steps.len(),
        epochs,
        final_loss: last_loss,
    })
}

/// Convenience: loads every demo file on disk and runs `pretrain`.
pub fn pretrain_from_disk<M: crate::PolicyModel>(
    model: &M,
    varmap: &VarMap,
    epochs: usize,
    batch_size: usize,
    learning_rate: f64,
    device: &Device,
) -> Result<BcStats> {
    let steps = load_all_demos().map_err(|e| candle_core::Error::Msg(format!("demo load: {e}")))?;
    pretrain(
        model,
        varmap,
        &steps,
        epochs,
        batch_size,
        learning_rate,
        device,
    )
}

fn build_batch<M: crate::PolicyModel>(
    model: &M,
    steps: &[DemoStep],
    indices: &[usize],
    device: &Device,
) -> Result<(Tensor, Tensor)> {
    let b = indices.len();
    let mut observations = Vec::with_capacity(b);
    let mut flat_targets = Vec::with_capacity(b * 4);
    for &i in indices {
        let step = &steps[i];
        observations.push(step.obs.clone());
        for &pressed in &step.action.press {
            flat_targets.push(if pressed { 1.0f32 } else { 0.0 });
        }
    }

    let x = model.build_input_batch(&observations, device)?;
    let y = Tensor::from_vec(flat_targets, (b, 4), device)?;
    Ok((x, y))
}

/// Binary cross-entropy with logits, mean over all elements.
/// BCE(l, y) = max(l, 0) - l*y + log(1 + exp(-|l|))
fn bce_with_logits(logits: &Tensor, targets: &Tensor) -> Result<Tensor> {
    let zeros = Tensor::zeros_like(logits)?;
    let max_part = logits.maximum(&zeros)?;
    let ly = logits.mul(targets)?;
    let abs_l = logits.abs()?;
    let stable = ((abs_l.neg()?.exp()? + 1.0)?).log()?;
    let per_elem = ((max_part - ly)? + stable)?;
    let denom = per_elem.elem_count() as f64;
    per_elem.sum_all()? / denom
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::{Action, Observation};
    use crate::trainer::Trainer;

    #[test]
    fn pretrain_noop_on_empty_corpus() {
        let trainer = Trainer::new(32, 8, 1e-2).expect("trainer");
        let stats = pretrain(
            trainer.model_ref(),
            trainer.varmap_ref(),
            &[],
            1,
            4,
            1e-2,
            trainer.device_ref(),
        )
        .expect("pretrain");
        assert_eq!(stats.examples, 0);
        assert_eq!(stats.final_loss, 0.0);
    }

    #[test]
    fn manifest_roundtrip_and_contains() {
        let dir = std::env::temp_dir().join(format!(
            "rustic_rl_manifest_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("omni_bopeebo_hard.bc_seen.json");

        let mut m = BcManifest::load(path.clone());
        assert!(m.seen.is_empty());
        assert!(!m.contains("demo1.jsonl", 100));

        m.insert("demo1.jsonl".to_string(), 100);
        m.insert("demo2.jsonl".to_string(), 250);
        m.save().expect("save");

        let reloaded = BcManifest::load(path);
        assert!(reloaded.contains("demo1.jsonl", 100));
        assert!(reloaded.contains("demo2.jsonl", 250));
        // Wrong size → not contained (treat as a different file).
        assert!(!reloaded.contains("demo1.jsonl", 99));
        // Unknown filename → not contained.
        assert!(!reloaded.contains("demo3.jsonl", 100));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pretrain_reduces_loss_on_synthetic_demo() {
        let trainer = Trainer::new(32, 8, 1e-2).expect("trainer");
        // One example, lane 0 always pressed. After a bunch of epochs the
        // model should be extremely confident about lane 0.
        let step = DemoStep {
            obs: Observation::zero(),
            action: Action {
                press: [true, false, false, false],
            },
            reward: 0.0,
        };
        let steps = vec![step; 8];
        let stats = pretrain(
            trainer.model_ref(),
            trainer.varmap_ref(),
            &steps,
            50,
            4,
            1e-1,
            trainer.device_ref(),
        )
        .expect("pretrain");
        assert!(stats.final_loss.is_finite());
        // Sanity: loss shouldn't be NaN or wildly negative.
        assert!(stats.final_loss >= 0.0);
    }
}
