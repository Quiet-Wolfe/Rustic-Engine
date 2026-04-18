use std::path::{Path, PathBuf};

use rustanimate::FlxAnimate;
use rustic_core::paths::AssetPaths;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::SpriteAtlas;

pub(super) const CAPSULE_SELECTED: &str = "selected";
pub(super) const CAPSULE_UNSELECTED: &str = "unselected";

pub(super) struct CapsuleAsset {
    pub(super) texture: GpuTexture,
    pub(super) atlas: SpriteAtlas,
}

impl CapsuleAsset {
    pub(super) fn load(gpu: &GpuState, paths: &AssetPaths) -> Option<Self> {
        let png = find_funkin_asset(
            paths,
            "freeplay/freeplayCapsule/capsule/freeplayCapsule.png",
        )?;
        let xml = find_funkin_asset(
            paths,
            "freeplay/freeplayCapsule/capsule/freeplayCapsule.xml",
        )?;
        let xml_data = std::fs::read_to_string(xml).ok()?;
        let mut atlas = SpriteAtlas::from_xml(&xml_data);
        atlas.add_by_prefix(CAPSULE_SELECTED, "mp3 capsule w backing");
        atlas.add_by_prefix(CAPSULE_UNSELECTED, "mp3 capsule w backing NOT SELECTED");
        Some(Self {
            texture: gpu.load_texture_from_path(&png),
            atlas,
        })
    }
}

pub(super) struct FreeplayDj {
    texture: GpuTexture,
    animate: FlxAnimate,
    idle_index: usize,
    confirm_index: Option<usize>,
}

impl FreeplayDj {
    pub(super) fn load(gpu: &GpuState, paths: &AssetPaths) -> Option<Self> {
        let dir = find_funkin_dir(paths, "freeplay/freeplay-boyfriend")?;
        let animate = FlxAnimate::load(dir.to_str()?).ok()?;
        let texture = gpu.load_texture_from_path(&dir.join("spritemap1.png"));
        let idle_index = animation_index(&animate, &["boyfriend dj", "idle"]).unwrap_or(0);
        let confirm_index = animation_index(&animate, &["confirm"]);

        let mut dj = Self {
            texture,
            animate,
            idle_index,
            confirm_index,
        };
        dj.play_index(idle_index, true);
        Some(dj)
    }

    pub(super) fn update(&mut self, dt: f32) {
        self.animate.update(dt);
        if self.animate.finished() {
            self.play_index(self.idle_index, true);
        }
    }

    pub(super) fn play_confirm(&mut self) {
        if let Some(index) = self.confirm_index {
            self.play_index(index, false);
        }
    }

    fn play_index(&mut self, index: usize, looping: bool) {
        if let Some(anim) = self.animate.available_animations.get(index) {
            self.animate.active_anim_idx = index;
            self.animate.playing_symbol = anim.sn.clone();
            self.animate.current_frame = 0;
            self.animate.time_accumulator = 0.0;
            self.animate.finished = false;
            self.animate.set_looping(looping);
        }
    }

    pub(super) fn draw(&self, gpu: &mut GpuState, x: f32, y: f32, scale: f32, alpha: f32) {
        for call in self.animate.render(0.0, 0.0) {
            let positions = std::array::from_fn(|i| {
                [
                    x + call.vertices[i].position[0] * scale,
                    y + call.vertices[i].position[1] * scale,
                ]
            });
            let uvs = std::array::from_fn(|i| call.vertices[i].uv);
            let mut color = call.vertices[0].color;
            color[3] *= alpha;
            gpu.push_raw_quad(positions, uvs, color);
        }
        gpu.draw_batch(Some(&self.texture));
    }
}

pub(super) fn find_funkin_asset(
    paths: &AssetPaths,
    relative_after_images: &str,
) -> Option<PathBuf> {
    let engine_relative = format!("images/{relative_after_images}");
    paths
        .find(&engine_relative)
        .or_else(|| existing_path(Path::new("assets/preload/images").join(relative_after_images)))
        .or_else(|| existing_path(Path::new("assets/images").join(relative_after_images)))
        .or_else(|| {
            existing_path(
                Path::new("references/funkin/assets/preload/images").join(relative_after_images),
            )
        })
}

fn find_funkin_dir(paths: &AssetPaths, relative_after_images: &str) -> Option<PathBuf> {
    let engine_relative = format!("images/{relative_after_images}");
    paths
        .find_dir(&engine_relative)
        .or_else(|| existing_dir(Path::new("assets/preload/images").join(relative_after_images)))
        .or_else(|| existing_dir(Path::new("assets/images").join(relative_after_images)))
        .or_else(|| {
            existing_dir(
                Path::new("references/funkin/assets/preload/images").join(relative_after_images),
            )
        })
}

fn animation_index(animate: &FlxAnimate, needles: &[&str]) -> Option<usize> {
    animate
        .available_animations
        .iter()
        .enumerate()
        .find(|(_, anim)| {
            let name = anim.sn.to_lowercase();
            needles.iter().all(|needle| name.contains(needle))
        })
        .map(|(idx, _)| idx)
}

fn existing_path(path: PathBuf) -> Option<PathBuf> {
    path.exists().then_some(path)
}

fn existing_dir(path: PathBuf) -> Option<PathBuf> {
    path.is_dir().then_some(path)
}
