use std::path::PathBuf;

/// Mod-priority asset path resolver.
/// Checks: current_mod -> global_mods -> base_assets.
pub struct AssetPaths {
    pub base_path: PathBuf,
    pub current_mod: Option<PathBuf>,
    pub global_mods: Vec<PathBuf>,
}

impl AssetPaths {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            current_mod: None,
            global_mods: Vec::new(),
        }
    }

    /// Resolve an asset path with mod priority.
    pub fn resolve(&self, relative: &str) -> PathBuf {
        if let Some(ref mod_path) = self.current_mod {
            let p = mod_path.join(relative);
            if p.exists() {
                return p;
            }
        }

        for mod_path in &self.global_mods {
            let p = mod_path.join(relative);
            if p.exists() {
                return p;
            }
        }

        self.base_path.join(relative)
    }

    pub fn exists(&self, relative: &str) -> bool {
        self.resolve(relative).exists()
    }

    /// Image path: checks shared/images/ then images/.
    pub fn image(&self, name: &str) -> PathBuf {
        let p = self.resolve(&format!("shared/images/{name}.png"));
        if p.exists() {
            return p;
        }
        self.resolve(&format!("images/{name}.png"))
    }

    /// Atlas XML path: checks shared/images/ then images/.
    pub fn xml(&self, name: &str) -> PathBuf {
        let p = self.resolve(&format!("shared/images/{name}.xml"));
        if p.exists() {
            return p;
        }
        self.resolve(&format!("images/{name}.xml"))
    }

    pub fn song(&self, song_name: &str, file: &str) -> PathBuf {
        self.resolve(&format!("songs/{song_name}/{file}"))
    }

    pub fn chart(&self, song_name: &str, difficulty: &str) -> PathBuf {
        let filename = if difficulty == "normal" || difficulty.is_empty() {
            format!("{song_name}.json")
        } else {
            format!("{song_name}-{difficulty}.json")
        };
        self.resolve(&format!("data/{song_name}/{filename}"))
    }

    pub fn character(&self, name: &str) -> PathBuf {
        self.resolve(&format!("characters/{name}.json"))
    }

    pub fn stage(&self, name: &str) -> PathBuf {
        self.resolve(&format!("stages/{name}.json"))
    }

    pub fn character_image(&self, name: &str) -> PathBuf {
        self.resolve(&format!("shared/images/characters/{name}.png"))
    }

    pub fn character_xml(&self, name: &str) -> PathBuf {
        self.resolve(&format!("shared/images/characters/{name}.xml"))
    }

    pub fn health_icon(&self, name: &str) -> PathBuf {
        let p = self.resolve(&format!("shared/images/icons/icon-{name}.png"));
        if p.exists() {
            return p;
        }
        self.resolve(&format!("images/icons/icon-{name}.png"))
    }

    pub fn week(&self, name: &str) -> PathBuf {
        self.resolve(&format!("weeks/{name}.json"))
    }

    /// Scan a directory for file stems matching an extension.
    pub fn scan_directory(&self, relative_dir: &str, extension: &str) -> Vec<String> {
        let mut results = Vec::new();
        let dir = self.resolve(relative_dir);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        results.push(stem.to_string());
                    }
                }
            }
        }
        results.sort();
        results
    }

    /// Scan for available songs by looking in data/ directories.
    pub fn scan_songs(&self) -> Vec<String> {
        let mut songs = Vec::new();
        let data_dir = self.resolve("data");
        if let Ok(entries) = std::fs::read_dir(&data_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        let chart = path.join(format!("{name}.json"));
                        if chart.exists() {
                            songs.push(name.to_string());
                        }
                    }
                }
            }
        }
        songs.sort();
        songs
    }

    pub fn set_current_mod(&mut self, path: Option<PathBuf>) {
        self.current_mod = path;
    }

    pub fn add_global_mod(&mut self, path: PathBuf) {
        if !self.global_mods.contains(&path) {
            self.global_mods.push(path);
        }
    }
}
