use serde::{Deserialize, Serialize};

use crate::note::{EventNote, NoteData, NoteKind};

/// Full Psych Engine chart song format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwagSong {
    pub song: String,
    pub notes: Vec<SwagSection>,
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
    pub bpm: f64,
    #[serde(default = "default_speed")]
    pub speed: f64,
    #[serde(default)]
    pub offset: f64,

    #[serde(default = "default_player1")]
    pub player1: String,
    #[serde(default = "default_player2")]
    pub player2: String,
    #[serde(default = "default_gf")]
    pub gf_version: String,

    #[serde(default = "default_stage")]
    pub stage: String,
    #[serde(default)]
    pub format: String,
    #[serde(default = "default_true")]
    pub needs_voices: bool,

    #[serde(default)]
    pub game_over_char: String,
    #[serde(default)]
    pub game_over_sound: String,
    #[serde(default)]
    pub game_over_loop: String,
    #[serde(default)]
    pub game_over_end: String,

    #[serde(default)]
    pub disable_note_rgb: bool,
    #[serde(default)]
    pub arrow_skin: String,
    #[serde(default)]
    pub splash_skin: String,
}

fn default_speed() -> f64 {
    1.0
}
fn default_player1() -> String {
    "bf".into()
}
fn default_player2() -> String {
    "dad".into()
}
fn default_gf() -> String {
    "gf".into()
}
fn default_stage() -> String {
    "stage".into()
}
fn default_true() -> bool {
    true
}

/// A section in the chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwagSection {
    pub section_notes: Vec<Vec<serde_json::Value>>,
    #[serde(default = "default_section_beats")]
    pub section_beats: f64,
    #[serde(default)]
    pub must_hit_section: bool,
    #[serde(default)]
    pub alt_anim: bool,
    #[serde(default)]
    pub gf_section: bool,
    #[serde(default)]
    pub bpm: f64,
    #[serde(default)]
    pub change_bpm: bool,
}

fn default_section_beats() -> f64 {
    4.0
}

/// Wrapper for the JSON file structure `{ "song": { ... } }`.
#[derive(Debug, Deserialize)]
pub struct ChartFile {
    pub song: serde_json::Value,
}

/// Result of parsing a chart.
pub struct ParsedChart {
    pub song: SwagSong,
    pub notes: Vec<NoteData>,
    pub events: Vec<EventNote>,
}

/// Parse a Psych Engine chart JSON string into notes and events.
pub fn parse_chart(json_data: &str) -> Result<ParsedChart, ChartError> {
    let chart_file: ChartFile =
        serde_json::from_str(json_data).map_err(|e| ChartError::Parse(e.to_string()))?;

    // Handle potential double-nesting: { "song": { "song": { ... } } }
    let song_value = if chart_file.song.get("song").is_some()
        && chart_file.song.get("notes").is_none()
    {
        chart_file.song.get("song").unwrap().clone()
    } else {
        chart_file.song
    };

    let mut song: SwagSong =
        serde_json::from_value(song_value).map_err(|e| ChartError::Parse(e.to_string()))?;

    if !song.format.starts_with("psych_v1") {
        convert_to_psych_v1(&mut song);
    }

    let mut notes = Vec::new();
    let mut events = Vec::new();

    for section in &song.notes {
        for sn in &section.section_notes {
            if sn.len() < 3 {
                continue;
            }

            let strum_time = match sn[0].as_f64() {
                Some(t) => t,
                None => continue,
            };

            let direction = match sn[1].as_f64() {
                Some(d) => d as i32,
                None => continue,
            };

            if direction < 0 {
                if sn.len() >= 4 {
                    let event_name = sn[1].as_str().unwrap_or("").to_string();
                    let v1 = sn.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let v2 = sn.get(3).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    events.push(EventNote::new(strum_time, event_name, v1, v2));
                }
                continue;
            }

            let direction = direction as usize;
            if direction > 7 {
                continue;
            }

            let sustain_length = sn[2].as_f64().unwrap_or(0.0).max(0.0);
            let kind = NoteKind::from_chart_value(sn.get(3));

            // After conversion to psych_v1, directions are absolute:
            // 0-3 = player (must_press), 4-7 = opponent
            let must_press = direction < 4;
            let lane = direction % 4;

            let mut note = NoteData::new(strum_time, lane, sustain_length, must_press, kind);
            note.gf_note = section.gf_section;
            note.alt_note = section.alt_anim;

            notes.push(note);
        }
    }

    parse_events_array(&song.events, &mut events);

    notes.sort_by(|a, b| a.strum_time.partial_cmp(&b.strum_time).unwrap_or(std::cmp::Ordering::Equal));
    events.sort_by(|a, b| a.strum_time.partial_cmp(&b.strum_time).unwrap_or(std::cmp::Ordering::Equal));

    Ok(ParsedChart { song, notes, events })
}

/// Parse a separate events.json file.
pub fn parse_events_file(json_data: &str) -> Result<Vec<EventNote>, ChartError> {
    let parsed: serde_json::Value =
        serde_json::from_str(json_data).map_err(|e| ChartError::Parse(e.to_string()))?;

    let mut events = Vec::new();

    let event_array = parsed
        .pointer("/song/events")
        .or_else(|| parsed.get("events"))
        .and_then(|v| v.as_array());

    if let Some(arr) = event_array {
        let values: Vec<serde_json::Value> = arr.clone();
        parse_events_array(&values, &mut events);
    }

    events.sort_by(|a, b| a.strum_time.partial_cmp(&b.strum_time).unwrap_or(std::cmp::Ordering::Equal));
    Ok(events)
}

fn parse_events_array(arr: &[serde_json::Value], events: &mut Vec<EventNote>) {
    for entry in arr {
        let entry = match entry.as_array() {
            Some(a) => a,
            None => continue,
        };
        if entry.len() < 2 {
            continue;
        }

        let strum_time = match entry[0].as_f64() {
            Some(t) => t,
            None => continue,
        };

        let sub_events = match entry[1].as_array() {
            Some(a) => a,
            None => continue,
        };

        for sub in sub_events {
            let sub = match sub.as_array() {
                Some(a) => a,
                None => continue,
            };
            if sub.len() < 3 {
                continue;
            }

            let name = sub[0].as_str().unwrap_or("").to_string();
            let value1 = sub[1].as_str().unwrap_or("").to_string();
            let value2 = sub[2].as_str().unwrap_or("").to_string();

            events.push(EventNote::new(strum_time, name, value1, value2));
        }
    }
}

fn convert_to_psych_v1(song: &mut SwagSong) {
    // Normalize note directions so 0-3 = player, 4-7 = opponent.
    // Legacy charts store directions relative to mustHitSection:
    //   if mustHitSection: 0-3 = player, 4-7 = opponent
    //   if !mustHitSection: 0-3 = opponent, 4-7 = player
    // We remap so directions are absolute, matching Song.hx lines 113-114:
    //   gottaHitNote = (note[1] < 4) ? mustHitSection : !mustHitSection
    //   note[1] = (note[1] % 4) + (gottaHitNote ? 0 : 4)
    for section in &mut song.notes {
        if section.section_beats <= 0.0 {
            section.section_beats = 4.0;
        }

        for sn in &mut section.section_notes {
            if sn.len() < 3 {
                continue;
            }
            let dir = match sn[1].as_f64() {
                Some(d) => d as i32,
                None => continue,
            };
            if dir < 0 || dir > 7 {
                continue;
            }

            let is_player = if dir < 4 {
                section.must_hit_section
            } else {
                !section.must_hit_section
            };
            let new_dir = (dir % 4) + if is_player { 0 } else { 4 };
            sn[1] = serde_json::Value::from(new_dir);
        }
    }
    song.format = "psych_v1".to_string();
}

#[derive(Debug, thiserror::Error)]
pub enum ChartError {
    #[error("Failed to parse chart: {0}")]
    Parse(String),
    #[error("Chart file not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_chart_json() -> &'static str {
        r#"{
            "song": {
                "song": "Test Song",
                "bpm": 150.0,
                "speed": 2.0,
                "player1": "bf",
                "player2": "dad",
                "gfVersion": "gf",
                "stage": "stage",
                "needsVoices": true,
                "notes": [
                    {
                        "sectionNotes": [
                            [0.0, 0, 0],
                            [500.0, 5, 200.0],
                            [1000.0, 2, 0, "Alt Animation"]
                        ],
                        "mustHitSection": true,
                        "sectionBeats": 4.0
                    }
                ],
                "events": [
                    [2000.0, [["Hey!", "BF", "0.6"]]]
                ]
            }
        }"#
    }

    #[test]
    fn test_parse_chart() {
        let result = parse_chart(sample_chart_json()).unwrap();
        assert_eq!(result.song.song, "Test Song");
        assert!((result.song.bpm - 150.0).abs() < 0.001);
        assert!((result.song.speed - 2.0).abs() < 0.001);
        assert_eq!(result.song.player1, "bf");
    }

    #[test]
    fn test_parse_notes() {
        let result = parse_chart(sample_chart_json()).unwrap();
        assert_eq!(result.notes.len(), 3);

        assert!(result.notes[0].must_press);
        assert_eq!(result.notes[0].lane, 0);
        assert_eq!(result.notes[0].sustain_length, 0.0);

        assert!(!result.notes[1].must_press);
        assert_eq!(result.notes[1].lane, 1);
        assert!((result.notes[1].sustain_length - 200.0).abs() < 0.001);

        assert!(result.notes[2].must_press);
        assert_eq!(result.notes[2].kind, NoteKind::Alt);
    }

    #[test]
    fn test_parse_events() {
        let result = parse_chart(sample_chart_json()).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].name, "Hey!");
        assert_eq!(result.events[0].value1, "BF");
    }

    #[test]
    fn test_must_hit_section_does_not_flip_ownership() {
        // Opponent section (!mustHitSection) with notes at directions 0-3:
        // In legacy format, dir 0-3 in a !mustHit section = opponent notes.
        // After conversion: they should become 4-7 (opponent).
        let json = r#"{
            "song": {
                "song": "Test",
                "bpm": 120.0,
                "notes": [
                    {
                        "sectionNotes": [
                            [0.0, 0, 0],
                            [100.0, 1, 0],
                            [200.0, 6, 0],
                            [300.0, 7, 0]
                        ],
                        "mustHitSection": false,
                        "sectionBeats": 4.0
                    }
                ],
                "events": []
            }
        }"#;
        let result = parse_chart(json).unwrap();
        assert_eq!(result.notes.len(), 4);

        // dir 0 in !mustHit → opponent (must_press = false)
        assert!(!result.notes[0].must_press);
        assert_eq!(result.notes[0].lane, 0);

        // dir 1 in !mustHit → opponent
        assert!(!result.notes[1].must_press);
        assert_eq!(result.notes[1].lane, 1);

        // dir 6 in !mustHit → player (flipped: !mustHit for dir>=4 means player)
        assert!(result.notes[2].must_press);
        assert_eq!(result.notes[2].lane, 2);

        // dir 7 in !mustHit → player
        assert!(result.notes[3].must_press);
        assert_eq!(result.notes[3].lane, 3);
    }

    #[test]
    fn test_psych_v1_skips_conversion() {
        // psych_v1 format: directions are already absolute (0-3=player, 4-7=opponent)
        let json = r#"{
            "song": {
                "song": "Test",
                "bpm": 120.0,
                "format": "psych_v1",
                "notes": [
                    {
                        "sectionNotes": [
                            [0.0, 2, 0],
                            [100.0, 5, 0]
                        ],
                        "mustHitSection": false,
                        "sectionBeats": 4.0
                    }
                ],
                "events": []
            }
        }"#;
        let result = parse_chart(json).unwrap();
        assert_eq!(result.notes.len(), 2);

        // dir 2 = player regardless of mustHitSection
        assert!(result.notes[0].must_press);
        assert_eq!(result.notes[0].lane, 2);

        // dir 5 = opponent regardless of mustHitSection
        assert!(!result.notes[1].must_press);
        assert_eq!(result.notes[1].lane, 1);
    }

    #[test]
    fn test_psych_v1_convert_skips_conversion() {
        // psych_v1_convert: already normalized by Psych Engine, must NOT double-convert
        let json = r#"{
            "song": {
                "song": "Test",
                "bpm": 120.0,
                "format": "psych_v1_convert",
                "notes": [
                    {
                        "sectionNotes": [
                            [0.0, 6, 0],
                            [100.0, 2, 0]
                        ],
                        "mustHitSection": false,
                        "sectionBeats": 4.0
                    }
                ],
                "events": []
            }
        }"#;
        let result = parse_chart(json).unwrap();
        assert_eq!(result.notes.len(), 2);

        // dir 6 = opponent (already normalized, no conversion)
        assert!(!result.notes[0].must_press);
        assert_eq!(result.notes[0].lane, 2);

        // dir 2 = player (already normalized, no conversion)
        assert!(result.notes[1].must_press);
        assert_eq!(result.notes[1].lane, 2);
    }

    #[test]
    fn test_parse_events_file() {
        let events_json = r#"{
            "song": {
                "events": [
                    [1000.0, [["Play Animation", "hey", "BF"]]],
                    [2000.0, [["Add Camera Zoom", "0.04", "0.03"]]]
                ]
            }
        }"#;
        let events = parse_events_file(events_json).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].name, "Play Animation");
        assert_eq!(events[1].name, "Add Camera Zoom");
    }
}
