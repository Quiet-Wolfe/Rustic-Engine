#[path = "freeplay_funkin_assets.rs"]
mod freeplay_funkin_assets;
#[path = "freeplay_funkin_draw.rs"]
mod freeplay_funkin_draw;

use rustic_audio::AudioEngine;
use rustic_core::paths::AssetPaths;
use rustic_render::gpu::{GpuState, GpuTexture};

use self::freeplay_funkin_assets::{
    find_funkin_asset, find_funkin_music, CapsuleAsset, FreeplayDj,
};
use super::{FreeplayScreen, GAME_W};

pub(super) const CUTOUT_W: f32 = GAME_W / 1.5;
pub(super) const DJ_X: f32 = CUTOUT_W * 0.44;

pub(super) struct FunkinFreeplayUi {
    background: Option<GpuTexture>,
    pink_back: Option<GpuTexture>,
    card_glow: Option<GpuTexture>,
    confirm_glow: Option<GpuTexture>,
    glowing_text: Option<GpuTexture>,
    highscore: Option<GpuTexture>,
    clear_box: Option<GpuTexture>,
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
