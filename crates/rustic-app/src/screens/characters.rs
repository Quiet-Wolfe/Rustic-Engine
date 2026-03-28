use std::collections::HashMap;
use std::path::Path;

use rustic_core::character::{self, CharacterFile};
use rustic_render::camera::GameCamera;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::{AnimationController, SpriteAtlas};
use rustanimate::FlxAnimate;

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
    pub alpha: f32,
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
    /// Animation suffix appended to sing/dance anims (e.g. "-alt", "-DAD").
    pub anim_suffix: String,
    /// Idle suffix appended to idle/dance anims (e.g. "-DAD", "-GENERAL").
    pub idle_suffix: String,
    /// When true, dance() won't interrupt the current animation until it finishes.
    /// Set by play_anim(force=true) for non-sing/idle animations (e.g. descend, ascend, intro).
    pub special_anim: bool,
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
            alpha: 1.0,
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
            anim_suffix: String::new(),
            idle_suffix: String::new(),
            special_anim: false,
        };

        if has_dance_idle {
            sprite.play_anim("danceLeft", false);
        } else {
            sprite.play_anim("idle", false);
        }

        sprite
    }

    /// Try playing `name + suffix` first, fall back to `name`.
    fn resolve_anim_name(&self, name: &str) -> String {
        if !self.anim_suffix.is_empty() {
            let suffixed = format!("{}{}", name, self.anim_suffix);
            if self.atlas.frame_count(&suffixed) > 0 {
                return suffixed;
            }
        }
        name.to_string()
    }

    /// Resolve idle animation name using idle_suffix (for dance/idle only).
    fn resolve_idle_name(&self, name: &str) -> String {
        if !self.idle_suffix.is_empty() {
            let suffixed = format!("{}{}", name, self.idle_suffix);
            if self.atlas.frame_count(&suffixed) > 0 {
                return suffixed;
            }
        }
        name.to_string()
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
            // Mark as special animation if it's not a standard sing/miss/idle/dance.
            // Special animations prevent dance() from interrupting until they finish.
            let is_standard = name.starts_with("sing") || name.starts_with("miss")
                || name == "idle" || name.starts_with("dance");
            self.special_anim = !is_standard;
        } else if self.anim.current_anim != name || self.anim.finished {
            self.anim.play(name, 24.0, looping);
        }
    }

    pub fn play_sing(&mut self, lane: usize) {
        let base = character::SING_DIRECTIONS[lane];
        let resolved = self.resolve_anim_name(base);
        self.special_anim = false;
        self.play_anim(&resolved, true);
        self.hold_timer = 0.0;
    }

    pub fn play_miss(&mut self, lane: usize) {
        let base = character::MISS_DIRECTIONS[lane];
        let resolved = self.resolve_anim_name(base);
        self.special_anim = false;
        self.play_anim(&resolved, true);
        self.hold_timer = 0.0;
    }

    pub fn dance(&mut self) {
        // Don't interrupt special animations (descend, ascend, intro, etc.) until they finish
        if self.special_anim {
            if !self.anim.finished {
                return;
            }
            self.special_anim = false;
        }
        if self.has_dance_idle {
            self.dance_left = !self.dance_left;
            let base = if self.dance_left { "danceLeft" } else { "danceRight" };
            let resolved = self.resolve_idle_name(base);
            self.play_anim(&resolved, false);
        } else {
            let resolved = self.resolve_idle_name("idle");
            self.play_anim(&resolved, false);
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
            [1.0, 1.0, 1.0, self.alpha],
        );
    }

    /// Draw a flipped, transparent copy below the character (reflection effect).
    pub fn draw_reflection(&self, gpu: &mut GpuState, cam: &GameCamera, alpha: f32, dist_y: f32) {
        let (off_x, off_y) = self.current_offset();
        let world_x = self.x - off_x;
        let world_y = self.y - off_y;

        let frame = match self.atlas.get_frame(&self.anim.current_anim, self.anim.frame_index) {
            Some(f) => f,
            None => return,
        };

        // Reflection is placed below the character: y + frame_height + dist_y (in world space)
        let display_h = if frame.rotated { frame.src.w } else { frame.src.h };
        let reflect_world_y = world_y + display_h * self.scale + dist_y - off_y;

        let (sx, sy) = cam.world_to_screen(world_x, reflect_world_y, GAME_W, GAME_H);
        let scale = self.scale * cam.zoom;
        let a = alpha * self.alpha;

        gpu.draw_sprite_frame_flip_y(
            frame, self.tex_w, self.tex_h,
            sx, sy, scale, self.flip_x,
            [a, a, a, a],
        );
    }
}

// =============================================================================
// Adobe Animate Atlas Character (used by Nightflaid, etc.)
// =============================================================================

/// A character backed by an Adobe Animate texture atlas (Animation.json + spritemap).
pub struct AtlasCharacterSprite {
    pub texture: GpuTexture,
    animate: FlxAnimate,
    /// Maps engine anim name ("idle") → index in FlxAnimate::available_animations.
    anim_indices: HashMap<String, usize>,
    /// Maps engine anim name → symbol name in the atlas.
    symbol_names: HashMap<String, String>,
    /// Per-animation loop flags from character JSON.
    loop_flags: HashMap<String, bool>,
    /// Current engine animation name.
    current_anim: String,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub alpha: f32,
    flip_x: bool,
    offsets: HashMap<String, [f64; 2]>,
    pub hold_timer: f64,
    pub sing_duration: f64,
    pub has_dance_idle: bool,
    dance_left: bool,
    pub healthbar_colors: [u8; 3],
    pub healthicon: String,
    pub camera_position: [f64; 2],
    /// Animation suffix appended to sing/dance anims (e.g. "-alt", "-DAD").
    pub anim_suffix: String,
    /// Idle suffix appended to idle/dance anims (e.g. "-DAD", "-GENERAL").
    pub idle_suffix: String,
    /// When true, dance() won't interrupt the current animation until it finishes.
    /// Set by play_anim(force=true) for non-sing/idle animations (e.g. descend, ascend, intro).
    pub special_anim: bool,
}

impl AtlasCharacterSprite {
    pub fn load(
        gpu: &GpuState,
        char_def: &CharacterFile,
        atlas_dir: &Path,
        stage_x: f64,
        stage_y: f64,
        is_player: bool,
    ) -> Self {
        let atlas_dir_str = atlas_dir.to_str().unwrap_or("");
        let animate = FlxAnimate::load(atlas_dir_str)
            .unwrap_or_else(|e| panic!("Failed to load Animate atlas {:?}: {}", atlas_dir, e));

        let png_path = atlas_dir.join("spritemap1.png");
        let texture = gpu.load_texture_from_path(&png_path);

        // Build anim name → available_animations index + symbol name mapping.
        let mut anim_indices = HashMap::new();
        let mut symbol_names = HashMap::new();
        let mut offsets = HashMap::new();
        let mut loop_flags = HashMap::new();
        for anim_def in &char_def.animations {
            for (idx, avail) in animate.available_animations.iter().enumerate() {
                if avail.sn == anim_def.name {
                    anim_indices.insert(anim_def.anim.clone(), idx);
                    break;
                }
            }
            symbol_names.insert(anim_def.anim.clone(), anim_def.name.clone());
            offsets.insert(anim_def.anim.clone(), anim_def.offsets);
            loop_flags.insert(anim_def.anim.clone(), anim_def.loop_anim);
        }

        let flip_x = char_def.flip_x != is_player;
        let x = (stage_x + char_def.position[0]) as f32;
        let y = (stage_y + char_def.position[1]) as f32;
        let has_dance_idle = char_def.has_dance_idle();

        let mut sprite = AtlasCharacterSprite {
            texture,
            animate,
            anim_indices,
            symbol_names,
            loop_flags,
            current_anim: String::new(),
            x,
            y,
            scale: char_def.scale as f32,
            alpha: 1.0,
            flip_x,
            offsets,
            hold_timer: 0.0,
            sing_duration: char_def.sing_duration,
            has_dance_idle,
            dance_left: false,
            healthbar_colors: char_def.healthbar_colors,
            healthicon: char_def.healthicon.clone(),
            camera_position: char_def.camera_position,
            anim_suffix: String::new(),
            idle_suffix: String::new(),
            special_anim: false,
        };

        if has_dance_idle {
            sprite.play_anim("danceLeft", false);
        } else {
            sprite.play_anim("idle", false);
        }

        sprite
    }

    /// Try playing `name + suffix` first, fall back to `name`.
    fn resolve_anim_name(&self, name: &str) -> String {
        if !self.anim_suffix.is_empty() {
            let suffixed = format!("{}{}", name, self.anim_suffix);
            if self.symbol_names.contains_key(&suffixed) {
                return suffixed;
            }
        }
        name.to_string()
    }

    /// Resolve idle animation name using idle_suffix (for dance/idle only).
    fn resolve_idle_name(&self, name: &str) -> String {
        if !self.idle_suffix.is_empty() {
            let suffixed = format!("{}{}", name, self.idle_suffix);
            if self.symbol_names.contains_key(&suffixed) {
                return suffixed;
            }
        }
        name.to_string()
    }

    pub fn play_anim(&mut self, name: &str, force: bool) {
        if !force && self.current_anim == name && !self.animate.finished() {
            return;
        }
        if let Some(symbol_name) = self.symbol_names.get(name) {
            self.animate.playing_symbol = symbol_name.clone();
            self.animate.current_frame = 0;
            self.animate.time_accumulator = 0.0;
            self.animate.finished = false;
            let looping = self.loop_flags.get(name).copied()
                .unwrap_or_else(|| name.contains("-loop") || name == "idle");
            self.animate.set_looping(looping);
            if let Some(&idx) = self.anim_indices.get(name) {
                self.animate.active_anim_idx = idx;
            }
            self.current_anim = name.to_string();
            if force {
                let is_standard = name.starts_with("sing") || name.starts_with("miss")
                    || name == "idle" || name.starts_with("dance");
                self.special_anim = !is_standard;
            }
        } else {
            log::warn!("Atlas character: animation '{}' not found", name);
        }
    }

    pub fn play_sing(&mut self, lane: usize) {
        let base = character::SING_DIRECTIONS[lane];
        let resolved = self.resolve_anim_name(base);
        self.special_anim = false;
        self.play_anim(&resolved, true);
        self.hold_timer = 0.0;
    }

    pub fn play_miss(&mut self, lane: usize) {
        let base = character::MISS_DIRECTIONS[lane];
        let resolved = self.resolve_anim_name(base);
        self.special_anim = false;
        self.play_anim(&resolved, true);
        self.hold_timer = 0.0;
    }

    pub fn dance(&mut self) {
        if self.special_anim {
            if !self.animate.finished() {
                return;
            }
            self.special_anim = false;
        }
        if self.has_dance_idle {
            self.dance_left = !self.dance_left;
            let base = if self.dance_left { "danceLeft" } else { "danceRight" };
            let resolved = self.resolve_idle_name(base);
            self.play_anim(&resolved, false);
        } else {
            let resolved = self.resolve_idle_name("idle");
            self.play_anim(&resolved, false);
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.animate.update(dt);
    }

    pub fn midpoint(&self) -> (f32, f32) {
        (self.x + 150.0 * self.scale, self.y + 200.0 * self.scale)
    }

    fn current_offset(&self) -> (f32, f32) {
        if let Some(off) = self.offsets.get(&self.current_anim) {
            (off[0] as f32, off[1] as f32)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn draw(&self, gpu: &mut GpuState, cam: &GameCamera) {
        let (off_x, off_y) = self.current_offset();
        let world_x = self.x - off_x;
        let world_y = self.y - off_y;

        let (screen_x, screen_y) = cam.world_to_screen(world_x, world_y, GAME_W, GAME_H);
        let scale = self.scale * cam.zoom;

        // render_symbol bypasses the main timeline M3D, rendering the symbol at origin
        let draw_calls = self.animate.render_symbol(&self.animate.playing_symbol, 0.0, 0.0);
        let flip_sign = if self.flip_x { -1.0f32 } else { 1.0 };

        for dc in &draw_calls {
            let positions: [[f32; 2]; 4] = std::array::from_fn(|i| {
                [screen_x + dc.vertices[i].position[0] * scale * flip_sign,
                 screen_y + dc.vertices[i].position[1] * scale]
            });
            let uvs: [[f32; 2]; 4] = std::array::from_fn(|i| dc.vertices[i].uv);
            let mut color = dc.vertices[0].color;
            color[3] *= self.alpha;
            gpu.push_raw_quad(positions, uvs, color);
        }
    }

    /// Draw a flipped, transparent copy below the character (reflection effect).
    pub fn draw_reflection(&self, gpu: &mut GpuState, cam: &GameCamera, alpha: f32, dist_y: f32) {
        let (off_x, off_y) = self.current_offset();
        let world_x = self.x - off_x;
        let world_y = self.y - off_y;

        let (screen_x, screen_y) = cam.world_to_screen(world_x, world_y, GAME_W, GAME_H);
        let scale = self.scale * cam.zoom;

        let draw_calls = self.animate.render_symbol(&self.animate.playing_symbol, 0.0, 0.0);
        let flip_sign = if self.flip_x { -1.0f32 } else { 1.0 };

        // Compute bounding box to find the character's height for reflection placement
        let mut max_y = 0.0f32;
        for dc in &draw_calls {
            for v in &dc.vertices {
                max_y = max_y.max(v.position[1]);
            }
        }
        let char_bottom = max_y * scale;
        let reflect_offset = char_bottom * 2.0 + dist_y * cam.zoom;

        for dc in &draw_calls {
            // Flip Y: mirror each vertex's Y position around the character bottom
            let positions: [[f32; 2]; 4] = std::array::from_fn(|i| {
                let px = screen_x + dc.vertices[i].position[0] * scale * flip_sign;
                // Reflect: new_y = screen_y + reflect_offset - (original_y - screen_y)
                let orig_y = screen_y + dc.vertices[i].position[1] * scale;
                let py = screen_y + reflect_offset - (orig_y - screen_y);
                [px, py]
            });
            let uvs: [[f32; 2]; 4] = std::array::from_fn(|i| dc.vertices[i].uv);
            let mut color = dc.vertices[0].color;
            color[3] *= alpha * self.alpha;
            gpu.push_raw_quad(positions, uvs, color);
        }
    }
}

// =============================================================================
// Character enum — wraps either Sparrow XML or Adobe Animate atlas character
// =============================================================================

pub enum Character {
    Sparrow(CharacterSprite),
    Atlas(AtlasCharacterSprite),
}

impl Character {
    pub fn play_anim(&mut self, name: &str, force: bool) {
        match self {
            Character::Sparrow(s) => s.play_anim(name, force),
            Character::Atlas(a) => a.play_anim(name, force),
        }
    }

    pub fn play_sing(&mut self, lane: usize) {
        match self {
            Character::Sparrow(s) => s.play_sing(lane),
            Character::Atlas(a) => a.play_sing(lane),
        }
    }

    pub fn play_miss(&mut self, lane: usize) {
        match self {
            Character::Sparrow(s) => s.play_miss(lane),
            Character::Atlas(a) => a.play_miss(lane),
        }
    }

    pub fn dance(&mut self) {
        match self {
            Character::Sparrow(s) => s.dance(),
            Character::Atlas(a) => a.dance(),
        }
    }

    pub fn update(&mut self, dt: f32) {
        match self {
            Character::Sparrow(s) => s.update(dt),
            Character::Atlas(a) => a.update(dt),
        }
    }

    pub fn draw(&self, gpu: &mut GpuState, cam: &GameCamera) {
        match self {
            Character::Sparrow(s) => s.draw(gpu, cam),
            Character::Atlas(a) => a.draw(gpu, cam),
        }
    }

    /// Draw a vertically-flipped transparent reflection below the character.
    pub fn draw_reflection(&self, gpu: &mut GpuState, cam: &GameCamera, alpha: f32, dist_y: f32) {
        match self {
            Character::Sparrow(s) => s.draw_reflection(gpu, cam, alpha, dist_y),
            Character::Atlas(a) => a.draw_reflection(gpu, cam, alpha, dist_y),
        }
    }

    pub fn midpoint(&self) -> (f32, f32) {
        match self {
            Character::Sparrow(s) => s.midpoint(),
            Character::Atlas(a) => a.midpoint(),
        }
    }

    pub fn texture(&self) -> &GpuTexture {
        match self {
            Character::Sparrow(s) => &s.texture,
            Character::Atlas(a) => &a.texture,
        }
    }

    pub fn x(&self) -> f32 {
        match self {
            Character::Sparrow(s) => s.x,
            Character::Atlas(a) => a.x,
        }
    }

    pub fn y(&self) -> f32 {
        match self {
            Character::Sparrow(s) => s.y,
            Character::Atlas(a) => a.y,
        }
    }

    pub fn set_x(&mut self, x: f32) {
        match self {
            Character::Sparrow(s) => s.x = x,
            Character::Atlas(a) => a.x = x,
        }
    }

    pub fn set_y(&mut self, y: f32) {
        match self {
            Character::Sparrow(s) => s.y = y,
            Character::Atlas(a) => a.y = y,
        }
    }

    pub fn scale(&self) -> f32 {
        match self {
            Character::Sparrow(s) => s.scale,
            Character::Atlas(a) => a.scale,
        }
    }

    pub fn set_scale(&mut self, scale: f32) {
        match self {
            Character::Sparrow(s) => s.scale = scale,
            Character::Atlas(a) => a.scale = scale,
        }
    }

    pub fn special_anim(&self) -> bool {
        match self {
            Character::Sparrow(s) => s.special_anim,
            Character::Atlas(a) => a.special_anim,
        }
    }

    pub fn hold_timer(&self) -> f64 {
        match self {
            Character::Sparrow(s) => s.hold_timer,
            Character::Atlas(a) => a.hold_timer,
        }
    }

    pub fn set_hold_timer(&mut self, t: f64) {
        match self {
            Character::Sparrow(s) => s.hold_timer = t,
            Character::Atlas(a) => a.hold_timer = t,
        }
    }

    pub fn sing_duration(&self) -> f64 {
        match self {
            Character::Sparrow(s) => s.sing_duration,
            Character::Atlas(a) => a.sing_duration,
        }
    }

    pub fn has_dance_idle(&self) -> bool {
        match self {
            Character::Sparrow(s) => s.has_dance_idle,
            Character::Atlas(a) => a.has_dance_idle,
        }
    }

    pub fn camera_position(&self) -> [f64; 2] {
        match self {
            Character::Sparrow(s) => s.camera_position,
            Character::Atlas(a) => a.camera_position,
        }
    }

    pub fn anim_finished(&self) -> bool {
        match self {
            Character::Sparrow(s) => s.anim.finished,
            Character::Atlas(a) => a.animate.finished(),
        }
    }

    pub fn current_anim(&self) -> &str {
        match self {
            Character::Sparrow(s) => &s.anim.current_anim,
            Character::Atlas(a) => &a.current_anim,
        }
    }

    pub fn anim_suffix(&self) -> &str {
        match self {
            Character::Sparrow(s) => &s.anim_suffix,
            Character::Atlas(a) => &a.anim_suffix,
        }
    }

    pub fn set_anim_suffix(&mut self, suffix: &str) {
        match self {
            Character::Sparrow(s) => s.anim_suffix = suffix.to_string(),
            Character::Atlas(a) => a.anim_suffix = suffix.to_string(),
        }
    }

    pub fn alpha(&self) -> f32 {
        match self {
            Character::Sparrow(s) => s.alpha,
            Character::Atlas(a) => a.alpha,
        }
    }

    pub fn set_alpha(&mut self, alpha: f32) {
        match self {
            Character::Sparrow(s) => s.alpha = alpha,
            Character::Atlas(a) => a.alpha = alpha,
        }
    }

    pub fn idle_suffix(&self) -> &str {
        match self {
            Character::Sparrow(s) => &s.idle_suffix,
            Character::Atlas(a) => &a.idle_suffix,
        }
    }

    pub fn set_idle_suffix(&mut self, suffix: &str) {
        match self {
            Character::Sparrow(s) => s.idle_suffix = suffix.to_string(),
            Character::Atlas(a) => a.idle_suffix = suffix.to_string(),
        }
    }

    pub fn current_anim_name(&self) -> &str {
        match self {
            Character::Sparrow(s) => &s.anim.current_anim,
            Character::Atlas(a) => &a.current_anim,
        }
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
