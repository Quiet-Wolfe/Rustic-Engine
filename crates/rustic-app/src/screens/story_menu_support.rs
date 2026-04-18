use std::collections::HashSet;
use std::fs;

use rustic_core::highscore::HighscoreStore;
use rustic_core::paths::AssetPaths;
use rustic_core::week::{self, WeekData};
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::{AnimationController, SpriteAtlas};

pub const STORY_DIFFICULTIES: [&str; 3] = ["easy", "normal", "hard"];

#[derive(Debug, Clone)]
pub struct StoryWeekEntry {
    pub week: WeekData,
    pub locked: bool,
}

#[derive(Debug, serde::Deserialize)]
struct RawMenuCharacter {
    image: String,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    position: Vec<i32>,
    #[serde(default, rename = "idle_anim")]
    idle_anim: String,
    #[serde(default, rename = "confirm_anim")]
    confirm_anim: String,
    #[serde(default, rename = "flipX")]
    flip_x: bool,
}

pub struct StoryMenuCharacter {
    texture: GpuTexture,
    atlas: SpriteAtlas,
    tex_w: f32,
    tex_h: f32,
    anim: AnimationController,
    x: f32,
    y: f32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    flip_x: bool,
    tint: [f32; 4],
    has_confirm_animation: bool,
}

impl StoryMenuCharacter {
    pub fn load(gpu: &GpuState, paths: &AssetPaths, slot: usize, name: &str) -> Option<Self> {
        if name.is_empty() {
            return None;
        }

        let base_x = 170.0 + slot as f32 * 320.0;
        let base_y = 126.0;
        let (character_name, tint) = if paths
            .find(&format!("images/menucharacters/{name}.json"))
            .is_some()
        {
            (name.to_string(), [1.0, 1.0, 1.0, 1.0])
        } else {
            ("bf".to_string(), [0.0, 0.0, 0.0, 0.6])
        };

        let json_path = paths.find(&format!("images/menucharacters/{}.json", character_name))?;
        let json = fs::read_to_string(json_path).ok()?;
        let raw: RawMenuCharacter = serde_json::from_str(&json).ok()?;

        let png_path = paths.find(&format!("images/menucharacters/{}.png", raw.image))?;
        let xml_path = paths.find(&format!("images/menucharacters/{}.xml", raw.image))?;
        let xml = fs::read_to_string(xml_path).ok()?;

        let texture = gpu.load_texture_from_path(&png_path);
        let mut atlas = SpriteAtlas::from_xml(&xml);
        atlas.add_by_prefix("idle", &raw.idle_anim);

        let mut has_confirm_animation = false;
        if !raw.confirm_anim.is_empty() && raw.confirm_anim != raw.idle_anim {
            atlas.add_by_prefix("confirm", &raw.confirm_anim);
            has_confirm_animation = atlas.has_anim("confirm");
        }

        let mut anim = AnimationController::new();
        anim.play("idle", 24.0, true);

        Some(Self {
            tex_w: texture.width as f32,
            tex_h: texture.height as f32,
            texture,
            atlas,
            anim,
            x: base_x,
            y: base_y,
            scale: raw.scale.unwrap_or(1.0),
            offset_x: raw.position.first().copied().unwrap_or_default() as f32,
            offset_y: raw.position.get(1).copied().unwrap_or_default() as f32,
            flip_x: raw.flip_x,
            tint,
            has_confirm_animation,
        })
    }

    pub fn play_idle(&mut self) {
        self.anim.play("idle", 24.0, true);
    }

    pub fn play_confirm(&mut self) {
        if self.has_confirm_animation {
            self.anim.force_play("confirm", 24.0, false);
        }
    }

    pub fn update(&mut self, dt: f32) {
        let count = self.atlas.frame_count(&self.anim.current_anim);
        self.anim.update(dt, count);
        if self.anim.finished && self.anim.current_anim == "confirm" {
            self.play_idle();
        }
    }

    pub fn draw(&self, gpu: &mut GpuState) {
        let Some(frame) = self
            .atlas
            .get_frame(&self.anim.current_anim, self.anim.frame_index)
        else {
            return;
        };
        gpu.draw_sprite_frame(
            frame,
            self.tex_w,
            self.tex_h,
            self.x - self.offset_x,
            self.y - self.offset_y,
            self.scale,
            self.flip_x,
            self.tint,
        );
        gpu.draw_batch(Some(&self.texture));
    }
}

pub fn song_id(name: &str) -> String {
    name.trim().to_lowercase().replace(' ', "-")
}

pub fn load_story_weeks(paths: &AssetPaths, highscores: &HighscoreStore) -> Vec<StoryWeekEntry> {
    let mut weeks = Vec::new();
    let mut seen = HashSet::new();

    for weeks_dir in paths.all_weeks_dirs() {
        for week in week::load_weeks(&weeks_dir) {
            if week.hide_story_mode || !seen.insert(week.file_name.clone()) {
                continue;
            }
            let locked = is_week_locked(&week, highscores);
            if locked && week.hidden_until_unlocked {
                continue;
            }
            weeks.push(StoryWeekEntry { week, locked });
        }
    }

    weeks.sort_by(|a, b| a.week.file_name.cmp(&b.week.file_name));
    weeks
}

pub fn week_is_completed(week_name: &str, highscores: &HighscoreStore) -> bool {
    STORY_DIFFICULTIES
        .iter()
        .any(|difficulty| highscores.get_week_score(week_name, difficulty) > 0)
}

pub fn is_week_locked(week: &WeekData, highscores: &HighscoreStore) -> bool {
    !week.start_unlocked
        && !week.week_before.is_empty()
        && !week_is_completed(&week.week_before, highscores)
}

pub fn available_difficulties(paths: &AssetPaths, week: &WeekData) -> Vec<String> {
    let mut difficulties = Vec::new();
    for difficulty in STORY_DIFFICULTIES {
        let all_songs_have_chart = week
            .songs
            .iter()
            .all(|song| paths.chart(&song_id(&song.name), difficulty).is_some());
        if all_songs_have_chart {
            difficulties.push(difficulty.to_string());
        }
    }
    if difficulties.is_empty() {
        difficulties.push("normal".to_string());
    }
    difficulties
}
