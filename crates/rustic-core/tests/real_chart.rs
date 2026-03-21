use rustic_core::chart::parse_chart;
use rustic_core::character::CharacterFile;
use rustic_core::stage::StageFile;

#[test]
fn test_parse_real_bopeebo_chart() {
    let path = "references/FNF-PsychEngine/assets/base_game/shared/data/bopeebo/bopeebo.json";
    let json = match std::fs::read_to_string(path) {
        Ok(j) => j,
        Err(_) => {
            eprintln!("Skipping real chart test: {path} not found");
            return;
        }
    };

    let result = parse_chart(&json).expect("Failed to parse bopeebo chart");
    assert_eq!(result.song.song.to_lowercase(), "bopeebo");
    assert!(result.song.bpm > 0.0);
    assert!(!result.notes.is_empty(), "Should have parsed some notes");
    assert!(result.song.speed > 0.0);

    // Verify notes are sorted by time
    for pair in result.notes.windows(2) {
        assert!(pair[0].strum_time <= pair[1].strum_time);
    }

    // Verify all lanes are 0-3
    for note in &result.notes {
        assert!(note.lane < 4, "Lane {} out of range", note.lane);
    }
}

#[test]
fn test_parse_real_blammed_chart() {
    let path = "references/FNF-PsychEngine/assets/base_game/shared/data/blammed/blammed-hard.json";
    let json = match std::fs::read_to_string(path) {
        Ok(j) => j,
        Err(_) => {
            eprintln!("Skipping real chart test: {path} not found");
            return;
        }
    };

    let result = parse_chart(&json).expect("Failed to parse blammed-hard chart");
    assert!(!result.notes.is_empty());

    // Count player and opponent notes
    let player_notes = result.notes.iter().filter(|n| n.must_press).count();
    let opponent_notes = result.notes.iter().filter(|n| !n.must_press).count();
    assert!(player_notes > 0, "Should have player notes");
    assert!(opponent_notes > 0, "Should have opponent notes");
}

#[test]
fn test_parse_real_bf_character() {
    let path = "references/FNF-PsychEngine/assets/shared/characters/bf.json";
    let json = match std::fs::read_to_string(path) {
        Ok(j) => j,
        Err(_) => {
            eprintln!("Skipping real character test: {path} not found");
            return;
        }
    };

    let char_file = CharacterFile::from_json(&json).expect("Failed to parse bf.json");
    assert_eq!(char_file.image, "characters/BOYFRIEND");
    assert!(char_file.flip_x);
    assert_eq!(char_file.healthicon, "bf");
    assert!(char_file.find_anim("idle").is_some());
    assert!(char_file.find_anim("singLEFT").is_some());
    assert!(char_file.find_anim("singDOWN").is_some());
    assert!(char_file.find_anim("singUP").is_some());
    assert!(char_file.find_anim("singRIGHT").is_some());
}

#[test]
fn test_parse_real_stage() {
    let path = "references/FNF-PsychEngine/assets/shared/stages/stage.json";
    let json = match std::fs::read_to_string(path) {
        Ok(j) => j,
        Err(_) => {
            eprintln!("Skipping real stage test: {path} not found");
            return;
        }
    };

    let stage = StageFile::from_json(&json).expect("Failed to parse stage.json");
    assert!(stage.default_zoom > 0.0);
}
