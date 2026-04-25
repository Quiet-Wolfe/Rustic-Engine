use rustic_core::stage::StageFile;
use rustic_core::{mods::ModLoader, paths::AssetPaths};
use std::fs;
use std::path::PathBuf;

#[test]
fn test_midnight_stage_parse() {
    let json_str =
        fs::read_to_string("../../mods/MidnightMemorabiliaDemo/assets/stages/midnightStage.json")
            .unwrap();
    let stage = StageFile::from_json(&json_str).unwrap();
    println!("Parsed opponent: {:?}", stage.opponent);
    assert_eq!(stage.opponent[0], -100.0);
}

#[test]
fn test_midnight_stage_folder_images_resolve() {
    let loader = ModLoader::discover(
        PathBuf::from("../../assets"),
        PathBuf::from("../../mods/MidnightMemorabiliaDemo"),
    );
    let paths = AssetPaths::from_mod_loader(&loader);
    let images = paths.images_in_dir("midnightStage");
    println!("midnightStage images: {:?}", images);
    assert!(images.contains(&"midnightStage/back".to_string()));
    assert!(images.contains(&"midnightStage/throne".to_string()));
    assert!(paths.stage_image("midnightStage/back", "").is_some());
    assert!(paths.stage_image("midnightStage/throne", "").is_some());
    assert!(paths.chart("avarice", "hard").is_some());
    assert!(paths.stage_json("midnightStage").is_some());
}
