use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Character definition loaded from `characters/{name}.json`.
/// Matches Psych Engine's CharacterFile typedef.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterFile {
    pub animations: Vec<AnimArray>,
    #[serde(default)]
    pub image: String,
    /// Multi-atlas character: maps sheet names to image paths (e.g. "baseSheet" → "characters/BOYFRIEND").
    /// Used when `image` is empty — the "baseSheet" entry is the primary atlas.
    #[serde(default)]
    pub images: HashMap<String, String>,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default = "default_sing_duration")]
    pub sing_duration: f64,
    #[serde(default)]
    pub healthicon: String,

    #[serde(default)]
    pub position: [f64; 2],
    #[serde(default)]
    pub camera_position: [f64; 2],

    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub no_antialiasing: bool,
    #[serde(default = "default_healthbar_colors")]
    pub healthbar_colors: [u8; 3],
    #[serde(default)]
    pub vocals_file: String,

    /// Custom note skin (e.g. "notes/Notes_Ace"). Overrides default NOTE_assets.
    #[serde(default)]
    pub skin: String,
    /// Custom health bar image (e.g. "dozirc"). References healthBars/{name}/bar.png.
    #[serde(default, rename = "healthBarImg")]
    pub health_bar_img: String,

    /// Per-stage scale overrides (e.g. "nightflaid" → 0.774).
    #[serde(default)]
    pub stage_scale: HashMap<String, f64>,
    /// Per-stage camera offset overrides.
    #[serde(default)]
    pub stage_camera: HashMap<String, [f64; 2]>,
}

fn default_scale() -> f64 {
    1.0
}
fn default_sing_duration() -> f64 {
    4.0
}
fn default_healthbar_colors() -> [u8; 3] {
    [161, 161, 161]
}

/// An animation entry in a character's JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimArray {
    pub anim: String,
    pub name: String,
    #[serde(default = "default_fps")]
    pub fps: i32,
    /// Psych Engine JSON uses "loop" — reserved word in Rust.
    #[serde(rename = "loop", default)]
    pub loop_anim: bool,
    #[serde(default)]
    pub indices: Vec<i32>,
    #[serde(default)]
    pub offsets: [f64; 2],
}

fn default_fps() -> i32 {
    24
}

pub const SING_DIRECTIONS: [&str; 4] = ["singLEFT", "singDOWN", "singUP", "singRIGHT"];
pub const MISS_DIRECTIONS: [&str; 4] = [
    "singLEFTmiss",
    "singDOWNmiss",
    "singUPmiss",
    "singRIGHTmiss",
];

impl CharacterFile {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Get the effective image path. If `image` is empty, falls back to `images["baseSheet"]`.
    pub fn effective_image(&self) -> &str {
        if !self.image.is_empty() {
            return &self.image;
        }
        if let Some(base) = self.images.get("baseSheet") {
            return base;
        }
        ""
    }

    pub fn find_anim(&self, anim_name: &str) -> Option<&AnimArray> {
        self.animations.iter().find(|a| a.anim == anim_name)
    }

    pub fn sing_anim_for_lane(&self, lane: usize) -> &'static str {
        SING_DIRECTIONS.get(lane).copied().unwrap_or("singLEFT")
    }

    pub fn miss_anim_for_lane(&self, lane: usize) -> &'static str {
        MISS_DIRECTIONS.get(lane).copied().unwrap_or("singLEFTmiss")
    }

    /// Whether this character uses danceLeft/danceRight instead of idle.
    pub fn has_dance_idle(&self) -> bool {
        self.animations.iter().any(|a| a.anim == "danceLeft")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_character() {
        let json = r#"{
            "animations": [
                {
                    "anim": "idle",
                    "name": "BF idle dance",
                    "fps": 24,
                    "loop": false,
                    "indices": [],
                    "offsets": [-5, 0]
                },
                {
                    "anim": "singLEFT",
                    "name": "BF NOTE LEFT0",
                    "fps": 24,
                    "loop": false,
                    "indices": [],
                    "offsets": [5, -6]
                }
            ],
            "image": "characters/BOYFRIEND",
            "scale": 1,
            "sing_duration": 4,
            "healthicon": "bf",
            "position": [0, 350],
            "camera_position": [0, 0],
            "flip_x": true,
            "no_antialiasing": false,
            "healthbar_colors": [49, 176, 209]
        }"#;

        let char = CharacterFile::from_json(json).unwrap();
        assert_eq!(char.image, "characters/BOYFRIEND");
        assert_eq!(char.animations.len(), 2);
        assert_eq!(char.animations[0].anim, "idle");
        assert!(!char.animations[0].loop_anim);
        assert!(char.flip_x);
        assert_eq!(char.healthbar_colors, [49, 176, 209]);
    }

    #[test]
    fn test_sing_anim_lookup() {
        assert_eq!(SING_DIRECTIONS[0], "singLEFT");
        assert_eq!(SING_DIRECTIONS[1], "singDOWN");
        assert_eq!(SING_DIRECTIONS[2], "singUP");
        assert_eq!(SING_DIRECTIONS[3], "singRIGHT");
    }
}
