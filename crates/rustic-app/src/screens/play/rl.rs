//! Per-frame RL glue: build an `Observation` from the live `PlayState`,
//! hand it to the harness, inject the agent's presses, and after the
//! gameplay tick feed reward + record demo line.
//!
//! Kept isolated so the feature-gate stays legible. Only compiled when
//! `rustic-app` is built with `--features rl`.

use rustic_render::gpu::GpuState;
use rustic_rl::{build_observation, Action, UpcomingNote};

use super::PlayScreen;

impl PlayScreen {
    pub(super) fn rl_pre_update(&mut self) {
        // Build an observation independently of whether a harness is
        // attached — it's cheap and keeps the branch-heavy glue in one
        // place. Skip if no harness.
        if self.rl_harness.is_none() {
            return;
        }

        let song_pos = self.game.conductor.song_position;
        let bpm = self.game.conductor.bpm;
        let health = (self.game.score.health / 2.0).clamp(0.0, 1.0); // normalize
        let keys_held = self.game.keys_held;

        let play_as_opponent = self.game.play_as_opponent;
        let upcoming = self.game.notes.iter().filter_map(|n| {
            // Only playable lanes for the human/agent side.
            let playable = if play_as_opponent {
                !n.must_press
            } else {
                n.must_press
            };
            if !playable || n.was_good_hit || n.too_late {
                return None;
            }
            let dt = (n.strum_time - song_pos) as f32;
            // Drop notes that are far past — they contribute no useful signal.
            if dt < -500.0 {
                return None;
            }
            Some(UpcomingNote {
                lane: n.lane,
                time_until_hit_ms: dt,
                sustain_ms: n.sustain_length as f32,
            })
        });

        let obs = build_observation(song_pos, bpm, health, keys_held, upcoming);

        let Some(harness) = self.rl_harness.as_mut() else {
            return;
        };
        let action = match harness.decide(&obs) {
            Ok(a) => a,
            Err(e) => {
                log::warn!("rustic-rl: decide failed: {e}");
                return;
            }
        };

        if harness.control_gameplay() {
            // Diff desired presses against what the game currently sees as
            // held, and synthesize press/release events for each lane that
            // changed. This reuses the normal gameplay paths so hit
            // detection, holds, and misses all work without extra logic.
            let held = self.game.keys_held;
            for lane in 0..4 {
                match (held[lane], action.press[lane]) {
                    (false, true) => self.game.key_press(lane),
                    (true, false) => self.game.key_release(lane),
                    _ => {}
                }
            }
        }
    }

    pub(super) fn rl_post_update(&mut self) {
        let Some(harness) = self.rl_harness.as_mut() else {
            return;
        };
        let score = self.game.score.score;
        let health = (self.game.score.health / 2.0).clamp(0.0, 1.0);

        // When we're not driving gameplay, the "human action" for BC is
        // the post-update key-held vector — that's what the player had
        // their fingers on during this tick.
        let human_action = if harness.control_gameplay() {
            None
        } else {
            Some(Action {
                press: self.game.keys_held,
            })
        };
        if let Err(e) = harness.end_step(score, health, human_action) {
            log::warn!("rustic-rl: end_step failed: {e}");
        }

        // Auto-restart: when RL is driving the run, short-circuit death and
        // song-end back into another attempt so training doesn't halt at the
        // death screen or results screen. Flush the recorder first so the
        // finished run is persisted.
        if self.game.dead || self.game.song_ended {
            harness.flush();
            self.wants_restart = true;
            self.game.dead = false;
            self.game.song_ended = false;
            // Clear the death screen state machine so next_screen doesn't wait
            // for the confirm animation.
            self.death = None;
        }
    }

    /// Flush any buffered demo steps. Called from song end / screen drop.
    pub(super) fn rl_flush(&mut self) {
        if let Some(h) = self.rl_harness.as_mut() {
            h.flush();
        }
    }

    /// Top-left telemetry + agent-view overlay, plus a larger attention
    /// panel on the right edge of the screen. Human-only — the observation
    /// the agent sees does not include any of this, so there's no
    /// information leakage.
    pub(super) fn draw_rl_hud(&self, gpu: &mut GpuState) {
        let Some(harness) = self.rl_harness.as_ref() else {
            return;
        };
        self.draw_attn_panel(gpu, harness);
        let white = [1.0, 1.0, 1.0, 0.9];
        let dim = [0.85, 0.85, 0.85, 0.75];

        let x = 14.0;
        let mut y = 14.0;
        let line_h = 18.0;

        // Model tag
        let model_tag = match harness.model_choice() {
            rustic_rl::ModelChoice::Omni => "OMNI",
            rustic_rl::ModelChoice::Mlp => "MLP",
        };
        let mode = if harness.control_gameplay() {
            "AGENT"
        } else {
            "RECORD"
        };

        // ── Panel 1: Training stats (top-left) ──
        let panel_h = line_h * 5.0 + 12.0;
        gpu.push_colored_quad(x - 6.0, y - 6.0, 290.0, panel_h, [0.0, 0.0, 0.0, 0.45]);
        gpu.draw_batch(None);

        gpu.draw_text(&format!("RL · {mode} · {model_tag}"), x, y, 16.0, white);
        y += line_h;

        gpu.draw_text(
            &format!(
                "buffer {}/{}",
                harness.trajectory_len(),
                harness.batch_size()
            ),
            x,
            y,
            14.0,
            dim,
        );
        y += line_h;

        gpu.draw_text(
            &format!("demos this run: {}", harness.demo_step_count()),
            x,
            y,
            14.0,
            dim,
        );
        y += line_h;

        if let Some(stats) = harness.last_stats {
            gpu.draw_text(
                &format!("upd #{}  loss {:.3}", stats.step, stats.loss),
                x,
                y,
                14.0,
                dim,
            );
            y += line_h;
            gpu.draw_text(
                &format!(
                    "reward µ {:+.3}  Σ {:+.2}",
                    stats.mean_reward, stats.total_reward
                ),
                x,
                y,
                14.0,
                dim,
            );
        } else {
            gpu.draw_text("no updates yet", x, y, 14.0, dim);
        }

        // BC warm-up indicator: shows how many demo steps were trained from
        // and the final loss, so you can tell your recorded sessions actually
        // fed the model.
        if let Some(bc) = harness.bc_stats {
            y += line_h;
            let green = [0.3, 1.0, 0.4, 0.9];
            gpu.draw_text(
                &format!(
                    "BC: {} examples, {} epochs, loss {:.3}",
                    bc.examples, bc.epochs, bc.final_loss
                ),
                x,
                y,
                13.0,
                green,
            );
        }

        // ── Panel 2: Observation viewer (below stats) ──
        let obs_y = 14.0 + panel_h + 8.0;
        self.draw_obs_viewer(gpu, harness, x, obs_y);
    }

    /// Draw a compact visualization of what the agent sees: per-lane upcoming
    /// notes as proximity bars, sigmoid probabilities, and the action mask.
    /// For the OmniModel, also renders a small attention heatmap.
    fn draw_obs_viewer(&self, gpu: &mut GpuState, harness: &rustic_rl::Harness, x: f32, y: f32) {
        let dim = [0.7, 0.7, 0.7, 0.7];
        let green = [0.3, 1.0, 0.4, 0.85];
        let red = [1.0, 0.35, 0.35, 0.85];
        let cyan = [0.4, 0.85, 1.0, 0.9];

        let lane_w = 62.0;
        let total_w = lane_w * 4.0 + 24.0;
        let line_h = 16.0;
        let row_h = 22.0;

        // Panel background
        let panel_h = row_h * 5.5 + line_h * 2.0 + 10.0;
        gpu.push_colored_quad(x - 6.0, y - 6.0, total_w, panel_h, [0.0, 0.0, 0.0, 0.5]);
        gpu.draw_batch(None);

        gpu.draw_text("AGENT VIEW", x, y, 13.0, cyan);
        let mut cy = y + line_h + 2.0;

        // Build observation from current game state for display
        let song_pos = self.game.conductor.song_position;
        let keys_held = self.game.keys_held;
        let play_as_opponent = self.game.play_as_opponent;

        let lane_names = ["LEFT", "DOWN", "UP", "RIGHT"];
        let lane_colors: [[f32; 4]; 4] = [
            [0.9, 0.4, 0.9, 0.8], // purple
            [0.2, 0.6, 1.0, 0.8], // blue
            [0.2, 1.0, 0.4, 0.8], // green
            [1.0, 0.4, 0.3, 0.8], // red
        ];

        // Per-lane: upcoming note proximity bars
        for lane in 0..4 {
            let lx = x + lane as f32 * lane_w;

            // Lane label
            gpu.draw_text(lane_names[lane], lx, cy, 11.0, lane_colors[lane]);

            // Find nearest upcoming note for this lane
            let mut nearest_dt = f32::MAX;
            for n in &self.game.notes {
                let playable = if play_as_opponent {
                    !n.must_press
                } else {
                    n.must_press
                };
                if !playable || n.was_good_hit || n.too_late || n.lane != lane {
                    continue;
                }
                let dt = (n.strum_time - song_pos) as f32;
                if dt >= -500.0 && dt.abs() < nearest_dt.abs() {
                    nearest_dt = dt;
                }
            }

            // Proximity bar: 0ms = full bar, 2000ms = empty
            let proximity = if nearest_dt < f32::MAX {
                (1.0 - (nearest_dt / 2000.0).abs()).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let bar_w = lane_w - 8.0;
            let bar_h = 6.0;
            let bar_x = lx + 2.0;
            let bar_y = cy + 12.0;

            // Background
            gpu.push_colored_quad(bar_x, bar_y, bar_w, bar_h, [0.2, 0.2, 0.2, 0.5]);
            // Fill
            let fill_color = if nearest_dt < 200.0 && nearest_dt > -100.0 {
                green
            } else if nearest_dt < 500.0 {
                [1.0, 0.9, 0.2, 0.85]
            } else {
                lane_colors[lane]
            };
            gpu.push_colored_quad(bar_x, bar_y, bar_w * proximity, bar_h, fill_color);
            gpu.draw_batch(None);

            // Time label
            if nearest_dt < f32::MAX {
                let label = if nearest_dt > 0.0 {
                    format!("{:.0}ms", nearest_dt)
                } else {
                    format!("{:.0}!", nearest_dt)
                };
                gpu.draw_text(&label, bar_x, bar_y + bar_h + 1.0, 10.0, dim);
            }
        }

        cy += row_h * 2.2;

        // Sigmoid probabilities from the model
        let probs = harness.last_probs();
        let is_omni = matches!(harness.model_choice(), rustic_rl::ModelChoice::Omni);

        gpu.draw_text(
            if is_omni {
                "SIGMOID P(lane)"
            } else {
                "MODEL OUT"
            },
            x,
            cy,
            11.0,
            cyan,
        );
        cy += line_h;

        for lane in 0..4 {
            let lx = x + lane as f32 * lane_w;
            let p = probs[lane];

            // Sigmoid bar
            let bar_w = lane_w - 8.0;
            let bar_h = 8.0;
            let bar_x = lx + 2.0;

            // Background (below threshold = dark, above = bright)
            gpu.push_colored_quad(bar_x, cy, bar_w, bar_h, [0.15, 0.15, 0.15, 0.5]);

            // Fill with color gradient: below 0.5 = blue-ish, above = green
            let color = if p >= 0.5 {
                green
            } else {
                [0.3, 0.5, 0.8, 0.7]
            };
            gpu.push_colored_quad(bar_x, cy, bar_w * p, bar_h, color);

            // Threshold line at 0.5
            let thresh_x = bar_x + bar_w * 0.5;
            gpu.push_colored_quad(thresh_x, cy - 1.0, 1.0, bar_h + 2.0, [1.0, 1.0, 1.0, 0.3]);
            gpu.draw_batch(None);

            // Probability value
            gpu.draw_text(&format!("{:.2}", p), bar_x, cy + bar_h + 1.0, 10.0, dim);
        }

        cy += row_h;

        // Action output: which lanes the model decided to press
        gpu.draw_text("ACTION", x, cy, 11.0, cyan);
        cy += line_h;

        for lane in 0..4 {
            let lx = x + lane as f32 * lane_w;
            let pressed = probs[lane] >= 0.5;
            let held = keys_held[lane];

            // Circle indicator: filled = pressed, ring = not pressed
            let cx = lx + lane_w / 2.0;
            let r = 7.0;
            let color = if pressed && held {
                green
            } else if pressed {
                [1.0, 0.9, 0.2, 0.8]
            } else if held {
                red
            } else {
                [0.3, 0.3, 0.3, 0.5]
            };

            gpu.push_colored_quad(cx - r, cy - r, r * 2.0, r * 2.0, color);
            gpu.draw_batch(None);
        }

        // Attention heatmap is rendered in its own right-side panel by
        // `draw_attn_panel` — keeps this overlay compact.
        let _ = dim;
        let _ = cy;
    }

    /// Dedicated attention + token-output panel on the right side of the
    /// screen. Large enough to actually read cell values at a glance,
    /// which the tiny 10px heatmap wasn't.
    fn draw_attn_panel(&self, gpu: &mut GpuState, harness: &rustic_rl::Harness) {
        let Some(heatmap) = harness.last_attn_heatmap() else {
            return;
        };

        // Expect a [5, 5] symbolic heatmap (4 lanes + 1 context token).
        // Any other shape: bail rather than guessing.
        let Ok(values) = heatmap.to_vec2::<f32>() else {
            return;
        };
        if values.len() != 5 || values.iter().any(|r| r.len() != 5) {
            return;
        }

        let white = [1.0, 1.0, 1.0, 0.92];
        let dim = [0.8, 0.8, 0.8, 0.75];
        let cyan = [0.4, 0.85, 1.0, 0.92];
        let faint = [0.6, 0.6, 0.6, 0.55];

        // Layout — anchored to the right edge of the game canvas.
        let cell = 34.0f32;
        let gap = 2.0f32;
        let n = 5usize;
        let grid_w = cell * n as f32 + gap * (n as f32 - 1.0);
        let grid_h = grid_w;
        let label_pad = 18.0f32; // room for row/col token labels
        let title_h = 20.0f32;
        let legend_h = 40.0f32;
        let panel_pad = 10.0f32;

        let panel_w = grid_w + label_pad + panel_pad * 2.0;
        let panel_h = grid_h + label_pad + title_h + legend_h + panel_pad * 2.0;
        let panel_x = crate::GAME_W - panel_w - 14.0;
        let panel_y = 14.0;

        // Panel background
        gpu.push_colored_quad(panel_x, panel_y, panel_w, panel_h, [0.0, 0.0, 0.0, 0.55]);
        gpu.draw_batch(None);

        // Title
        gpu.draw_text(
            "MODEL ATTENTION",
            panel_x + panel_pad,
            panel_y + panel_pad,
            14.0,
            cyan,
        );
        gpu.draw_text(
            "(rows = query, cols = key)",
            panel_x + panel_pad,
            panel_y + panel_pad + title_h - 4.0,
            10.0,
            faint,
        );

        let grid_x = panel_x + panel_pad + label_pad;
        let grid_y = panel_y + panel_pad + title_h + 6.0;

        // Column labels (along the top)
        let labels = ["L", "D", "U", "R", "C"];
        for (i, lbl) in labels.iter().enumerate() {
            let cx = grid_x + i as f32 * (cell + gap) + cell * 0.5 - 4.0;
            gpu.draw_text(lbl, cx, grid_y - 14.0, 12.0, dim);
        }

        // Row labels (along the left) and cells
        for row in 0..n {
            let ry = grid_y + row as f32 * (cell + gap);
            gpu.draw_text(
                labels[row],
                panel_x + panel_pad + 2.0,
                ry + cell * 0.5 - 6.0,
                12.0,
                dim,
            );
            for col in 0..n {
                let v = values[row][col].clamp(0.0, 1.0);
                // Heat gradient: low = dark blue, high = bright magenta-ish.
                // Diagonal in cyan so self-attention stands out at a glance.
                let color = if row == col {
                    [0.1 + 0.2 * v, 0.6 + 0.4 * v, 0.9, 0.9]
                } else {
                    let t = v.sqrt(); // gamma-boost visibility of small values
                    [0.2 + 0.75 * t, 0.1 + 0.2 * t, 0.3 + 0.55 * t, 0.9]
                };
                let cx = grid_x + col as f32 * (cell + gap);
                gpu.push_colored_quad(cx, ry, cell, cell, color);
            }
        }
        gpu.draw_batch(None);

        // Per-cell numeric values (drawn after quads so text sits on top).
        for row in 0..n {
            let ry = grid_y + row as f32 * (cell + gap);
            for col in 0..n {
                let v = values[row][col];
                let cx = grid_x + col as f32 * (cell + gap);
                // Text color flips for contrast on bright cells.
                let text_col = if v > 0.5 {
                    white
                } else {
                    [0.9, 0.9, 0.9, 0.75]
                };
                gpu.draw_text(
                    &format!("{v:.2}"),
                    cx + 4.0,
                    ry + cell * 0.5 - 6.0,
                    11.0,
                    text_col,
                );
            }
        }

        // Legend: per-lane sigmoid probs below the grid.
        let legend_y = grid_y + grid_h + 14.0;
        gpu.draw_text("PRESS P(lane)", panel_x + panel_pad, legend_y, 11.0, cyan);
        let probs = harness.last_probs();
        let bar_start = panel_x + panel_pad;
        let bar_full = panel_w - panel_pad * 2.0;
        let bar_h = 6.0;
        let by = legend_y + 14.0;
        gpu.push_colored_quad(bar_start, by, bar_full, bar_h, [0.15, 0.15, 0.15, 0.6]);
        for (i, p) in probs.iter().enumerate() {
            let lane_w = bar_full / 4.0;
            let bx = bar_start + i as f32 * lane_w;
            let fill_w = (lane_w - 2.0) * p.clamp(0.0, 1.0);
            let col = if *p >= 0.5 {
                [0.3, 1.0, 0.4, 0.9]
            } else {
                [0.3, 0.55, 0.85, 0.75]
            };
            gpu.push_colored_quad(bx + 1.0, by, fill_w, bar_h, col);
        }
        gpu.draw_batch(None);
        for (i, lbl) in ["L", "D", "U", "R"].iter().enumerate() {
            let lane_w = bar_full / 4.0;
            let bx = bar_start + i as f32 * lane_w;
            gpu.draw_text(lbl, bx + 2.0, by + bar_h + 2.0, 10.0, dim);
            gpu.draw_text(
                &format!("{:.2}", probs[i]),
                bx + lane_w * 0.4,
                by + bar_h + 2.0,
                10.0,
                faint,
            );
        }
    }
}
