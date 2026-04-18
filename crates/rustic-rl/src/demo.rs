//! Append-only NDJSON recorder for gameplay trajectories.
//!
//! Whenever the game runs with RL enabled we record `(obs, action, reward)`
//! tuples as one-JSON-line-per-tick. These files are the raw material for
//! behavior-cloning pretraining: before switching to REINFORCE we first
//! teach the model to mimic recorded human play so it has a clue what a
//! note is and how to hit it.
//!
//! The file layout is deliberately flat:
//!   ~/.rustic_rl/demos/<song>_<diff>_<unix-ts>.jsonl
//!
//! Each line is an independent `DemoStep` JSON — no wrapping array, no
//! session metadata. Easy to `cat` together and easy to truncate if
//! something goes sideways.

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::observe::{Action, Observation};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoStep {
    pub obs: Observation,
    pub action: Action,
    pub reward: f32,
}

/// Writes `DemoStep`s to disk as NDJSON. One file per song/session.
pub struct DemoRecorder {
    writer: Option<BufWriter<File>>,
    path: PathBuf,
    step_count: usize,
}

impl DemoRecorder {
    /// Create a recorder for a given song/difficulty. File is created lazily
    /// on the first `record` call so an unused recorder leaves no disk trace.
    pub fn new(song: &str, difficulty: &str) -> std::io::Result<Self> {
        Self::new_in(&demo_dir(), song, difficulty)
    }

    /// Same as `new` but writes into an explicit directory. Mostly useful
    /// for tests that don't want to touch `$HOME`.
    pub fn new_in(dir: &Path, song: &str, difficulty: &str) -> std::io::Result<Self> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let safe_song = sanitize(song);
        let safe_diff = sanitize(difficulty);
        let path = dir.join(format!("{safe_song}_{safe_diff}_{ts}.jsonl"));
        Ok(Self {
            writer: None,
            path,
            step_count: 0,
        })
    }

    /// Directory where all demo files live.
    pub fn dir() -> PathBuf {
        demo_dir()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn step_count(&self) -> usize {
        self.step_count
    }

    /// Append one step. Lazily opens the file on first call.
    pub fn record(&mut self, step: &DemoStep) -> std::io::Result<()> {
        if self.writer.is_none() {
            if let Some(parent) = self.path.parent() {
                fs::create_dir_all(parent)?;
            }
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            self.writer = Some(BufWriter::new(file));
        }
        let line = serde_json::to_string(step)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let w = self.writer.as_mut().expect("writer just initialized");
        w.write_all(line.as_bytes())?;
        w.write_all(b"\n")?;
        self.step_count += 1;
        Ok(())
    }

    /// Flush the buffer. Caller should invoke on song end / shutdown.
    pub fn flush(&mut self) -> std::io::Result<()> {
        if let Some(w) = self.writer.as_mut() {
            w.flush()?;
        }
        Ok(())
    }
}

impl Drop for DemoRecorder {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn demo_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".rustic_rl").join("demos");
    }
    PathBuf::from(".rustic_rl/demos")
}

/// Load every `.jsonl` file in the demo directory into a flat vector.
/// Invalid lines are skipped with a warning; an empty / missing directory
/// returns `Ok(vec![])`.
pub fn load_all_demos() -> std::io::Result<Vec<DemoStep>> {
    load_demos_from(&demo_dir())
}

/// Same as `load_all_demos` but reads from an explicit directory.
pub fn load_demos_from(dir: &Path) -> std::io::Result<Vec<DemoStep>> {
    let per_file = load_demo_files_from(dir)?;
    Ok(per_file.into_iter().flat_map(|f| f.steps).collect())
}

/// One demo file on disk, decoded. `name` is the filename only (no
/// directory), `size` is the raw byte length of the file at load time —
/// together they make a cheap fingerprint for "have we already BC'd on
/// this?" tracking.
#[derive(Debug, Clone)]
pub struct DemoFile {
    pub name: String,
    pub size: u64,
    pub steps: Vec<DemoStep>,
}

/// Load every `.jsonl` file in `dir` and return them per-file. Caller can
/// fingerprint individual files and filter before flattening into a training
/// corpus. Invalid lines are skipped with a warning.
pub fn load_demo_files_from(dir: &Path) -> std::io::Result<Vec<DemoFile>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()).map(String::from) else {
            continue;
        };
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let text = fs::read_to_string(&path)?;
        let mut steps = Vec::new();
        for (line_no, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<DemoStep>(line) {
                Ok(step) => steps.push(step),
                Err(e) => {
                    log::warn!(
                        "rustic-rl: skipping bad demo line {}:{}: {}",
                        path.display(),
                        line_no + 1,
                        e
                    );
                }
            }
        }
        out.push(DemoFile { name, size, steps });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::Observation;

    #[test]
    fn record_writes_ndjson_and_reloads() {
        let tmp = tempdir();
        let mut rec = DemoRecorder::new_in(&tmp, "song-A", "hard").expect("new");
        let step = DemoStep {
            obs: Observation::zero(),
            action: Action {
                press: [true, false, true, false],
            },
            reward: 1.5,
        };
        rec.record(&step).expect("record");
        rec.record(&step).expect("record2");
        rec.flush().expect("flush");
        drop(rec);

        let all = load_demos_from(&tmp).expect("load");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].action.press, [true, false, true, false]);
        assert_eq!(all[0].reward, 1.5);

        std::fs::remove_dir_all(tmp).ok();
    }

    fn tempdir() -> std::path::PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("rustic_rl_test_{ts}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
