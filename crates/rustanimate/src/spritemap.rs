use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnimateAtlas {
    #[serde(rename = "ATLAS")]
    pub atlas: AnimateSprites,
    pub meta: Meta,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnimateSprites {
    #[serde(rename = "SPRITES")]
    pub sprites: Vec<AnimateSprite>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnimateSprite {
    #[serde(rename = "SPRITE")]
    pub sprite: AnimateSpriteData,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnimateSpriteData {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rotated: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Meta {
    pub app: String,
    pub version: String,
    pub image: String,
    pub format: String,
    pub size: Size,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}
