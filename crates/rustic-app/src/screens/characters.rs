use std::collections::HashMap;
use std::path::Path;

use rustic_core::character::{self, CharacterFile};
use rustic_render::camera::GameCamera;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::{AnimationController, SpriteAtlas};

use super::play::{GAME_W, GAME_H};

/// A loaded character with sprite atlas and animation state.
pub struct CharacterSprite {
    pub texture: GpuTexture,
    atlas: SpriteAtlas,
    tex_w: f32,
    tex_h: f32,
    pub anim: AnimationController,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    flip_x: bool,
    offsets: HashMap<String, [f64; 2]>,
    loop_flags: HashMap<String, bool>,
    pub hold_timer: f64,
    pub sing_duration: f64,
    pub has_dance_idle: bool,
    dance_left: bool,
    pub healthbar_colors: [u8; 3],
    pub healthicon: String,
    /// Character-specific camera offset from character JSON.
    pub camera_position: [f64; 2],
}

impl CharacterSprite {
    pub fn load(
        gpu: &GpuState,
        char_json_path: &Path,
        atlas_dir: &Path,
        stage_x: f64,
        stage_y: f64,
        is_player: bool,
    ) -> Self {
        let json_str = std::fs::read_to_string(char_json_path)
            .unwrap_or_else(|e| panic!("Failed to read char JSON {:?}: {}", char_json_path, e));
        let char_def = CharacterFile::from_json(&json_str)
            .unwrap_or_else(|e| panic!("Failed to parse char JSON {:?}: {}", char_json_path, e));

        let effective_image = char_def.effective_image();
        let png_path = atlas_dir.join(format!("{}.png", effective_image));
        let xml_path = atlas_dir.join(format!("{}.xml", effective_image));
        let xml_str = std::fs::read_to_string(&xml_path)
            .unwrap_or_else(|e| panic!("Failed to read char atlas XML {:?}: {}", xml_path, e));

        let texture = gpu.load_texture_from_path(&png_path);
        let mut atlas = SpriteAtlas::from_xml(&xml_str);

        let mut offsets = HashMap::new();
        let mut loop_flags = HashMap::new();
        for anim_def in &char_def.animations {
            if anim_def.indices.is_empty() {
                atlas.add_by_prefix(&anim_def.anim, &anim_def.name);
            } else {
                let indices: Vec<i32> = anim_def.indices.clone();
                atlas.add_by_indices(&anim_def.anim, &anim_def.name, &indices);
            }
            offsets.insert(anim_def.anim.clone(), anim_def.offsets);
            loop_flags.insert(anim_def.anim.clone(), anim_def.loop_anim);
        }

        // Psych Engine: flipX = (json.flip_x != isPlayer)
        let flip_x = char_def.flip_x != is_player;

        let x = (stage_x + char_def.position[0]) as f32;
        let y = (stage_y + char_def.position[1]) as f32;

        let has_dance_idle = char_def.has_dance_idle();

        let mut sprite = CharacterSprite {
            tex_w: texture.width as f32,
            tex_h: texture.height as f32,
            texture,
            atlas,
            anim: AnimationController::new(),
            x,
            y,
            scale: char_def.scale as f32,
            flip_x,
            offsets,
            loop_flags,
            hold_timer: 0.0,
            sing_duration: char_def.sing_duration,
            has_dance_idle,
            dance_left: false,
            healthbar_colors: char_def.healthbar_colors,
            healthicon: char_def.healthicon.clone(),
            camera_position: char_def.camera_position,
        };

        if has_dance_idle {
            sprite.play_anim("danceLeft", false);
        } else {
            sprite.play_anim("idle", false);
        }

        sprite
    }

    pub fn play_anim(&mut self, name: &str, force: bool) {
        let count = self.atlas.frame_count(name);
        if count == 0 {
            return;
        }
        // Use the loop flag from character JSON, falling back to name heuristics
        let looping = self.loop_flags.get(name).copied()
            .unwrap_or_else(|| name.contains("-loop") || name == "idle");
        if force {
            self.anim.force_play(name, 24.0, looping);
        } else if self.anim.current_anim != name || self.anim.finished {
            self.anim.play(name, 24.0, looping);
        }
    }

    pub fn play_sing(&mut self, lane: usize) {
        let anim_name = character::SING_DIRECTIONS[lane];
        self.play_anim(anim_name, true);
        self.hold_timer = 0.0;
    }

    pub fn play_miss(&mut self, lane: usize) {
        let anim_name = character::MISS_DIRECTIONS[lane];
        self.play_anim(anim_name, true);
        self.hold_timer = 0.0;
    }

    pub fn dance(&mut self) {
        if self.has_dance_idle {
            self.dance_left = !self.dance_left;
            let name = if self.dance_left { "danceLeft" } else { "danceRight" };
            self.play_anim(name, false);
        } else {
            self.play_anim("idle", false);
        }
    }

    pub fn update(&mut self, dt: f32) {
        let count = self.atlas.frame_count(&self.anim.current_anim);
        self.anim.update(dt, count);
    }

    /// Get the character's midpoint (center of sprite in world coords).
    /// Uses the current animation's first frame dimensions.
    pub fn midpoint(&self) -> (f32, f32) {
        let anim_name = if self.has_dance_idle { "danceLeft" } else { "idle" };
        let (fw, fh) = self.atlas.get_frame(anim_name, 0)
            .map(|f| (f.frame_w, f.frame_h))
            .unwrap_or((300.0, 400.0));
        (self.x + fw * self.scale / 2.0, self.y + fh * self.scale / 2.0)
    }

    fn current_offset(&self) -> (f32, f32) {
        if let Some(off) = self.offsets.get(&self.anim.current_anim) {
            (off[0] as f32, off[1] as f32)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn draw(&self, gpu: &mut GpuState, cam: &GameCamera) {
        let (off_x, off_y) = self.current_offset();
        let world_x = self.x - off_x;
        let world_y = self.y - off_y;

        let frame = match self.atlas.get_frame(&self.anim.current_anim, self.anim.frame_index) {
            Some(f) => f,
            None => return,
        };

        let (sx, sy) = cam.world_to_screen(world_x, world_y, GAME_W, GAME_H);
        let scale = self.scale * cam.zoom;

        gpu.draw_sprite_frame(
            frame,
            self.tex_w,
            self.tex_h,
            sx,
            sy,
            scale,
            self.flip_x,
            [1.0, 1.0, 1.0, 1.0],
        );
    }
}

/// A simple stage background sprite (static image, no animation).
pub struct StageBgSprite {
    pub texture: GpuTexture,
    tex_w: f32,
    tex_h: f32,
    x: f32,
    y: f32,
    scale: f32,
    scroll_x: f32,
    scroll_y: f32,
    flip_x: bool,
}

impl StageBgSprite {
    pub fn new(
        texture: GpuTexture,
        x: f32,
        y: f32,
        scale: f32,
        scroll_x: f32,
        scroll_y: f32,
        flip_x: bool,
    ) -> Self {
        Self {
            tex_w: texture.width as f32,
            tex_h: texture.height as f32,
            texture,
            x,
            y,
            scale,
            scroll_x,
            scroll_y,
            flip_x,
        }
    }

    pub fn draw(&self, gpu: &mut GpuState, cam: &GameCamera) {
        // HaxeFlixel camera rendering:
        //   scroll = cam_center - screen_size / 2  (NO zoom division)
        //   buffer_pos = sprite.x - scroll * scrollFactor
        //   screen_pos = (buffer_pos - screen_size/2) * zoom + screen_size/2
        // The zoom is a scale transform centered on screen, applied AFTER
        // the scroll/parallax computation.
        // Note: updateHitbox's offset and origin cancel out in rendering —
        // the visual result is the same with or without it. So no hitbox offset needed.
        let scroll_x = cam.x - GAME_W / 2.0;
        let scroll_y = cam.y - GAME_H / 2.0;
        let buf_x = self.x - scroll_x * self.scroll_x;
        let buf_y = self.y - scroll_y * self.scroll_y;
        let sx = (buf_x - GAME_W / 2.0) * cam.zoom + GAME_W / 2.0;
        let sy = (buf_y - GAME_H / 2.0) * cam.zoom + GAME_H / 2.0;
        let scale = self.scale * cam.zoom;
        let w = self.tex_w * scale;
        let h = self.tex_h * scale;

        gpu.push_texture_region(
            self.tex_w,
            self.tex_h,
            0.0,
            0.0,
            self.tex_w,
            self.tex_h,
            sx,
            sy,
            w,
            h,
            self.flip_x,
            [1.0, 1.0, 1.0, 1.0],
        );
    }
}
