#[path = "freeplay_funkin_assets.rs"]
mod freeplay_funkin_assets;

use rustic_core::paths::AssetPaths;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::health_icon::IconState;

use self::freeplay_funkin_assets::{
    find_funkin_asset, CapsuleAsset, FreeplayDj, CAPSULE_SELECTED, CAPSULE_UNSELECTED,
};
use super::{FreeplayScreen, DIFFICULTIES, GAME_H, GAME_W};

pub(super) struct FunkinFreeplayUi {
    background: Option<GpuTexture>,
    capsule: Option<CapsuleAsset>,
    difficulty_textures: Vec<(String, GpuTexture)>,
    dj: Option<FreeplayDj>,
    capsule_frame: usize,
    capsule_timer: f32,
}

impl FunkinFreeplayUi {
    pub(super) fn new() -> Self {
        Self {
            background: None,
            capsule: None,
            difficulty_textures: Vec::new(),
            dj: None,
            capsule_frame: 0,
            capsule_timer: 0.0,
        }
    }

    pub(super) fn load(&mut self, gpu: &GpuState, paths: &AssetPaths) {
        if self.background.is_none() {
            if let Some(path) = find_funkin_asset(paths, "freeplay/freeplayBGweek1-bf.png") {
                self.background = Some(gpu.load_texture_from_path(&path));
            }
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
}

impl FreeplayScreen {
    pub(super) fn draw_funkin(&mut self, gpu: &mut GpuState) {
        if !gpu.begin_frame() {
            return;
        }

        self.draw_funkin_background(gpu);
        self.draw_funkin_dj(gpu);
        self.draw_funkin_capsules(gpu);
        self.draw_funkin_hud(gpu);
        self.draw_funkin_overlays(gpu);

        crate::debug_overlay::finish_frame(gpu);
    }

    fn draw_funkin_background(&self, gpu: &mut GpuState) {
        let c = self.bg_color;
        gpu.push_colored_quad(
            0.0,
            0.0,
            GAME_W,
            GAME_H,
            [c[0] * 0.35, c[1] * 0.35, c[2] * 0.35, 1.0],
        );
        gpu.push_colored_quad(0.0, 0.0, 560.0, GAME_H, [0.98, 0.23, 0.18, 1.0]);
        gpu.push_raw_quad(
            [[455.0, 0.0], [705.0, 0.0], [565.0, GAME_H], [315.0, GAME_H]],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            [0.92, 0.18, 0.14, 1.0],
        );
        gpu.push_colored_quad(
            610.0,
            0.0,
            GAME_W - 610.0,
            GAME_H,
            [0.04, 0.035, 0.045, 0.9],
        );
        gpu.draw_batch(None);

        if let Some(bg) = &self.funkin_ui.background {
            let scale = (GAME_H / bg.height as f32).max((GAME_W * 0.62) / bg.width as f32);
            let w = bg.width as f32 * scale;
            let h = bg.height as f32 * scale;
            let x = GAME_W - w + 20.0;
            let y = (GAME_H - h) * 0.5;
            gpu.push_texture_region(
                bg.width as f32,
                bg.height as f32,
                0.0,
                0.0,
                bg.width as f32,
                bg.height as f32,
                x,
                y,
                w,
                h,
                false,
                [1.0, 1.0, 1.0, 0.62],
            );
            gpu.draw_batch(Some(bg));
        } else if let Some(bg) = &self.bg_tex {
            let bw = bg.width as f32;
            let bh = bg.height as f32;
            gpu.push_texture_region(
                bw,
                bh,
                0.0,
                0.0,
                bw,
                bh,
                (GAME_W - bw) / 2.0,
                (GAME_H - bh) / 2.0,
                bw,
                bh,
                false,
                [c[0], c[1], c[2], 0.45],
            );
            gpu.draw_batch(Some(bg));
        }
    }

    fn draw_funkin_dj(&self, gpu: &mut GpuState) {
        if let Some(dj) = &self.funkin_ui.dj {
            dj.draw(gpu, 42.0, 92.0, 0.72, 1.0);
        } else {
            gpu.draw_text("BF", 140.0, 235.0, 120.0, [1.0, 1.0, 1.0, 0.8]);
            gpu.draw_text("DJ", 170.0, 340.0, 120.0, [1.0, 1.0, 1.0, 0.8]);
        }
    }

    fn draw_funkin_capsules(&mut self, gpu: &mut GpuState) {
        let draw_dist = 5;
        let base_x = 665.0;
        let base_y = 315.0;
        let spacing = 92.0;

        if let Some(capsule) = &self.funkin_ui.capsule {
            let tex_w = capsule.texture.width as f32;
            let tex_h = capsule.texture.height as f32;
            for (i, _) in self.filtered.iter().enumerate() {
                let target_y = i as f32 - self.lerp_selected;
                if target_y.abs() > draw_dist as f32 {
                    continue;
                }
                let selected = i == self.cur_selected;
                let anim = if selected {
                    CAPSULE_SELECTED
                } else {
                    CAPSULE_UNSELECTED
                };
                let frame_count = capsule.atlas.frame_count(anim).max(1);
                let frame_idx = if selected {
                    self.funkin_ui.capsule_frame % frame_count
                } else {
                    (self.funkin_ui.capsule_frame / 2) % frame_count
                };
                if let Some(frame) = capsule.atlas.get_frame(anim, frame_idx) {
                    let x = base_x + target_y * 18.0;
                    let y = base_y + target_y * spacing;
                    let scale = if selected { 0.88 } else { 0.78 };
                    let alpha = if selected { 1.0 } else { 0.58 };
                    gpu.draw_sprite_frame(
                        frame,
                        tex_w,
                        tex_h,
                        x,
                        y,
                        scale,
                        false,
                        [1.0, 1.0, 1.0, alpha],
                    );
                }
            }
            gpu.draw_batch(Some(&capsule.texture));
        } else {
            for (i, _) in self.filtered.iter().enumerate() {
                let target_y = i as f32 - self.lerp_selected;
                if target_y.abs() > draw_dist as f32 {
                    continue;
                }
                let selected = i == self.cur_selected;
                let y = base_y + target_y * spacing + 8.0;
                let alpha = if selected { 0.95 } else { 0.45 };
                gpu.push_colored_quad(
                    base_x,
                    y,
                    if selected { 535.0 } else { 500.0 },
                    68.0,
                    [1.0, 1.0, 1.0, alpha],
                );
            }
            gpu.draw_batch(None);
        }

        for (i, &song_idx) in self.filtered.iter().enumerate() {
            let target_y = i as f32 - self.lerp_selected;
            if target_y.abs() > draw_dist as f32 {
                continue;
            }

            let selected = i == self.cur_selected;
            let y = base_y + target_y * spacing;
            let alpha = if selected { 1.0 } else { 0.58 };
            let song = &mut self.songs[song_idx];
            let label_size = if selected { 29.0 } else { 25.0 };
            let label_x = base_x + 142.0;
            let label_y = y + if selected { 36.0 } else { 39.0 };
            let label = truncate_for_capsule(&song.name);
            gpu.draw_text(
                &label,
                label_x,
                label_y,
                label_size,
                [0.06, 0.05, 0.06, alpha],
            );

            if let Some(icon) = &mut song.icon {
                icon.set_state(if selected {
                    IconState::Winning
                } else {
                    IconState::Neutral
                });
                icon.draw(
                    gpu,
                    base_x + 37.0,
                    y + 8.0,
                    if selected { 76.0 } else { 68.0 },
                    [1.0, 1.0, 1.0, alpha],
                );
            }
        }
    }

    fn draw_funkin_hud(&self, gpu: &mut GpuState) {
        gpu.push_colored_quad(0.0, 0.0, GAME_W, 74.0, [0.0, 0.0, 0.0, 0.45]);
        gpu.push_colored_quad(610.0, 86.0, 620.0, 4.0, [1.0, 1.0, 1.0, 0.55]);
        gpu.push_colored_quad(610.0, GAME_H - 72.0, 620.0, 4.0, [1.0, 1.0, 1.0, 0.35]);
        gpu.draw_batch(None);

        let diff = DIFFICULTIES[self.cur_difficulty];
        if let Some(tex) = self.funkin_ui.difficulty_texture(diff) {
            let scale = 0.74;
            let w = tex.width as f32 * scale;
            let h = tex.height as f32 * scale;
            gpu.push_texture_region(
                tex.width as f32,
                tex.height as f32,
                0.0,
                0.0,
                tex.width as f32,
                tex.height as f32,
                126.0,
                34.0,
                w,
                h,
                false,
                [1.0, 1.0, 1.0, 1.0],
            );
            gpu.draw_batch(Some(tex));
        } else {
            gpu.draw_text(
                &format!("< {} >", diff.to_uppercase()),
                116.0,
                42.0,
                30.0,
                [1.0, 1.0, 1.0, 1.0],
            );
        }

        gpu.draw_text("FREEPLAY", 610.0, 22.0, 36.0, [1.0, 1.0, 1.0, 1.0]);
        gpu.draw_text(
            &self.current_score_text(),
            820.0,
            24.0,
            25.0,
            [1.0, 1.0, 1.0, 1.0],
        );

        let status = if self.previewing_song.is_some() {
            "SPACE Stop Preview"
        } else {
            "SPACE Preview"
        };
        let count_text = if self.search.is_empty() {
            format!(
                "{} songs  |  {}  |  ENTER Play  |  CTRL Changers  |  TAB Opponent: {}",
                self.filtered.len(),
                status,
                if self.play_as_opponent { "ON" } else { "OFF" }
            )
        } else {
            format!(
                "{}/{} songs  |  ESC Clear Search  |  ENTER Play  |  TAB Opponent: {}",
                self.filtered.len(),
                self.songs.len(),
                if self.play_as_opponent { "ON" } else { "OFF" }
            )
        };
        gpu.draw_text(
            &count_text,
            28.0,
            GAME_H - 44.0,
            18.0,
            [1.0, 1.0, 1.0, 0.92],
        );

        if !self.search.is_empty() {
            gpu.push_colored_quad(610.0, 96.0, 475.0, 42.0, [0.0, 0.0, 0.0, 0.62]);
            gpu.draw_batch(None);
            gpu.draw_text(
                &format!("Search: {}_", self.search),
                625.0,
                106.0,
                22.0,
                [1.0, 0.92, 0.34, 1.0],
            );
        }

        draw_difficulty_dots(gpu, self.cur_difficulty, DIFFICULTIES.len());
    }

    fn draw_funkin_overlays(&mut self, gpu: &mut GpuState) {
        if cfg!(target_os = "android") {
            let strip_top = 86.0;
            let strip_h = GAME_H - 170.0;
            let letter_h = strip_h / 26.0;
            gpu.push_colored_quad(8.0, strip_top, 30.0, strip_h, [0.0, 0.0, 0.0, 0.35]);
            gpu.draw_batch(None);
            for i in 0..26u8 {
                let letter = (b'A' + i) as char;
                let y = strip_top + i as f32 * letter_h;
                gpu.draw_text(
                    &letter.to_string(),
                    15.0,
                    y,
                    letter_h.min(18.0),
                    [1.0, 1.0, 1.0, 0.7],
                );
            }
        }

        if let Some(gameplay_changers) = &self.gameplay_changers {
            gameplay_changers.draw(gpu);
        }
        if let Some(reset_modal) = &mut self.reset_modal {
            reset_modal.draw(gpu);
        }
    }
}

fn draw_difficulty_dots(gpu: &mut GpuState, selected: usize, count: usize) {
    let start_x = 154.0;
    let y = 108.0;
    for i in 0..count {
        let active = i == selected;
        let size = if active { 16.0 } else { 10.0 };
        let offset = if active { 0.0 } else { 3.0 };
        gpu.push_colored_quad(
            start_x + i as f32 * 26.0,
            y + offset,
            size,
            size,
            if active {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.55]
            },
        );
    }
    gpu.draw_batch(None);
}

fn truncate_for_capsule(name: &str) -> String {
    const MAX: usize = 23;
    if name.chars().count() <= MAX {
        return name.to_string();
    }
    let mut out: String = name.chars().take(MAX.saturating_sub(1)).collect();
    out.push_str("...");
    out
}
