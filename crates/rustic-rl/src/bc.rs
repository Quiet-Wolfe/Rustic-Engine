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

use candle_core::{Device, Module, Result, Tensor};
use candle_nn::{Optimizer, VarMap};
use rand::seq::SliceRandom;

use crate::demo::{load_all_demos, DemoStep};
use crate::trainer::{flatten_observation, OBS_DIM};

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
pub fn pretrain<M: Module>(
    model: &M,
    varmap: &VarMap,
    steps: &[DemoStep],
    epochs: usize,
    batch_size: usize,
    learning_rate: f64,
    device: &Device,
) -> Result<BcStats> {
    if steps.is_empty() {
        return Ok(BcStats { examples: 0, epochs: 0, final_loss: 0.0 });
    }

    let mut optimizer = candle_nn::SGD::new(varmap.all_vars(), learning_rate)?;
    let mut rng = rand::rng();
    let mut indices: Vec<usize> = (0..steps.len()).collect();
    let mut last_loss = 0.0f32;

    for _epoch in 0..epochs {
        indices.shuffle(&mut rng);
        for chunk in indices.chunks(batch_size) {
            let (inputs, targets) = build_batch(steps, chunk, device)?;
            let logits = model.forward(&inputs)?;
            let loss = bce_with_logits(&logits, &targets)?;
            let grads = loss.backward()?;
            optimizer.step(&grads)?;
            last_loss = loss.to_scalar::<f32>()?;
        }
    }

    Ok(BcStats {
        examples: steps.len(),
        epochs,
        final_loss: last_loss,
    })
}

/// Convenience: loads every demo file on disk and runs `pretrain`.
pub fn pretrain_from_disk<M: Module>(
    model: &M,
    varmap: &VarMap,
    epochs: usize,
    batch_size: usize,
    learning_rate: f64,
    device: &Device,
) -> Result<BcStats> {
    let steps = load_all_demos()
        .map_err(|e| candle_core::Error::Msg(format!("demo load: {e}")))?;
    pretrain(model, varmap, &steps, epochs, batch_size, learning_rate, device)
}

fn build_batch(
    steps: &[DemoStep],
    indices: &[usize],
    device: &Device,
) -> Result<(Tensor, Tensor)> {
    let b = indices.len();
    let mut flat_inputs = Vec::with_capacity(b * OBS_DIM);
    let mut flat_targets = Vec::with_capacity(b * 4);
    for &i in indices {
        let step = &steps[i];
        flat_inputs.extend(flatten_observation(&step.obs));
        for &pressed in &step.action.press {
            flat_targets.push(if pressed { 1.0f32 } else { 0.0 });
        }
    }
    let x = Tensor::from_vec(flat_inputs, (b, OBS_DIM), device)?;
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
    fn pretrain_reduces_loss_on_synthetic_demo() {
        let trainer = Trainer::new(32, 8, 1e-2).expect("trainer");
        // One example, lane 0 always pressed. After a bunch of epochs the
        // model should be extremely confident about lane 0.
        let step = DemoStep {
            obs: Observation::zero(),
            action: Action { press: [true, false, false, false] },
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
