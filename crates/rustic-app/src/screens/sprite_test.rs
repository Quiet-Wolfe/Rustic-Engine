use winit::keyboard::KeyCode;

use rustic_core::character::CharacterFile;
use rustic_core::paths::AssetPaths;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::{AnimationController, SpriteAtlas};

use crate::screen::Screen;

const GAME_W: f32 = 1280.0;
const GAME_H: f32 = 720.0;

pub struct SpriteTestScreen {
    bf_char: Option<CharacterFile>,
    atlas: Option<SpriteAtlas>,
    gpu_texture: Option<GpuTexture>,
    anim_names: Vec<String>,
    anim_ctrl: AnimationController,
    current_anim_idx: usize,
    fps: f32,
    fps_accum: f32,
    fps_frames: u32,
}

impl SpriteTestScreen {
    pub fn new() -> Self {
        Self {
            bf_char: None,
            atlas: None,
            gpu_texture: None,
            anim_names: Vec::new(),
            anim_ctrl: AnimationController::new(),
            current_anim_idx: 0,
            fps: 0.0,
            fps_accum: 0.0,
            fps_frames: 0,
        }
    }

    fn play_current_anim(&mut self) {
        let Some(bf_char) = &self.bf_char else { return };
        let anim = &bf_char.animations[self.current_anim_idx];
        self.anim_ctrl
            .play(&anim.anim, anim.fps as f32, anim.loop_anim);
    }
}

impl Screen for SpriteTestScreen {
    fn init(&mut self, gpu: &GpuState) {
        let assets = AssetPaths::platform_default();

        let char_path = assets.character_json("bf").expect("bf.json not found");
        let char_json = std::fs::read_to_string(&char_path)
            .unwrap_or_else(|e| panic!("Failed to read character file {:?}: {}", char_path, e));
        let bf_char = CharacterFile::from_json(&char_json).expect("Failed to parse bf.json");

        let img_path = assets.image(&bf_char.image).expect("BF image not found");
        let xml_path = assets
            .image_xml(&bf_char.image)
            .expect("BF atlas XML not found");

        let gpu_texture = gpu.load_texture_from_path(&img_path);

        let xml_data = std::fs::read_to_string(&xml_path)
            .unwrap_or_else(|e| panic!("Failed to read atlas XML {:?}: {}", xml_path, e));
        let mut atlas = SpriteAtlas::from_xml(&xml_data);

        for anim_def in &bf_char.animations {
            if anim_def.indices.is_empty() {
                atlas.add_by_prefix(&anim_def.anim, &anim_def.name);
            } else {
                atlas.add_by_indices(&anim_def.anim, &anim_def.name, &anim_def.indices);
            }
            self.anim_names.push(anim_def.anim.clone());
            log::info!(
                "Registered anim '{}' (prefix '{}') -> {} frames",
                anim_def.anim,
                anim_def.name,
                atlas.frame_count(&anim_def.anim),
            );
        }

        self.bf_char = Some(bf_char);
        self.atlas = Some(atlas);
        self.gpu_texture = Some(gpu_texture);

        self.play_current_anim();
    }

    fn handle_key(&mut self, key: KeyCode) {
        let Some(bf_char) = &self.bf_char else { return };
        let num_anims = self.anim_names.len();

        match key {
            KeyCode::ArrowRight => {
                self.current_anim_idx = (self.current_anim_idx + 1) % num_anims;
                let anim = &bf_char.animations[self.current_anim_idx];
                self.anim_ctrl
                    .play(&anim.anim, anim.fps as f32, anim.loop_anim);
            }
            KeyCode::ArrowLeft => {
                self.current_anim_idx = if self.current_anim_idx == 0 {
                    num_anims - 1
                } else {
                    self.current_anim_idx - 1
                };
                let anim = &bf_char.animations[self.current_anim_idx];
                self.anim_ctrl
                    .play(&anim.anim, anim.fps as f32, anim.loop_anim);
            }
            KeyCode::Space => {
                self.anim_ctrl.play("", 0.0, false);
                let anim = &bf_char.animations[self.current_anim_idx];
                self.anim_ctrl
                    .play(&anim.anim, anim.fps as f32, anim.loop_anim);
            }
            _ => {}
        }
    }

    fn update(&mut self, dt: f32) {
        // FPS counter — update once per half second
        self.fps_accum += dt;
        self.fps_frames += 1;
        if self.fps_accum >= 0.5 {
            self.fps = self.fps_frames as f32 / self.fps_accum;
            self.fps_accum = 0.0;
            self.fps_frames = 0;
        }

        let Some(atlas) = &self.atlas else { return };
        let anim_name = &self.anim_names[self.current_anim_idx];
        let frame_count = atlas.frame_count(anim_name);
        self.anim_ctrl.update(dt, frame_count);
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        let Some(bf_char) = &self.bf_char else { return };
        let Some(atlas) = &self.atlas else { return };
        let Some(gpu_texture) = &self.gpu_texture else {
            return;
        };

        let anim_name = &self.anim_names[self.current_anim_idx];
        let char_x = GAME_W / 2.0 - 100.0;
        let char_y = GAME_H / 2.0 - 200.0;

        let anim_offsets = bf_char.animations[self.current_anim_idx].offsets;
        let scale = bf_char.scale as f32;

        if let Some(frame) = atlas.get_frame(anim_name, self.anim_ctrl.frame_index) {
            gpu.draw_sprite_frame(
                frame,
                gpu_texture.width as f32,
                gpu_texture.height as f32,
                char_x - anim_offsets[0] as f32 * scale,
                char_y - anim_offsets[1] as f32 * scale,
                scale,
                bf_char.flip_x,
                [1.0, 1.0, 1.0, 1.0],
            );
        }

        let frame_count = atlas.frame_count(anim_name);
        let white = [1.0, 1.0, 1.0, 1.0];
        let info = format!(
            "FPS: {:.0}\nAnimation: {} ({}/{})\nFrame: {}/{}\nLeft/Right: cycle | Space: replay | Esc: quit",
            self.fps,
            anim_name,
            self.current_anim_idx + 1,
            self.anim_names.len(),
            self.anim_ctrl.frame_index + 1,
            frame_count,
        );
        gpu.draw_text(&info, 10.0, 10.0, 18.0, white);

        gpu.present(gpu_texture);
    }
}
