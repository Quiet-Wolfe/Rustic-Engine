use std::path::{Path, PathBuf};

/// Mod-priority asset path resolver.
///
/// Search roots are checked in order (first = highest priority).
/// Typical setup: mod assets → engine shared → base game shared → base game.
pub struct AssetPaths {
    /// Ordered search roots. First match wins.
    search_roots: Vec<PathBuf>,
}

impl AssetPaths {
    pub fn new() -> Self {
        Self {
            search_roots: Vec::new(),
        }
    }

    /// Cross-platform default: psych_default() on desktop, android_default() on Android.
    pub fn platform_default() -> Self {
        #[cfg(target_os = "android")]
        { Self::android_default() }
        #[cfg(not(target_os = "android"))]
        { Self::psych_default() }
    }

    /// Build the default path resolver for Android.
    /// Assets at /sdcard/RusticV2/assets/, mods at /sdcard/RusticV2/mods/.
    #[cfg(target_os = "android")]
    pub fn android_default() -> Self {
        let mut paths = Self::new();
        let base = PathBuf::from("/sdcard/RusticV2");

        // Mods: each subfolder under mods/ gets highest priority
        let mods_dir = base.join("mods");
        if mods_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&mods_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        // Check for assets/ subdirectory (Psych Engine mod structure)
                        let assets = p.join("assets");
                        if assets.is_dir() {
                            paths.add_root(assets.clone());
                            let shared = assets.join("shared");
                            if shared.exists() {
                                paths.add_root(shared);
                            }
                        } else {
                            // Flat mod (files directly in mod folder)
                            paths.add_root(p.clone());
                            let shared = p.join("shared");
                            if shared.exists() {
                                paths.add_root(shared);
                            }
                        }
                    }
                }
            }
        }

        // Engine shared
        paths.add_root(base.join("assets/shared"));
        // Base game shared
        paths.add_root(base.join("assets/base_game/shared"));
        // Base game root
        paths.add_root(base.join("assets/base_game"));
        // Engine fallback
        paths.add_root(base.join("assets"));

        paths
    }

    /// Build the default path resolver for Psych Engine + mods.
    /// Roots are added highest priority first.
    pub fn psych_default() -> Self {
        let mut paths = Self::new();

        // Mod: VS Retrospecter Part 2
        let retro = PathBuf::from("references/Vs-Retrospecter-Part-2-COMPILED/assets");
        if retro.exists() {
            paths.add_root(retro.clone());
            // Mod also has a shared/ subdirectory with images/sounds/music
            let retro_shared = retro.join("shared");
            if retro_shared.exists() {
                paths.add_root(retro_shared);
            }
        }

        // Engine shared (note skins, common SFX, menu music, bf character)
        paths.add_root(PathBuf::from("references/FNF-PsychEngine/assets/shared"));

        // Base game shared (characters, stages, data, images)
        paths.add_root(PathBuf::from("references/FNF-PsychEngine/assets/base_game/shared"));

        // Base game root (songs, week-specific dirs like week1/)
        paths.add_root(PathBuf::from("references/FNF-PsychEngine/assets/base_game"));

        // Engine root (fallback)
        paths.add_root(PathBuf::from("references/FNF-PsychEngine/assets"));

        paths
    }

    /// Add a search root (lower priority than existing roots).
    pub fn add_root(&mut self, path: PathBuf) {
        self.search_roots.push(path);
    }

    /// Add a search root at highest priority (before all existing roots).
    pub fn add_root_front(&mut self, path: PathBuf) {
        self.search_roots.insert(0, path);
    }

    // === Core resolution ===

    /// Find the first existing file matching `relative` across all search roots.
    pub fn find(&self, relative: &str) -> Option<PathBuf> {
        for root in &self.search_roots {
            let p = root.join(relative);
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Find first match across multiple relative path patterns.
    /// Checks ALL roots for the first pattern, then ALL roots for the second, etc.
    fn find_any(&self, patterns: &[String]) -> Option<PathBuf> {
        for pattern in patterns {
            if let Some(p) = self.find(pattern) {
                return Some(p);
            }
        }
        None
    }

    /// Find ALL matches for a relative path across all search roots.
    /// Used for collecting scripts from multiple sources.
    fn find_all(&self, relative: &str) -> Vec<PathBuf> {
        let mut results = Vec::new();
        for root in &self.search_roots {
            let p = root.join(relative);
            if p.exists() {
                results.push(p);
            }
        }
        results
    }

    // === Character assets ===

    /// Find a character JSON file.
    pub fn character_json(&self, name: &str) -> Option<PathBuf> {
        self.find(&format!("characters/{name}.json"))
    }

    /// Find character sprite atlas (png + xml). Returns the directory containing them.
    pub fn character_atlas_dir(&self, image_field: &str) -> Option<PathBuf> {
        // Check images/{image}.png in each root
        let png = format!("images/{image_field}.png");
        for root in &self.search_roots {
            if root.join(&png).exists() {
                return Some(root.join("images"));
            }
        }
        None
    }

    /// Find an Adobe Animate atlas directory for a character.
    /// The image field points to a folder (e.g. "characters/atlases/nightgflaid")
    /// containing Animation.json + spritemap1.json + spritemap1.png.
    pub fn character_animate_dir(&self, image_field: &str) -> Option<PathBuf> {
        let dir = format!("images/{image_field}");
        for root in &self.search_roots {
            let p = root.join(&dir);
            if p.join("Animation.json").exists() {
                return Some(p);
            }
        }
        None
    }

    /// Find a health bar icon.
    /// Checks both standard `icon-{name}.png` and mod-style `{name}.png` paths.
    pub fn health_icon(&self, name: &str) -> Option<PathBuf> {
        self.find_any(&[
            format!("images/icons/icon-{name}.png"),
            format!("images/icons/{name}.png"),
        ])
    }

    // === Stage assets ===

    /// Find a stage JSON file.
    pub fn stage_json(&self, name: &str) -> Option<PathBuf> {
        self.find(&format!("stages/{name}.json"))
    }

    /// Find a stage Lua script.
    pub fn stage_lua(&self, name: &str) -> Option<PathBuf> {
        self.find(&format!("stages/{name}.lua"))
    }

    /// Find a stage image, checking the stage's directory first.
    /// Psych Engine: stage directory → shared/images → images.
    pub fn stage_image(&self, image: &str, stage_dir: &str) -> Option<PathBuf> {
        let mut patterns = Vec::new();
        if !stage_dir.is_empty() {
            patterns.push(format!("{stage_dir}/images/{image}.png"));
        }
        patterns.push(format!("images/{image}.png"));
        self.find_any(&patterns)
    }

    // === Song/chart assets ===

    /// Find a chart JSON file.
    pub fn chart(&self, song_name: &str, difficulty: &str) -> Option<PathBuf> {
        let filename = if difficulty == "normal" || difficulty.is_empty() {
            format!("{song_name}.json")
        } else {
            format!("{song_name}-{difficulty}.json")
        };
        self.find(&format!("data/{song_name}/{filename}"))
    }

    /// Find a song audio file (e.g., "Inst.ogg", "Voices-Player.ogg").
    pub fn song_audio(&self, song_name: &str, file: &str) -> Option<PathBuf> {
        self.find(&format!("songs/{song_name}/{file}"))
    }

    /// Discover all Lua scripts for a song.
    /// Psych Engine loads: data/{song}/*.lua (script.lua, modchart.lua, eventScript.lua, etc.)
    /// Rustic extension: if data/{song}/rustic/{file}.lua exists, it replaces the original.
    /// Additionally, any .lua files that ONLY exist in rustic/ are also loaded (rustic-only scripts).
    pub fn song_scripts(&self, song_name: &str) -> Vec<PathBuf> {
        let relative_dir = format!("data/{song_name}");
        let mut scripts = Vec::new();
        let mut seen_names = std::collections::HashSet::new();
        // Check each root for a data/{song}/ directory
        for root in &self.search_roots {
            let dir = root.join(&relative_dir);
            if dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                            let name = entry.file_name().to_string_lossy().to_string();
                            seen_names.insert(name);
                            // Prefer rustic/ override: if data/{song}/rustic/{file}.lua exists, use it
                            let rustic_path = dir.join("rustic").join(entry.file_name());
                            if rustic_path.exists() {
                                eprintln!("[rustic] Using override: {:?}", rustic_path);
                                scripts.push(rustic_path);
                            } else {
                                scripts.push(path);
                            }
                        }
                    }
                }
                // Also discover rustic-only scripts (no corresponding file in parent dir)
                let rustic_dir = dir.join("rustic");
                if rustic_dir.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&rustic_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                                let name = entry.file_name().to_string_lossy().to_string();
                                if !seen_names.contains(&name) {
                                    eprintln!("[rustic] Loading rustic-only script: {:?}", path);
                                    scripts.push(path);
                                    seen_names.insert(name);
                                }
                            }
                        }
                    }
                }
            }
        }
        scripts.sort();
        scripts
    }

    /// Discover custom event Lua scripts.
    pub fn custom_event_scripts(&self) -> Vec<PathBuf> {
        let mut scripts = Vec::new();
        for root in &self.search_roots {
            let dir = root.join("custom_events");
            if dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                            scripts.push(path);
                        }
                    }
                }
            }
        }
        scripts.sort();
        scripts
    }

    // === Image / sound assets ===

    /// Find an image by name (checks images/ in each root).
    pub fn image(&self, name: &str) -> Option<PathBuf> {
        self.find(&format!("images/{name}.png"))
    }

    /// Find an image atlas XML.
    pub fn image_xml(&self, name: &str) -> Option<PathBuf> {
        self.find(&format!("images/{name}.xml"))
    }

    /// Find a video file (tries common extensions).
    pub fn video(&self, name: &str) -> Option<PathBuf> {
        self.find_any(&[
            format!("videos/{name}.mp4"),
            format!("videos/{name}.webm"),
            format!("videos/{name}.ogv"),
        ])
    }

    /// Find a sound effect.
    pub fn sound(&self, name: &str) -> Option<PathBuf> {
        self.find_any(&[
            format!("sounds/{name}.ogg"),
        ])
    }

    /// Find a music file.
    pub fn music(&self, name: &str) -> Option<PathBuf> {
        self.find_any(&[
            format!("music/{name}.ogg"),
        ])
    }

    // === Week / freeplay assets ===

    /// Find a week JSON.
    pub fn week_json(&self, name: &str) -> Option<PathBuf> {
        self.find(&format!("weeks/{name}.json"))
    }

    /// Find the first existing weeks directory (for scanning available weeks).
    pub fn weeks_dir(&self) -> Option<PathBuf> {
        for root in &self.search_roots {
            let dir = root.join("weeks");
            if dir.is_dir() {
                return Some(dir);
            }
        }
        None
    }

    /// Collect all weeks directories across all roots (for merging week lists).
    pub fn all_weeks_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        for root in &self.search_roots {
            let dir = root.join("weeks");
            if dir.is_dir() {
                dirs.push(dir);
            }
        }
        dirs
    }

    // === Song discovery ===

    /// Discover all song names by scanning `data/` directories across all roots.
    /// A directory is considered a song if it contains a `.json` chart file
    /// matching the directory name (e.g. `data/extirpatient/extirpatient.json`).
    /// Returns unique song names (highest-priority root wins on duplicates).
    pub fn discover_songs(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut songs = Vec::new();
        for root in &self.search_roots {
            let data_dir = root.join("data");
            if !data_dir.is_dir() { continue; }
            if let Ok(entries) = std::fs::read_dir(&data_dir) {
                for entry in entries.flatten() {
                    if !entry.path().is_dir() { continue; }
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy().to_string();
                    if seen.contains(&name_str) { continue; }
                    // Check for a chart JSON matching the folder name
                    let chart = entry.path().join(format!("{}.json", name_str));
                    if chart.exists() {
                        seen.insert(name_str.clone());
                        songs.push(name_str);
                    }
                }
            }
        }
        songs.sort();
        songs
    }

    // === Scanning ===

    /// Scan for all files with a given extension in a relative directory across all roots.
    /// Returns unique file stems.
    pub fn scan_stems(&self, relative_dir: &str, extension: &str) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::new();
        for root in &self.search_roots {
            let dir = root.join(relative_dir);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if seen.insert(stem.to_string()) {
                                results.push(stem.to_string());
                            }
                        }
                    }
                }
            }
        }
        results.sort();
        results
    }

    /// Find the first directory matching a relative path across all roots.
    pub fn find_dir(&self, relative: &str) -> Option<PathBuf> {
        for root in &self.search_roots {
            let p = root.join(relative);
            if p.is_dir() {
                return Some(p);
            }
        }
        None
    }

    /// Get all search roots (for debugging).
    pub fn roots(&self) -> &[PathBuf] {
        &self.search_roots
    }
}

impl Default for AssetPaths {
    fn default() -> Self {
        Self::new()
    }
}
