use serde::Deserialize;

/// A song entry within a week: [name, character, [r, g, b]].
#[derive(Debug, Clone)]
pub struct WeekSong {
    pub name: String,
    pub character: String,
    pub color: [u8; 3],
}

/// Parsed week JSON data.
#[derive(Debug, Clone)]
pub struct WeekData {
    pub songs: Vec<WeekSong>,
    pub week_characters: [String; 3],
    pub week_background: String,
    pub story_name: String,
    pub week_name: String,
    pub week_before: String,
    pub start_unlocked: bool,
    pub hide_story_mode: bool,
    pub hide_freeplay: bool,
    /// File stem used for ordering (e.g. "week1", "week2").
    pub file_name: String,
}

#[derive(Deserialize)]
struct RawWeek {
    songs: Vec<serde_json::Value>,
    #[serde(default, rename = "weekCharacters")]
    week_characters: Vec<String>,
    #[serde(default, rename = "weekBackground")]
    week_background: String,
    #[serde(default, rename = "storyName")]
    story_name: String,
    #[serde(default, rename = "weekName")]
    week_name: String,
    #[serde(default, rename = "weekBefore")]
    week_before: String,
    #[serde(default, rename = "startUnlocked")]
    start_unlocked: bool,
    #[serde(default, rename = "hideStoryMode")]
    hide_story_mode: bool,
    #[serde(default, rename = "hideFreeplay")]
    hide_freeplay: bool,
}

impl WeekData {
    pub fn from_json(json_str: &str, file_name: &str) -> Result<Self, String> {
        let raw: RawWeek = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse week JSON: {}", e))?;

        let songs: Vec<WeekSong> = raw.songs.iter().filter_map(|entry| {
            let arr = entry.as_array()?;
            let name = arr.first()?.as_str()?.to_string();
            let character = arr.get(1)?.as_str().unwrap_or("bf").to_string();
            let color = if let Some(c) = arr.get(2).and_then(|v| v.as_array()) {
                [
                    c.first().and_then(|v| v.as_u64()).unwrap_or(146) as u8,
                    c.get(1).and_then(|v| v.as_u64()).unwrap_or(113) as u8,
                    c.get(2).and_then(|v| v.as_u64()).unwrap_or(253) as u8,
                ]
            } else {
                [146, 113, 253]
            };
            Some(WeekSong { name, character, color })
        }).collect();

        let wc = raw.week_characters;
        let week_characters = [
            wc.first().cloned().unwrap_or_default(),
            wc.get(1).cloned().unwrap_or_default(),
            wc.get(2).cloned().unwrap_or_default(),
        ];

        Ok(WeekData {
            songs,
            week_characters,
            week_background: raw.week_background,
            story_name: raw.story_name,
            week_name: raw.week_name,
            week_before: raw.week_before,
            start_unlocked: raw.start_unlocked,
            hide_story_mode: raw.hide_story_mode,
            hide_freeplay: raw.hide_freeplay,
            file_name: file_name.to_string(),
        })
    }
}

/// Load all weeks from a directory, sorted by filename.
pub fn load_weeks(weeks_dir: &std::path::Path) -> Vec<WeekData> {
    let mut weeks = Vec::new();
    if let Ok(entries) = std::fs::read_dir(weeks_dir) {
        let mut paths: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect();
        paths.sort_by_key(|e| e.file_name());

        for entry in paths {
            let path = entry.path();
            let file_name = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if let Ok(json) = std::fs::read_to_string(&path) {
                match WeekData::from_json(&json, &file_name) {
                    Ok(week) => weeks.push(week),
                    Err(e) => eprintln!("Warning: Failed to parse week {:?}: {}", path, e),
                }
            }
        }
    }
    weeks
}
