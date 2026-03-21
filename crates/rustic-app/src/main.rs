use macroquad::prelude::*;
use rustic_core::character::CharacterFile;
use rustic_core::paths::AssetPaths;
use rustic_render::camera::GameCamera;
use rustic_render::sprites::{AnimationController, SpriteAtlas, draw_sprite_frame};
use std::path::PathBuf;

const SCREEN_W: f32 = 1280.0;
const SCREEN_H: f32 = 720.0;

fn window_conf() -> Conf {
    Conf {
        window_title: "RusticV2 — Phase 1 Test".to_string(),
        window_width: SCREEN_W as i32,
        window_height: SCREEN_H as i32,
        window_resizable: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Point to Psych Engine reference assets for testing
    let assets = AssetPaths::new(PathBuf::from("references/FNF-PsychEngine/assets/shared"));

    // Load BF character definition
    let char_path = assets.character("bf");
    let char_json = std::fs::read_to_string(&char_path).unwrap_or_else(|e| {
        panic!("Failed to read character file {:?}: {}", char_path, e);
    });
    let bf_char = CharacterFile::from_json(&char_json).expect("Failed to parse bf.json");

    // Load BF atlas
    let img_path = assets.image(&bf_char.image);
    let xml_path = assets.xml(&bf_char.image);

    let texture = load_texture(img_path.to_str().unwrap()).await.unwrap();
    texture.set_filter(FilterMode::Linear);

    let xml_data = std::fs::read_to_string(&xml_path).unwrap_or_else(|e| {
        panic!("Failed to read atlas XML {:?}: {}", xml_path, e);
    });
    let atlas = SpriteAtlas::from_xml(&xml_data);

    // Collect available animations from the character def
    let anim_ids: Vec<String> = bf_char.animations.iter().map(|a| a.anim.clone()).collect();
    let mut current_anim_idx = 0;

    let mut anim_ctrl = AnimationController::new();
    let first_anim = &bf_char.animations[0];
    anim_ctrl.play(&first_anim.name, first_anim.fps as f32, first_anim.loop_anim);

    // Camera
    let mut camera = GameCamera::new(0.9);
    camera.snap_to(SCREEN_W / 2.0, SCREEN_H / 2.0);

    let mut show_info = true;

    loop {
        let dt = get_frame_time();

        // Input: cycle animations with left/right arrows
        if is_key_pressed(KeyCode::Right) {
            current_anim_idx = (current_anim_idx + 1) % bf_char.animations.len();
            let anim = &bf_char.animations[current_anim_idx];
            anim_ctrl.play(&anim.name, anim.fps as f32, anim.loop_anim);
        }
        if is_key_pressed(KeyCode::Left) {
            current_anim_idx = if current_anim_idx == 0 {
                bf_char.animations.len() - 1
            } else {
                current_anim_idx - 1
            };
            let anim = &bf_char.animations[current_anim_idx];
            anim_ctrl.play(&anim.name, anim.fps as f32, anim.loop_anim);
        }

        // Replay current animation
        if is_key_pressed(KeyCode::Space) {
            let anim = &bf_char.animations[current_anim_idx];
            anim_ctrl.play("", 0.0, false); // force reset
            anim_ctrl.play(&anim.name, anim.fps as f32, anim.loop_anim);
        }

        // Toggle info
        if is_key_pressed(KeyCode::F1) {
            show_info = !show_info;
        }

        // Camera zoom
        if is_key_down(KeyCode::Equal) {
            camera.target_zoom += 0.5 * dt;
        }
        if is_key_down(KeyCode::Minus) {
            camera.target_zoom -= 0.5 * dt;
        }

        // Camera zoom bump on B
        if is_key_pressed(KeyCode::B) {
            camera.bump_zoom(0.05);
        }

        camera.update(dt);

        // Update animation
        let current_anim_name = &bf_char.animations[current_anim_idx].name;
        let frame_count = atlas.frame_count(current_anim_name);
        anim_ctrl.update(dt, frame_count);

        // Draw
        clear_background(BLACK);

        // Apply game camera transform
        let cam_offset_x = SCREEN_W / 2.0 - camera.x * camera.zoom;
        let cam_offset_y = SCREEN_H / 2.0 - camera.y * camera.zoom;

        // Draw character at center of screen
        let char_x = SCREEN_W / 2.0 - 100.0;
        let char_y = SCREEN_H / 2.0 - 200.0;
        let draw_x = char_x * camera.zoom + cam_offset_x;
        let draw_y = char_y * camera.zoom + cam_offset_y;

        let anim_offsets = bf_char.animations[current_anim_idx].offsets;
        let scale = bf_char.scale as f32 * camera.zoom;

        if let Some(frame) = atlas.get_frame(current_anim_name, anim_ctrl.frame_index) {
            draw_sprite_frame(
                &texture,
                frame,
                draw_x - anim_offsets[0] as f32 * scale,
                draw_y - anim_offsets[1] as f32 * scale,
                scale,
                bf_char.flip_x,
                WHITE,
            );
        }

        // Info overlay
        if show_info {
            let info_lines = vec![
                format!("FPS: {}", get_fps()),
                format!("Animation: {} ({})", anim_ids[current_anim_idx], current_anim_name),
                format!("Frame: {} / {}", anim_ctrl.frame_index + 1, frame_count),
                format!("Camera zoom: {:.2}", camera.zoom),
                format!("Atlas animations: {}", atlas.animations.len()),
                String::new(),
                "Left/Right: cycle animations".into(),
                "Space: replay | B: bump zoom".into(),
                "+/-: zoom | F1: toggle info".into(),
            ];

            for (i, line) in info_lines.iter().enumerate() {
                draw_text(line, 10.0, 20.0 + i as f32 * 18.0, 16.0, WHITE);
            }
        }

        next_frame().await;
    }
}
