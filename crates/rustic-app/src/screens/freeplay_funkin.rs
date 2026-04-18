#[path = "freeplay_funkin_assets.rs"]
mod freeplay_funkin_assets;
#[path = "freeplay_funkin_draw.rs"]
mod freeplay_funkin_draw;
#[path = "freeplay_funkin_layout.rs"]
mod freeplay_funkin_layout;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rustic_audio::AudioEngine;
use rustic_core::paths::AssetPaths;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::SpriteAtlas;
use serde_json::Value;

use self::freeplay_funkin_assets::{
    find_funkin_asset, find_funkin_music, CapsuleAsset, FreeplayDj, SelectorAsset,
};
use super::{FreeplayScreen, GAME_W};

pub(super) const CUTOUT_W: f32 = GAME_W / 1.5;
pub(super) const DJ_X: f32 = 650.0;
pub(super) const DJ_Y: f32 = 355.0;

pub(super) struct FunkinFreeplayUi {
    background: Option<GpuTexture>,
    pink_back: Option<GpuTexture>,
    card_glow: Option<GpuTexture>,
    confirm_glow: Option<GpuTexture>,
    glowing_text: Option<GpuTexture>,
    highscore: Option<GpuTexture>,
    clear_box: Option<GpuTexture>,
    difficulty_dot: Option<GpuTexture>,
    selector: Option<SelectorAsset>,
    albums: HashMap<String, AlbumAsset>,
    song_albums: HashMap<String, String>,
    current_album: Option<String>,
    album_switch_timer: Option<f32>,
    capsule: Option<CapsuleAsset>,
    difficulty_textures: Vec<(String, GpuTexture)>,
    dj: Option<FreeplayDj>,
    capsule_frame: usize,
    capsule_timer: f32,
    intro_timer: f32,
    confirm_timer: Option<f32>,
}

impl FunkinFreeplayUi {
    pub(super) fn new() -> Self {
        Self {
            background: None,
            pink_back: None,
            card_glow: None,
            confirm_glow: None,
            glowing_text: None,
            highscore: None,
            clear_box: None,
            difficulty_dot: None,
            selector: None,
            albums: HashMap::new(),
            song_albums: HashMap::new(),
            current_album: None,
            album_switch_timer: None,
            capsule: None,
            difficulty_textures: Vec::new(),
            dj: None,
            capsule_frame: 0,
            capsule_timer: 0.0,
            intro_timer: 0.0,
            confirm_timer: None,
        }
    }

    pub(super) fn load(&mut self, gpu: &GpuState, paths: &AssetPaths) {
        if self.background.is_none() {
            if let Some(path) = find_funkin_asset(paths, "freeplay/freeplayBGweek1-bf.png") {
                self.background = Some(gpu.load_texture_from_path(&path));
            }
        }
        load_optional_texture(gpu, paths, &mut self.pink_back, "freeplay/pinkBack.png");
        load_optional_texture(gpu, paths, &mut self.card_glow, "freeplay/cardGlow.png");
        load_optional_texture(
            gpu,
            paths,
            &mut self.confirm_glow,
            "freeplay/confirmGlow.png",
        );
        load_optional_texture(
            gpu,
            paths,
            &mut self.glowing_text,
            "freeplay/glowingText.png",
        );
        load_optional_texture(gpu, paths, &mut self.highscore, "freeplay/highscore.png");
        load_optional_texture(gpu, paths, &mut self.clear_box, "freeplay/clearBox.png");
        load_optional_texture(
            gpu,
            paths,
            &mut self.difficulty_dot,
            "freeplay/seperator.png",
        );
        if self.selector.is_none() {
            self.selector = SelectorAsset::load(gpu, paths);
        }
        if self.albums.is_empty() {
            load_album_assets(gpu, paths, &mut self.albums);
            self.song_albums = load_song_album_map(paths);
        }

        if self.capsule.is_none() {
            self.capsule = CapsuleAsset::load(gpu, paths);
        }

        if self.difficulty_textures.is_empty() {
            for (name, rel) in [
                ("easy", "freeplay/freeplayeasy.png"),
                ("normal", "freeplay/freeplaynormal.png"),
                ("hard", "freeplay/freeplayhard.png"),
                ("nightmare", "freeplay/freeplaynightmare.png"),
            ] {
                if let Some(path) = find_funkin_asset(paths, rel) {
                    self.difficulty_textures
                        .push((name.to_string(), gpu.load_texture_from_path(&path)));
                }
            }
        }

        if self.dj.is_none() {
            self.dj = FreeplayDj::load(gpu, paths);
        }
    }

    pub(super) fn update(&mut self, dt: f32) {
        self.intro_timer += dt;
        if let Some(timer) = &mut self.confirm_timer {
            *timer += dt;
        }
        if let Some(timer) = &mut self.album_switch_timer {
            *timer += dt;
            if *timer >= 0.45 {
                self.album_switch_timer = None;
            }
        }

        self.capsule_timer += dt;
        while self.capsule_timer >= 1.0 / 24.0 {
            self.capsule_timer -= 1.0 / 24.0;
            self.capsule_frame = self.capsule_frame.wrapping_add(1);
        }

        if let Some(dj) = &mut self.dj {
            dj.update(dt);
        }
    }

    pub(super) fn play_confirm(&mut self) {
        self.confirm_timer = Some(0.0);
        if let Some(dj) = &mut self.dj {
            dj.play_confirm();
        }
    }

    pub(super) fn set_selected_song(&mut self, song_id: &str) {
        let album_id = self.album_id_for_song(song_id).to_string();
        if self.current_album.as_deref() == Some(album_id.as_str()) {
            return;
        }
        self.current_album = Some(album_id);
        self.album_switch_timer = Some(0.0);
    }

    fn difficulty_texture(&self, difficulty: &str) -> Option<&GpuTexture> {
        self.difficulty_textures
            .iter()
            .find(|(name, _)| name == difficulty)
            .map(|(_, texture)| texture)
    }

    fn intro_amount(&self) -> f32 {
        ease_out_quart((self.intro_timer / 0.7).clamp(0.0, 1.0))
    }

    fn hud_amount(&self) -> f32 {
        ease_out_quart(((self.intro_timer - 0.45) / 0.45).clamp(0.0, 1.0))
    }

    fn confirm_amount(&self) -> f32 {
        self.confirm_timer
            .map(|timer| ease_out_quart((timer / 0.45).clamp(0.0, 1.0)))
            .unwrap_or(0.0)
    }

    fn album_for_song(&self, song_id: &str) -> Option<&AlbumAsset> {
        let album_id = self.album_id_for_song(song_id);
        self.albums
            .get(album_id)
            .or_else(|| self.albums.get("volume1"))
    }

    fn album_id_for_song<'a>(&'a self, song_id: &'a str) -> &'a str {
        self.song_albums
            .get(song_id)
            .map(String::as_str)
            .unwrap_or("volume1")
    }
}

pub(super) struct AlbumAsset {
    art: GpuTexture,
    title: Option<AlbumTitleAsset>,
    title_offset: [f32; 2],
}

pub(super) struct AlbumTitleAsset {
    texture: GpuTexture,
    atlas: SpriteAtlas,
}

impl FreeplayScreen {
    pub(super) fn start_funkin_freeplay_music(&mut self, paths: &AssetPaths) {
        let Some(music) = find_funkin_music(paths, "freeplayRandom")
            .or_else(|| find_funkin_music(paths, "freakyMenu"))
            .or_else(|| paths.music("freakyMenu"))
        else {
            return;
        };

        let audio = self.audio.get_or_insert_with(AudioEngine::new);
        audio.stop_loop_music();
        audio.play_loop_music_vol(&music, 0.7);
        self.previewing_song = None;
    }

    pub(super) fn restore_main_menu_music(&mut self) {
        let paths = AssetPaths::platform_default();
        let Some(music) =
            find_funkin_music(&paths, "freakyMenu").or_else(|| paths.music("freakyMenu"))
        else {
            return;
        };
        let audio = self.audio.get_or_insert_with(AudioEngine::new);
        audio.stop_loop_music();
        audio.play_loop_music_vol(&music, 0.7);
        self.previewing_song = None;
    }
}

fn load_optional_texture(
    gpu: &GpuState,
    paths: &AssetPaths,
    slot: &mut Option<GpuTexture>,
    relative_after_images: &str,
) {
    if slot.is_some() {
        return;
    }
    if let Some(path) = find_funkin_asset(paths, relative_after_images) {
        *slot = Some(gpu.load_texture_from_path(&path));
    }
}

fn ease_out_quart(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(4)
}

fn load_album_assets(gpu: &GpuState, paths: &AssetPaths, albums: &mut HashMap<String, AlbumAsset>) {
    for data_path in album_data_paths(paths) {
        let Some(album_id) = data_path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if albums.contains_key(album_id) {
            continue;
        }
        let Ok(data) = std::fs::read_to_string(&data_path) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        let art_key = json
            .get("albumArtAsset")
            .and_then(Value::as_str)
            .unwrap_or("freeplay/albumRoll/volume1");
        let Some(art_path) = find_funkin_asset(paths, &format!("{art_key}.png")) else {
            continue;
        };
        let title = json
            .get("albumTitleAsset")
            .and_then(Value::as_str)
            .and_then(|key| load_album_title(gpu, paths, key));
        let title_offset = json
            .get("albumTitleOffsets")
            .and_then(Value::as_array)
            .map(|values| {
                [
                    values.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    values.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
                ]
            })
            .unwrap_or([0.0, 0.0]);
        albums.insert(
            album_id.to_string(),
            AlbumAsset {
                art: gpu.load_texture_from_path(&art_path),
                title,
                title_offset,
            },
        );
    }
}

fn load_album_title(gpu: &GpuState, paths: &AssetPaths, key: &str) -> Option<AlbumTitleAsset> {
    let png = find_funkin_asset(paths, &format!("{key}.png"))?;
    let xml = find_funkin_asset(paths, &format!("{key}.xml"))?;
    let xml_data = std::fs::read_to_string(xml).ok()?;
    let mut atlas = SpriteAtlas::from_xml(&xml_data);
    atlas.add_by_prefix("idle", "idle0");
    atlas.add_by_prefix("switch", "switch0");
    Some(AlbumTitleAsset {
        texture: gpu.load_texture_from_path(&png),
        atlas,
    })
}

fn load_song_album_map(paths: &AssetPaths) -> HashMap<String, String> {
    let mut albums = HashMap::new();
    for root in data_roots(paths) {
        let songs_dir = root.join("songs");
        let Ok(song_dirs) = std::fs::read_dir(songs_dir) else {
            continue;
        };
        for song_dir in song_dirs.flatten() {
            if !song_dir.path().is_dir() {
                continue;
            }
            let song_id = song_dir.file_name().to_string_lossy().to_string();
            let metadata = song_dir.path().join(format!("{song_id}-metadata.json"));
            let Ok(data) = std::fs::read_to_string(metadata) else {
                continue;
            };
            let Ok(json) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            let album = json
                .get("playData")
                .and_then(|play_data| play_data.get("album"))
                .and_then(Value::as_str);
            if let Some(album) = album {
                albums.entry(song_id).or_insert_with(|| album.to_string());
            }
        }
    }
    albums
}

fn album_data_paths(paths: &AssetPaths) -> Vec<PathBuf> {
    let mut results = Vec::new();
    for root in data_roots(paths) {
        let albums_dir = root.join("ui/freeplay/albums");
        let Ok(entries) = std::fs::read_dir(albums_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                results.push(path);
            }
        }
    }
    results
}

fn data_roots(paths: &AssetPaths) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    collect_data_root(&mut roots, paths.find_dir("data"));
    collect_data_root(
        &mut roots,
        Some(Path::new("assets/preload/data").to_path_buf()),
    );
    collect_data_root(&mut roots, Some(Path::new("assets/data").to_path_buf()));
    collect_data_root(
        &mut roots,
        Some(Path::new("references/funkin/assets/preload/data").to_path_buf()),
    );
    roots
}

fn collect_data_root(roots: &mut Vec<PathBuf>, root: Option<PathBuf>) {
    let Some(root) = root else {
        return;
    };
    if root.is_dir() && !roots.iter().any(|existing| existing == &root) {
        roots.push(root);
    }
}
