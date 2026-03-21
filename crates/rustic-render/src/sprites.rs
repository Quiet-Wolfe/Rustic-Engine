use macroquad::prelude::*;
use std::collections::HashMap;

/// A single frame within a sprite atlas.
#[derive(Debug, Clone)]
pub struct SpriteFrame {
    /// Source rectangle on the spritesheet.
    pub src: Rect,
    /// Frame offset from trimming (frameX/frameY).
    pub offset_x: f32,
    pub offset_y: f32,
    /// Full frame dimensions before trimming.
    pub frame_w: f32,
    pub frame_h: f32,
    /// Whether this frame is rotated 90deg CW in the atlas.
    pub rotated: bool,
}

/// A parsed Sparrow atlas containing named animations mapped to frame sequences.
#[derive(Debug, Clone)]
pub struct SpriteAtlas {
    pub animations: HashMap<String, Vec<SpriteFrame>>,
}

impl SpriteAtlas {
    /// Parse a Sparrow XML atlas string.
    pub fn from_xml(xml_data: &str) -> Self {
        let mut animations: HashMap<String, Vec<(u32, SpriteFrame)>> = HashMap::new();

        let mut reader = quick_xml::Reader::from_str(xml_data);
        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Empty(ref e))
                | Ok(quick_xml::events::Event::Start(ref e)) => {
                    if e.name().as_ref() == b"SubTexture" {
                        let mut name = String::new();
                        let mut x = 0.0f32;
                        let mut y = 0.0f32;
                        let mut w = 0.0f32;
                        let mut h = 0.0f32;
                        let mut fx = 0.0f32;
                        let mut fy = 0.0f32;
                        let mut fw = 0.0f32;
                        let mut fh = 0.0f32;
                        let mut has_frame = false;
                        let mut rotated = false;

                        for attr in e.attributes().flatten() {
                            let val = String::from_utf8_lossy(attr.value.as_ref()).to_string();
                            match attr.key.as_ref() {
                                b"name" => name = val,
                                b"x" => x = val.parse().unwrap_or(0.0),
                                b"y" => y = val.parse().unwrap_or(0.0),
                                b"width" => w = val.parse().unwrap_or(0.0),
                                b"height" => h = val.parse().unwrap_or(0.0),
                                b"frameX" => {
                                    fx = val.parse().unwrap_or(0.0);
                                    has_frame = true;
                                }
                                b"frameY" => fy = val.parse().unwrap_or(0.0),
                                b"frameWidth" => fw = val.parse().unwrap_or(0.0),
                                b"frameHeight" => fh = val.parse().unwrap_or(0.0),
                                b"rotated" => rotated = val == "true",
                                _ => {}
                            }
                        }

                        if !has_frame {
                            fw = if rotated { h } else { w };
                            fh = if rotated { w } else { h };
                        }

                        let (anim_name, frame_idx) = split_anim_name(&name);

                        let frame = SpriteFrame {
                            src: Rect::new(x, y, w, h),
                            offset_x: fx,
                            offset_y: fy,
                            frame_w: fw,
                            frame_h: fh,
                            rotated,
                        };

                        animations
                            .entry(anim_name)
                            .or_default()
                            .push((frame_idx, frame));
                    }
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        let animations = animations
            .into_iter()
            .map(|(name, mut frames)| {
                frames.sort_by_key(|(idx, _)| *idx);
                (name, frames.into_iter().map(|(_, f)| f).collect())
            })
            .collect();

        SpriteAtlas { animations }
    }

    pub fn get_frame(&self, anim: &str, frame: usize) -> Option<&SpriteFrame> {
        self.animations.get(anim).and_then(|frames| {
            if frames.is_empty() {
                None
            } else {
                Some(&frames[frame % frames.len()])
            }
        })
    }

    pub fn frame_count(&self, anim: &str) -> usize {
        self.animations.get(anim).map_or(0, |f| f.len())
    }

    pub fn has_anim(&self, anim: &str) -> bool {
        self.animations.contains_key(anim)
    }

    /// List all animation names in this atlas.
    pub fn anim_names(&self) -> Vec<&str> {
        self.animations.keys().map(|s| s.as_str()).collect()
    }
}

/// Split "purple0000" -> ("purple", 0), "BF idle dance0003" -> ("BF idle dance", 3)
fn split_anim_name(name: &str) -> (String, u32) {
    let digit_start = name
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(i, _)| i);

    match digit_start {
        Some(i) if i > 0 => {
            let prefix = name[..i].to_string();
            let num: u32 = name[i..].parse().unwrap_or(0);
            (prefix, num)
        }
        _ => (name.to_string(), 0),
    }
}

/// Draw a sprite frame from an atlas texture at the given position.
pub fn draw_sprite_frame(
    texture: &Texture2D,
    frame: &SpriteFrame,
    x: f32,
    y: f32,
    scale: f32,
    flip_x: bool,
    color: Color,
) {
    if frame.rotated {
        let actual_w = frame.src.h;
        let actual_h = frame.src.w;

        let draw_x = if flip_x {
            x + (frame.frame_w + frame.offset_x - actual_w) * scale
        } else {
            x - frame.offset_x * scale
        };
        let draw_y = y - frame.offset_y * scale;

        let params = DrawTextureParams {
            source: Some(frame.src),
            dest_size: Some(Vec2::new(actual_w * scale, actual_h * scale)),
            flip_x,
            rotation: -std::f32::consts::FRAC_PI_2,
            pivot: Some(Vec2::new(0.0, 0.0)),
            ..Default::default()
        };

        draw_texture_ex(texture, draw_x, draw_y + actual_h * scale, color, params);
    } else {
        let params = DrawTextureParams {
            source: Some(frame.src),
            dest_size: Some(Vec2::new(frame.src.w * scale, frame.src.h * scale)),
            flip_x,
            ..Default::default()
        };

        let draw_x = if flip_x {
            x + (frame.frame_w + frame.offset_x - frame.src.w) * scale
        } else {
            x - frame.offset_x * scale
        };
        let draw_y = y - frame.offset_y * scale;

        draw_texture_ex(texture, draw_x, draw_y, color, params);
    }
}

/// Animation controller for a sprite atlas.
pub struct AnimationController {
    pub current_anim: String,
    /// Current position in the frame sequence (index into `indices` if set, otherwise direct atlas frame).
    pub frame_index: usize,
    pub fps: f32,
    pub looping: bool,
    pub finished: bool,
    /// When non-empty, maps frame_index -> specific atlas frame indices.
    /// e.g. [1, 4, 5, 6, 7, 9, 1] means frame_index 0 shows atlas frame 1, etc.
    pub indices: Vec<usize>,
    timer: f32,
}

impl AnimationController {
    pub fn new() -> Self {
        Self {
            current_anim: String::new(),
            frame_index: 0,
            fps: 24.0,
            looping: false,
            finished: false,
            indices: Vec::new(),
            timer: 0.0,
        }
    }

    /// Switch to a new animation, resetting the frame counter.
    /// `indices` selects specific atlas frames; empty = use all frames in order.
    pub fn play(&mut self, anim: &str, fps: f32, looping: bool, indices: &[i32]) {
        if self.current_anim != anim || self.finished {
            self.current_anim = anim.to_string();
            self.frame_index = 0;
            self.timer = 0.0;
            self.finished = false;
        }
        self.fps = fps;
        self.looping = looping;
        self.indices = indices.iter().map(|&i| i as usize).collect();
    }

    /// The total number of frames in the current animation sequence.
    pub fn sequence_length(&self, atlas_frame_count: usize) -> usize {
        if self.indices.is_empty() {
            atlas_frame_count
        } else {
            self.indices.len()
        }
    }

    /// Get the actual atlas frame index for the current position.
    pub fn atlas_frame(&self) -> usize {
        if self.indices.is_empty() {
            self.frame_index
        } else {
            self.indices.get(self.frame_index).copied().unwrap_or(0)
        }
    }

    /// Advance the animation timer.
    pub fn update(&mut self, dt: f32, atlas_frame_count: usize) {
        let total = self.sequence_length(atlas_frame_count);
        if self.finished || total == 0 || self.fps <= 0.0 {
            return;
        }

        self.timer += dt;
        let frame_duration = 1.0 / self.fps;

        while self.timer >= frame_duration {
            self.timer -= frame_duration;
            self.frame_index += 1;

            if self.frame_index >= total {
                if self.looping {
                    self.frame_index = 0;
                } else {
                    self.frame_index = total - 1;
                    self.finished = true;
                    return;
                }
            }
        }
    }
}

impl Default for AnimationController {
    fn default() -> Self {
        Self::new()
    }
}
