use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PackJson {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}

#[derive(Debug, Clone)]
pub struct ModInfo {
    pub name: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub pack_json: Option<PackJson>,
    pub icon_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ModLoader {
    /// Active mod directories, priority order (first = highest).
    active_mods: Vec<ModInfo>,
    /// Base game path.
    base_game: PathBuf,
    mods_dir: PathBuf,
}

impl ModLoader {
    pub fn from_environment() -> Self {
        let base_game = env::var_os("RUSTIC_GAME_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("assets"));
        let mods_dir = env::var_os("RUSTIC_MODS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("mods"));
        Self::discover(base_game, mods_dir)
    }

    pub fn discover(base_game: PathBuf, mods_dir: PathBuf) -> Self {
        let mods_list = mods_list_path(&mods_dir);
        let entries = if is_mod_directory(&mods_dir) {
            vec![mod_info_from_path(&mods_dir)]
        } else {
            scan_mods(&mods_dir)
        };
        let by_name: HashMap<_, _> = entries
            .into_iter()
            .map(|info| (info.name.clone(), info))
            .collect();

        let mut active_mods = Vec::new();
        let mut seen = HashSet::new();

        for (name, enabled) in parse_mods_list(&mods_list) {
            let Some(mut info) = by_name.get(&name).cloned() else {
                continue;
            };
            seen.insert(name);
            info.enabled = enabled;
            if info.enabled {
                active_mods.push(info);
            }
        }

        let mut remaining: Vec<_> = by_name
            .into_iter()
            .filter_map(|(name, mut info)| {
                if seen.contains(&name) {
                    return None;
                }
                info.enabled = true;
                Some(info)
            })
            .collect();
        remaining.sort_by(|a, b| a.name.cmp(&b.name));
        active_mods.extend(remaining);

        Self {
            active_mods,
            base_game,
            mods_dir,
        }
    }

    pub fn active_mods(&self) -> &[ModInfo] {
        &self.active_mods
    }

    pub fn base_game(&self) -> &Path {
        &self.base_game
    }

    pub fn mods_dir(&self) -> &Path {
        &self.mods_dir
    }

    pub fn mods_list_path(&self) -> PathBuf {
        mods_list_path(&self.mods_dir)
    }

    pub fn asset_roots(&self) -> Vec<PathBuf> {
        self.active_mods
            .iter()
            .flat_map(|info| mod_asset_roots(&info.path))
            .collect()
    }
}

fn scan_mods(mods_dir: &Path) -> Vec<ModInfo> {
    let Ok(entries) = fs::read_dir(mods_dir) else {
        return Vec::new();
    };

    let mut mods = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !is_mod_directory(&path) {
            continue;
        }

        mods.push(mod_info_from_path(&path));
    }
    mods.sort_by(|a, b| a.name.cmp(&b.name));
    mods
}

fn mod_info_from_path(path: &Path) -> ModInfo {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "mod".to_string());
    ModInfo {
        name,
        path: path.to_path_buf(),
        enabled: true,
        pack_json: read_pack_json(path),
        icon_path: find_mod_icon(path),
    }
}

fn parse_mods_list(path: &Path) -> Vec<(String, bool)> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    contents
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let mut parts = trimmed.split('|');
            let name = parts.next()?.trim();
            if name.is_empty() {
                return None;
            }
            let enabled = parts
                .next()
                .map(|value| value.trim() == "1")
                .unwrap_or(true);
            Some((name.to_string(), enabled))
        })
        .collect()
}

fn mods_list_path(mods_dir: &Path) -> PathBuf {
    mods_dir
        .parent()
        .map(|parent| parent.join("modsList.txt"))
        .unwrap_or_else(|| PathBuf::from("modsList.txt"))
}

fn is_mod_directory(path: &Path) -> bool {
    path.join("pack.json").is_file()
        || path.join("assets").is_dir()
        || path.join("assets").join("pack.json").is_file()
}

fn read_pack_json(mod_path: &Path) -> Option<PackJson> {
    let candidates = [
        mod_path.join("pack.json"),
        mod_path.join("assets").join("pack.json"),
    ];
    for candidate in candidates {
        let Ok(contents) = fs::read_to_string(&candidate) else {
            continue;
        };
        if let Ok(pack) = serde_json::from_str::<PackJson>(&contents) {
            return Some(pack);
        }
    }
    None
}

fn find_mod_icon(mod_path: &Path) -> Option<PathBuf> {
    [
        mod_path.join("icon.png"),
        mod_path.join("icon.ico"),
        mod_path.join("pack.png"),
        mod_path.join("pack.ico"),
        mod_path.join("assets").join("icon.png"),
        mod_path.join("assets").join("icon.ico"),
        mod_path.join("assets").join("pack.png"),
        mod_path.join("assets").join("pack.ico"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn mod_asset_roots(mod_path: &Path) -> Vec<PathBuf> {
    let root = if mod_path.join("assets").is_dir() {
        mod_path.join("assets")
    } else {
        mod_path.to_path_buf()
    };

    let mut roots = vec![root.clone()];
    let shared = root.join("shared");
    if shared.is_dir() {
        roots.push(shared);
    }
    roots
}
