//! End-to-end sanity check: a random image + mel spectrogram flow through
//! the whole custom architecture and produce 4 action logits without
//! shape errors or NaNs.
//!
//! Not a correctness test — just proves the pieces wire together.

#![cfg(feature = "rl-train")]

use candle_core::{DType, Device, Tensor};
use rustic_rl::arch::{ArchConfig, OmniModel};

#[test]
fn tiny_config_produces_four_logits() {
    let device = Device::Cpu;
    let cfg = ArchConfig::tiny();
    cfg.validate().expect("tiny config should validate");

    let (model, _varmap) = OmniModel::fresh(cfg.clone(), &device).expect("build model");

    let batch = 2;
    let image = Tensor::rand(
        0f32,
        1.0,
        (
            batch,
            cfg.vision.in_channels,
            cfg.vision.image_size,
            cfg.vision.image_size,
        ),
        &device,
    )
    .unwrap();

    // Use a realistic frame count: ~1 second of 10ms-hop mel frames, then
    // divisible by 4 (two stride-2 subsample layers).
    let mel_frames = 96;
    let mel = Tensor::rand(0f32, 1.0, (batch, cfg.audio.mel_bins, mel_frames), &device).unwrap();

    let logits = model.forward(&image, &mel).expect("forward pass");
    assert_eq!(
        logits.dims(),
        &[batch, cfg.action_classes],
        "expected [B, 4] logits"
    );

    let logits_vec: Vec<f32> = logits.flatten_all().unwrap().to_vec1().expect("to_vec1");
    assert_eq!(logits_vec.len(), batch * cfg.action_classes);
    for (i, v) in logits_vec.iter().enumerate() {
        assert!(
            v.is_finite(),
            "logit[{i}] = {v} is not finite — model likely blew up"
        );
    }
}

#[test]
fn tiny_config_reports_parameter_count_within_range() {
    // Quick param-count sanity check. Tiny should be a few million params.
    let cfg = ArchConfig::tiny();
    let device = Device::Cpu;
    let (_model, varmap) = OmniModel::fresh(cfg, &device).expect("build");

    let total: usize = varmap
        .all_vars()
        .iter()
        .map(|v| v.as_tensor().elem_count())
        .sum();

    // Tiny preset targets roughly 5-15M params. Wide bracket because the
    // exact number depends on patch/token counts.
    assert!(
        (1_000_000..50_000_000).contains(&total),
        "expected 1M-50M params, got {total}"
    );

    // Also: everything should be f32 so VarMap is a predictable size.
    for v in varmap.all_vars() {
        assert_eq!(v.as_tensor().dtype(), DType::F32);
    }
}
