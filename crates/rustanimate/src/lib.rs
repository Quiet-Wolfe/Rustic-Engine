//! A Rust port of `flxanimate`, designed to parse and render Adobe Animate texture atlases.
//!
//! This library is renderer-agnostic. It parses the JSON data exported from Adobe Animate
//! and processes the hierarchical timeline data, yielding a flat list of `DrawCall`s containing
//! 2D vertices, UV coordinates, and color tints for each frame. These draw calls can be consumed
//! by any graphics backend (like `wgpu`, `macroquad`, `bevy`, etc.).

pub mod animation;
pub mod spritemap;

use std::collections::HashMap;
use std::path::Path;

/// Represents a single vertex in a 2D quad to be rendered.
#[derive(Debug, Clone)]
pub struct RenderVertex {
    /// The 2D world position of the vertex `[x, y]`.
    pub position: [f32; 2],
    /// The normalized texture coordinates `[u, v]`.
    pub uv: [f32; 2],
    /// The color tint to apply to this vertex `[r, g, b, a]` (0.0 to 1.0).
    pub color: [f32; 4],
}

/// Represents a single quad (sprite) to be drawn on the screen.
#[derive(Debug, Clone)]
pub struct DrawCall {
    /// The 4 vertices making up the corners of the quad.
    pub vertices: [RenderVertex; 4],
    /// The indices defining the two triangles of the quad, typically `[0, 1, 2, 0, 2, 3]`.
    pub indices: [u16; 6],
}

/// The main structure representing an active Adobe Animate animation.
/// It holds the parsed data and the current state of playback.
pub struct FlxAnimate {
    /// The parsed `Animation.json` structure.
    pub atlas: animation::AnimAtlas,
    /// The parsed `spritemap1.json` structure.
    pub sprites: spritemap::AnimateAtlas,
    /// A fast-lookup map for sprite data (dimensions, atlas coordinates).
    pub sprite_map: HashMap<String, spritemap::AnimateSpriteData>,
    /// A fast-lookup map for symbol definitions (timelines, layers).
    pub symbol_map: HashMap<String, animation::SymbolData>,
    /// The current frame index being played.
    pub current_frame: u32,
    /// The playback framerate, parsed from the animation metadata.
    pub framerate: f32,
    /// Internal accumulator used to track time across `update` calls.
    pub time_accumulator: f32,
    /// The name of the symbol currently being played.
    pub playing_symbol: String,
    /// A list of available top-level animations (useful for multi-animation atlases).
    pub available_animations: Vec<animation::SymbolInstance>,
    /// The index of the currently active animation within `available_animations`.
    pub active_anim_idx: usize,
    /// Whether the current animation loops (true) or plays once (false).
    pub looping: bool,
    /// Whether a non-looping animation has reached its last frame.
    pub finished: bool,
}

impl FlxAnimate {
    /// Loads the animation data from a directory.
    ///
    /// The directory must contain:
    /// - `Animation.json`
    /// - `spritemap1.json`
    ///
    /// The image file (`spritemap1.png`) must be loaded separately by your specific renderer.
    pub fn load(folder_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let anim_path = Path::new(folder_path).join("Animation.json");
        let sprite_path = Path::new(folder_path).join("spritemap1.json");

        let anim_str = std::fs::read_to_string(anim_path)?;
        let sprite_str = std::fs::read_to_string(sprite_path)?;

        let anim_str = anim_str.trim_start_matches('\u{feff}');
        let sprite_str = sprite_str.trim_start_matches('\u{feff}');

        let atlas: animation::AnimAtlas = serde_json::from_str(anim_str)?;
        let sprites: spritemap::AnimateAtlas = serde_json::from_str(sprite_str)?;

        let mut sprite_map = HashMap::new();
        for s in &sprites.atlas.sprites {
            sprite_map.insert(s.sprite.name.clone(), spritemap::AnimateSpriteData {
                name: s.sprite.name.clone(),
                x: s.sprite.x,
                y: s.sprite.y,
                w: s.sprite.w,
                h: s.sprite.h,
                rotated: s.sprite.rotated,
            });
        }

        let mut symbol_map = HashMap::new();
        if let Some(sd) = &atlas.sd {
            for symbol in &sd.s {
                let cloned_str = serde_json::to_string(symbol)?;
                let cloned_sym: animation::SymbolData = serde_json::from_str(&cloned_str)?;
                symbol_map.insert(symbol.sn.clone(), cloned_sym);
            }
        }
        
        let main_sn = atlas.an.sn.clone().unwrap_or_else(|| atlas.an.n.clone());
        if let Some(tl) = &atlas.an.tl {
            symbol_map.insert(main_sn.clone(), animation::SymbolData {
                sn: main_sn.clone(),
                tl: tl.clone(),
            });
        }

        let framerate = atlas.md.as_ref().and_then(|md| md.frt).unwrap_or(24.0);
        
        let mut available_animations = Vec::new();
        if let Some(main_sym) = symbol_map.get(&main_sn) {
            for layer in &main_sym.tl.l {
                for frame in &layer.fr {
                    for element in &frame.e {
                        if let Some(si) = &element.si {
                            available_animations.push(si.clone());
                        }
                    }
                }
            }
        }

        let playing_symbol = if !available_animations.is_empty() {
            available_animations[0].sn.clone()
        } else {
            main_sn.clone()
        };

        Ok(Self {
            atlas,
            sprites,
            sprite_map,
            symbol_map,
            current_frame: 0,
            framerate,
            time_accumulator: 0.0,
            playing_symbol,
            available_animations,
            active_anim_idx: 0,
            looping: true,
            finished: false,
        })
    }

    pub fn next_anim(&mut self) {
        if !self.available_animations.is_empty() {
            self.active_anim_idx = (self.active_anim_idx + 1) % self.available_animations.len();
            self.playing_symbol = self.available_animations[self.active_anim_idx].sn.clone();
            self.current_frame = 0;
            self.time_accumulator = 0.0;
        }
    }

    pub fn prev_anim(&mut self) {
        if !self.available_animations.is_empty() {
            if self.active_anim_idx == 0 {
                self.active_anim_idx = self.available_animations.len() - 1;
            } else {
                self.active_anim_idx -= 1;
            }
            self.playing_symbol = self.available_animations[self.active_anim_idx].sn.clone();
            self.current_frame = 0;
            self.time_accumulator = 0.0;
        }
    }

    /// Whether to loop the current animation. Set to false for play-once behavior.
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// Whether the current (non-looping) animation has finished.
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// Get the total frame count of the currently playing symbol.
    pub fn timeline_length(&self) -> u32 {
        if let Some(symbol) = self.symbol_map.get(&self.playing_symbol) {
            let mut max_len = 0;
            for layer in &symbol.tl.l {
                if let Some(last_frame) = layer.fr.last() {
                    max_len = max_len.max(last_frame.i + last_frame.du);
                }
            }
            max_len
        } else {
            1
        }
    }

    pub fn update(&mut self, dt: f32) {
        if self.finished {
            return;
        }
        self.time_accumulator += dt;
        let frame_duration = 1.0 / self.framerate;
        while self.time_accumulator >= frame_duration {
            self.time_accumulator -= frame_duration;
            self.current_frame += 1;
        }

        let length = self.timeline_length();
        if length > 0 && self.current_frame >= length {
            if self.looping {
                self.current_frame %= length;
            } else {
                self.current_frame = length.saturating_sub(1);
                self.finished = true;
            }
        }
    }

    pub fn render(&self, x: f32, y: f32) -> Vec<DrawCall> {
        let mut draw_calls = Vec::new();
        let base_transform = glam::Mat3::from_translation(glam::vec2(x, y));

        if !self.available_animations.is_empty() {
            let si = &self.available_animations[self.active_anim_idx];
            self.draw_symbol_instance(si, base_transform, self.current_frame, &mut draw_calls);
        } else if let Some(si) = &self.atlas.an.sti {
            self.draw_symbol_instance(&si.si, base_transform, self.current_frame, &mut draw_calls);
        } else {
            let dummy_si = animation::SymbolInstance {
                sn: self.playing_symbol.clone(),
                in_name: None,
                st: None,
                ff: None,
                lp: None,
                trp: None,
                m3d: None,
                mx: None,
            };
            self.draw_symbol_instance(&dummy_si, base_transform, self.current_frame, &mut draw_calls);
        }

        draw_calls
    }

    /// Render a specific symbol directly, without the main timeline's positioning transform.
    /// Used by character rendering where each animation symbol should be rendered at origin,
    /// not at its position on the main timeline canvas.
    pub fn render_symbol(&self, symbol_name: &str, x: f32, y: f32) -> Vec<DrawCall> {
        let mut draw_calls = Vec::new();
        let base_transform = glam::Mat3::from_translation(glam::vec2(x, y));

        let dummy_si = animation::SymbolInstance {
            sn: symbol_name.to_string(),
            in_name: None,
            st: None,
            ff: None,
            lp: None,
            trp: None,
            m3d: None,
            mx: None,
        };
        self.draw_symbol_instance(&dummy_si, base_transform, self.current_frame, &mut draw_calls);

        draw_calls
    }

    fn draw_symbol_instance(
        &self, 
        si: &animation::SymbolInstance, 
        parent_transform: glam::Mat3, 
        parent_frame: u32,
        draw_calls: &mut Vec<DrawCall>
    ) {
        // Resolve matrix
        let mut local_transform = glam::Mat3::IDENTITY;
        
        // Use M3D if available
        if let Some(m3d) = &si.m3d {
            if m3d.len() == 16 {
                local_transform = glam::Mat3::from_cols_array(&[
                    m3d[0], m3d[1], 0.0,
                    m3d[4], m3d[5], 0.0,
                    m3d[12], m3d[13], 1.0,
                ]);
            }
        } else if let Some(mx) = &si.mx {
            if mx.len() == 6 {
                local_transform = glam::Mat3::from_cols_array(&[
                    mx[0], mx[1], 0.0,
                    mx[2], mx[3], 0.0,
                    mx[4], mx[5], 1.0,
                ]);
            }
        }

        let transform = parent_transform * local_transform;

        // Draw symbol
        if let Some(symbol) = self.symbol_map.get(&si.sn) {
            // Find length of this symbol's timeline
            let mut child_len = 1;
            for layer in &symbol.tl.l {
                if let Some(last_frame) = layer.fr.last() {
                    child_len = child_len.max(last_frame.i + last_frame.du);
                }
            }

            // Apply loop type and first frame
            let ff = si.ff.unwrap_or(0);
            let mut local_frame = parent_frame + ff;
            
            // Handle looping (assuming Loop by default for now if lp is missing)
            if si.lp.as_deref() == Some("PO") { // Play Once
                local_frame = local_frame.min(child_len.saturating_sub(1));
            } else if si.lp.as_deref() == Some("SF") { // Single Frame
                local_frame = ff;
            } else { // LP (Loop) or None
                if child_len > 0 {
                    local_frame = local_frame % child_len;
                }
            }

            for layer in symbol.tl.l.iter().rev() {
                // Find the keyframe in the layer
                let mut current_kf = None;
                for kf in &layer.fr {
                    if local_frame >= kf.i && local_frame < kf.i + kf.du {
                        current_kf = Some(kf);
                        break;
                    }
                }

                if let Some(kf) = current_kf {
                    let relative_frame = local_frame - kf.i;
                    for element in &kf.e {
                        if let Some(child_si) = &element.si {
                            self.draw_symbol_instance(child_si, transform, relative_frame, draw_calls);
                        } else if let Some(asi) = &element.asi {
                            self.draw_atlas_symbol(asi, transform, draw_calls);
                        }
                    }
                }
            }
        }
    }

    fn draw_atlas_symbol(&self, asi: &animation::AtlasSymbolInstance, parent_transform: glam::Mat3, draw_calls: &mut Vec<DrawCall>) {
        let mut local_transform = glam::Mat3::IDENTITY;

        if let Some(m3d) = &asi.m3d {
            if m3d.len() == 16 {
                local_transform = glam::Mat3::from_cols_array(&[
                    m3d[0], m3d[1], 0.0,
                    m3d[4], m3d[5], 0.0,
                    m3d[12], m3d[13], 1.0,
                ]);
            }
        } else if let Some(mx) = &asi.mx {
            if mx.len() == 6 {
                local_transform = glam::Mat3::from_cols_array(&[
                    mx[0], mx[1], 0.0,
                    mx[2], mx[3], 0.0,
                    mx[4], mx[5], 1.0,
                ]);
            }
        }

        let transform = parent_transform * local_transform;

        if let Some(sprite_data) = self.sprite_map.get(&asi.n) {
            let offset_x = asi.pos.as_ref().map_or(0.0, |p| p.x);
            let offset_y = asi.pos.as_ref().map_or(0.0, |p| p.y);

            let (p0, p1, p2, p3) = if sprite_data.rotated {
                let w = sprite_data.h;
                let h = sprite_data.w;
                let p0 = transform.transform_point2(glam::vec2(-offset_x, -offset_y));
                let p1 = transform.transform_point2(glam::vec2(w - offset_x, -offset_y));
                let p2 = transform.transform_point2(glam::vec2(w - offset_x, h - offset_y));
                let p3 = transform.transform_point2(glam::vec2(-offset_x, h - offset_y));
                (p0, p1, p2, p3)
            } else {
                let w = sprite_data.w;
                let h = sprite_data.h;
                let p0 = transform.transform_point2(glam::vec2(-offset_x, -offset_y));
                let p1 = transform.transform_point2(glam::vec2(w - offset_x, -offset_y));
                let p2 = transform.transform_point2(glam::vec2(w - offset_x, h - offset_y));
                let p3 = transform.transform_point2(glam::vec2(-offset_x, h - offset_y));
                (p0, p1, p2, p3)
            };

            let tw = self.sprites.meta.size.w;
            let th = self.sprites.meta.size.h;

            let u0 = sprite_data.x / tw;
            let v0 = sprite_data.y / th;
            let u1 = (sprite_data.x + sprite_data.w) / tw;
            let v1 = (sprite_data.y + sprite_data.h) / th;

            let (uv0, uv1, uv2, uv3) = if sprite_data.rotated {
                // Needs special mapping if rotated in atlas
                (
                    glam::vec2(u0, v1),
                    glam::vec2(u0, v0),
                    glam::vec2(u1, v0),
                    glam::vec2(u1, v1),
                )
            } else {
                (
                    glam::vec2(u0, v0),
                    glam::vec2(u1, v0),
                    glam::vec2(u1, v1),
                    glam::vec2(u0, v1),
                )
            };

            let color = [1.0, 1.0, 1.0, 1.0]; // Default white tint
            
            let vertex_0 = RenderVertex { position: [p0.x, p0.y], uv: [uv0.x, uv0.y], color };
            let vertex_1 = RenderVertex { position: [p1.x, p1.y], uv: [uv1.x, uv1.y], color };
            let vertex_2 = RenderVertex { position: [p2.x, p2.y], uv: [uv2.x, uv2.y], color };
            let vertex_3 = RenderVertex { position: [p3.x, p3.y], uv: [uv3.x, uv3.y], color };

            let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
            
            draw_calls.push(DrawCall {
                vertices: [vertex_0, vertex_1, vertex_2, vertex_3],
                indices,
            });
        }
    }
}
