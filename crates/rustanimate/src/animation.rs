use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnimAtlas {
    #[serde(rename = "AN")]
    pub an: Animation,
    #[serde(rename = "SD")]
    pub sd: Option<SymbolDictionary>,
    #[serde(rename = "MD")]
    pub md: Option<MetaData>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SymbolDictionary {
    #[serde(rename = "S")]
    pub s: Vec<SymbolData>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Animation {
    #[serde(rename = "N")]
    pub n: String,
    #[serde(rename = "STI")]
    pub sti: Option<StageInstance>,
    #[serde(rename = "SN")]
    pub sn: Option<String>,
    #[serde(rename = "TL")]
    pub tl: Option<Timeline>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StageInstance {
    #[serde(rename = "SI")]
    pub si: SymbolInstance,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SymbolData {
    #[serde(rename = "SN")]
    pub sn: String,
    #[serde(rename = "TL")]
    pub tl: Timeline,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Timeline {
    #[serde(rename = "L")]
    pub l: Vec<Layer>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Layer {
    #[serde(rename = "LN")]
    pub ln: String,
    #[serde(rename = "LT")]
    pub lt: Option<String>,
    #[serde(rename = "Clpb")]
    pub clpb: Option<String>,
    #[serde(rename = "FR")]
    pub fr: Vec<Frame>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MetaData {
    #[serde(rename = "FRT")]
    pub frt: Option<f32>,
    #[serde(rename = "V")]
    pub v: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Frame {
    #[serde(rename = "N")]
    pub n: Option<String>,
    #[serde(rename = "I")]
    pub i: u32,
    #[serde(rename = "DU")]
    pub du: u32,
    #[serde(rename = "E")]
    pub e: Vec<Element>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Element {
    #[serde(rename = "SI")]
    pub si: Option<SymbolInstance>,
    #[serde(rename = "ASI")]
    pub asi: Option<AtlasSymbolInstance>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SymbolInstance {
    #[serde(rename = "SN")]
    pub sn: String,
    #[serde(rename = "IN")]
    pub in_name: Option<String>,
    #[serde(rename = "ST")]
    pub st: Option<String>,
    #[serde(rename = "FF")]
    pub ff: Option<u32>,
    #[serde(rename = "LP")]
    pub lp: Option<String>,
    #[serde(rename = "TRP")]
    pub trp: Option<TransformationPoint>,
    #[serde(rename = "M3D")]
    pub m3d: Option<Vec<f32>>,
    #[serde(rename = "MX")]
    pub mx: Option<Vec<f32>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AtlasSymbolInstance {
    #[serde(rename = "N")]
    pub n: String,
    #[serde(rename = "M3D")]
    pub m3d: Option<Vec<f32>>,
    #[serde(rename = "MX")]
    pub mx: Option<Vec<f32>>,
    #[serde(rename = "POS")]
    pub pos: Option<TransformationPoint>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransformationPoint {
    pub x: f32,
    pub y: f32,
}
