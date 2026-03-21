use serde::{Deserialize, Serialize};

/// Stage definition loaded from `stages/{name}.json`.
/// Matches Psych Engine's StageData format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageFile {
    #[serde(default)]
    pub directory: String,
    #[serde(default = "default_zoom", rename = "defaultZoom")]
    pub default_zoom: f64,
    #[serde(default, rename = "isPixelStage")]
    pub is_pixel_stage: bool,

    #[serde(default = "default_bf_pos")]
    pub boyfriend: [f64; 2],
    #[serde(default = "default_gf_pos")]
    pub girlfriend: [f64; 2],
    #[serde(default = "default_dad_pos")]
    pub opponent: [f64; 2],

    #[serde(default)]
    pub hide_girlfriend: bool,

    #[serde(default)]
    pub camera_boyfriend: [f64; 2],
    #[serde(default)]
    pub camera_opponent: [f64; 2],
    #[serde(default)]
    pub camera_girlfriend: [f64; 2],

    #[serde(default = "default_one")]
    pub camera_speed: f64,

    #[serde(default)]
    pub objects: Vec<StageObjectDef>,
}

fn default_zoom() -> f64 {
    0.9
}
fn default_bf_pos() -> [f64; 2] {
    [770.0, 100.0]
}
fn default_gf_pos() -> [f64; 2] {
    [400.0, 130.0]
}
fn default_dad_pos() -> [f64; 2] {
    [100.0, 100.0]
}
fn default_one() -> f64 {
    1.0
}
fn default_scale() -> [f64; 2] {
    [1.0, 1.0]
}
fn default_scroll() -> [f64; 2] {
    [1.0, 1.0]
}
fn default_alpha() -> f64 {
    1.0
}
fn default_color() -> String {
    "#FFFFFF".into()
}

/// A visual object in a stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageObjectDef {
    #[serde(default, rename = "type")]
    pub obj_type: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default = "default_scale")]
    pub scale: [f64; 2],
    #[serde(default = "default_scroll")]
    pub scroll: [f64; 2],
    #[serde(default = "default_alpha")]
    pub alpha: f64,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default)]
    pub angle: f64,
    #[serde(default, rename = "flipX")]
    pub flip_x: bool,
    #[serde(default, rename = "flipY")]
    pub flip_y: bool,
    #[serde(default = "default_true")]
    pub antialiasing: bool,
    #[serde(default)]
    pub animations: Vec<StageAnimDef>,
    #[serde(default, rename = "firstAnimation")]
    pub first_animation: String,
}

fn default_true() -> bool {
    true
}

/// Animation definition for stage sprites.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageAnimDef {
    #[serde(default)]
    pub anim: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(rename = "loop", default)]
    pub loop_anim: bool,
    #[serde(default)]
    pub offsets: [f64; 2],
    #[serde(default)]
    pub indices: Vec<u32>,
}

fn default_fps() -> u32 {
    24
}

impl StageFile {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn default_stage() -> Self {
        Self {
            directory: String::new(),
            default_zoom: 0.9,
            is_pixel_stage: false,
            boyfriend: [770.0, 100.0],
            girlfriend: [400.0, 130.0],
            opponent: [100.0, 100.0],
            hide_girlfriend: false,
            camera_boyfriend: [0.0, 0.0],
            camera_opponent: [0.0, 0.0],
            camera_girlfriend: [0.0, 0.0],
            camera_speed: 1.0,
            objects: Vec::new(),
        }
    }
}

/// Parse a hex color string like "#FF00FF" or "FF00FF" into [R, G, B].
pub fn parse_hex_color(s: &str) -> [u8; 3] {
    let s = s.trim_start_matches('#');
    if s.len() >= 6 {
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(255);
        [r, g, b]
    } else {
        [255, 255, 255]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stage() {
        let json = r#"{
            "directory": "",
            "defaultZoom": 0.9,
            "boyfriend": [770, 100],
            "girlfriend": [400, 130],
            "opponent": [100, 100],
            "hide_girlfriend": false,
            "camera_boyfriend": [0, 0],
            "camera_opponent": [0, 0],
            "camera_girlfriend": [0, 0],
            "camera_speed": 1
        }"#;
        let stage = StageFile::from_json(json).unwrap();
        assert!((stage.default_zoom - 0.9).abs() < 0.001);
        assert_eq!(stage.boyfriend, [770.0, 100.0]);
    }

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#FF0000"), [255, 0, 0]);
        assert_eq!(parse_hex_color("00FF00"), [0, 255, 0]);
        assert_eq!(parse_hex_color("#FFFFFF"), [255, 255, 255]);
    }
}
