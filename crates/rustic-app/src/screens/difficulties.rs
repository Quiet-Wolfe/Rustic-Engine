use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use rustic_core::paths::AssetPaths;

pub(crate) const STANDARD_DIFFICULTIES: [&str; 3] = ["easy", "normal", "hard"];

pub(crate) fn is_standard_difficulty(difficulty: &str) -> bool {
    STANDARD_DIFFICULTIES
        .iter()
        .any(|standard| standard.eq_ignore_ascii_case(difficulty))
}

pub(crate) fn sorted_difficulties(mut difficulties: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    difficulties.retain(|difficulty| seen.insert(difficulty.to_ascii_lowercase()));
    difficulties.sort_by(|a, b| {
        difficulty_rank(a)
            .cmp(&difficulty_rank(b))
            .then_with(|| a.cmp(b))
    });
    difficulties
}

pub(crate) fn detected_song_difficulties(paths: &AssetPaths, song_id: &str) -> Vec<String> {
    let mut difficulties = Vec::new();
    let mut seen = HashSet::new();
    let song_lower = song_id.to_ascii_lowercase();
    let prefixed = format!("{song_lower}-");

    for root in paths.roots() {
        let Some(dir) = song_data_dir(root, song_id) else {
            continue;
        };
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                continue;
            }

            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let stem_lower = stem.to_ascii_lowercase();
            if stem_lower == "events" {
                continue;
            }

            let difficulty = if stem_lower == song_lower {
                "normal"
            } else if let Some(difficulty) = stem_lower.strip_prefix(&prefixed) {
                difficulty
            } else {
                continue;
            };

            if difficulty.is_empty() {
                continue;
            }
            if seen.insert(difficulty.to_string()) {
                difficulties.push(difficulty.to_string());
            }
        }
    }

    sorted_difficulties(difficulties)
}

fn song_data_dir(root: &Path, song_id: &str) -> Option<PathBuf> {
    let data_dir = root.join("data");
    let direct = data_dir.join(song_id);
    if direct.is_dir() {
        return Some(direct);
    }

    let song_lower = song_id.to_ascii_lowercase();
    fs::read_dir(data_dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        if !path.is_dir() {
            return None;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        (name == song_lower).then_some(path)
    })
}

pub(crate) fn detected_song_difficulties_or_normal(
    paths: &AssetPaths,
    song_id: &str,
) -> Vec<String> {
    let difficulties = detected_song_difficulties(paths, song_id);
    if difficulties.is_empty() {
        vec!["normal".to_string()]
    } else {
        difficulties
    }
}

pub(crate) fn difficulty_rank(difficulty: &str) -> (usize, String) {
    match difficulty.to_ascii_lowercase().as_str() {
        "easy" => (0, String::new()),
        "normal" => (1, String::new()),
        "hard" => (2, String::new()),
        other => (3, other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rustic_difficulty_test_{}_{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn detects_custom_chart_difficulties() {
        let root = temp_root("custom");
        let song_dir = root.join("data").join("avarice");
        fs::create_dir_all(&song_dir).unwrap();
        fs::write(song_dir.join("avarice-hard.json"), "{}").unwrap();
        fs::write(song_dir.join("avarice-soulless.json"), "{}").unwrap();
        fs::write(song_dir.join("events.json"), "{}").unwrap();

        let mut paths = AssetPaths::new();
        paths.add_root(root.clone());

        assert_eq!(
            detected_song_difficulties(&paths, "avarice"),
            vec!["hard".to_string(), "soulless".to_string()]
        );

        let _ = fs::remove_dir_all(root);
    }
}
