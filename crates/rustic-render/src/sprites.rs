use std::collections::HashMap;

/// Simple rectangle (replaces macroquad::Rect).
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A single frame within a sprite atlas.
#[derive(Debug, Clone)]
pub struct SpriteFrame {
    pub src: Rect,
    pub offset_x: f32,
    pub offset_y: f32,
    pub frame_w: f32,
    pub frame_h: f32,
    pub rotated: bool,
}

/// A raw SubTexture entry from the atlas XML, before animation grouping.
#[derive(Debug, Clone)]
struct RawFrame {
    name: String,
    frame: SpriteFrame,
}

/// A parsed Sparrow atlas. Stores raw frames from the XML and named animations
/// built via prefix matching (like HaxeFlixel's `addByPrefix`).
#[derive(Debug, Clone)]
pub struct SpriteAtlas {
    raw_frames: Vec<RawFrame>,
    pub animations: HashMap<String, Vec<SpriteFrame>>,
}

impl SpriteAtlas {
    /// Parse a Sparrow XML atlas string.
    pub fn from_xml(xml_data: &str) -> Self {
        let mut raw_frames = Vec::new();

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

                        raw_frames.push(RawFrame {
                            name,
                            frame: SpriteFrame {
                                src: Rect { x, y, w, h },
                                offset_x: fx,
                                offset_y: fy,
                                frame_w: fw,
                                frame_h: fh,
                                rotated,
                            },
                        });
                    }
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        SpriteAtlas {
            raw_frames,
            animations: HashMap::new(),
        }
    }

    /// Register an animation by prefix matching (like HaxeFlixel's `addByPrefix`).
    /// Finds all SubTexture names that start with `prefix`, sorts by trailing number,
    /// and stores them under `anim_name`.
    pub fn add_by_prefix(&mut self, anim_name: &str, prefix: &str) {
        let mut matched: Vec<(u32, SpriteFrame)> = Vec::new();

        for raw in &self.raw_frames {
            if raw.name.starts_with(prefix) {
                let suffix = &raw.name[prefix.len()..];
                let idx: u32 = suffix.parse().unwrap_or(0);
                matched.push((idx, raw.frame.clone()));
            }
        }

        matched.sort_by_key(|(idx, _)| *idx);
        let frames: Vec<SpriteFrame> = matched.into_iter().map(|(_, f)| f).collect();

        if !frames.is_empty() {
            self.animations.insert(anim_name.to_string(), frames);
        }
    }

    /// Register an animation using specific frame indices.
    /// Matches HaxeFlixel's `addByIndices`: constructs exact frame names
    /// as `prefix + zero_padded(index)` and looks them up by name.
    pub fn add_by_indices(&mut self, anim_name: &str, prefix: &str, indices: &[i32]) {
        // Build a lookup map from raw frame names
        let name_map: HashMap<String, SpriteFrame> = self.raw_frames.iter()
            .map(|r| (r.name.clone(), r.frame.clone()))
            .collect();

        let frames: Vec<SpriteFrame> = indices
            .iter()
            .filter_map(|&i| {
                // HaxeFlixel zero-pads to 4 digits
                let frame_name = format!("{}{:04}", prefix, i);
                name_map.get(&frame_name).cloned()
            })
            .collect();

        if !frames.is_empty() {
            self.animations.insert(anim_name.to_string(), frames);
        }
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

    pub fn anim_names(&self) -> Vec<&str> {
        self.animations.keys().map(|s| s.as_str()).collect()
    }
}

/// Animation controller for a sprite atlas.
pub struct AnimationController {
    pub current_anim: String,
    pub frame_index: usize,
    pub fps: f32,
    pub looping: bool,
    pub finished: bool,
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
            timer: 0.0,
        }
    }

    pub fn play(&mut self, anim: &str, fps: f32, looping: bool) {
        if self.current_anim != anim || self.finished {
            self.current_anim = anim.to_string();
            self.frame_index = 0;
            self.timer = 0.0;
            self.finished = false;
        }
        self.fps = fps;
        self.looping = looping;
    }

    /// Force restart animation even if already playing.
    pub fn force_play(&mut self, anim: &str, fps: f32, looping: bool) {
        self.current_anim = anim.to_string();
        self.frame_index = 0;
        self.timer = 0.0;
        self.finished = false;
        self.fps = fps;
        self.looping = looping;
    }

    pub fn update(&mut self, dt: f32, frame_count: usize) {
        if self.finished || frame_count == 0 || self.fps <= 0.0 {
            return;
        }

        self.timer += dt;
        let frame_duration = 1.0 / self.fps;

        while self.timer >= frame_duration {
            self.timer -= frame_duration;
            self.frame_index += 1;

            if self.frame_index >= frame_count {
                if self.looping {
                    self.frame_index = 0;
                } else {
                    self.frame_index = frame_count - 1;
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
