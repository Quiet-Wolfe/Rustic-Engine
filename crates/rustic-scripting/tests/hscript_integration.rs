//! End-to-end smoke test: `.hx` files flow through ScriptManager the same
//! way Lua scripts do. We write a temp .hx file, load it via
//! `ScriptManager::load_script`, fire a callback, and check that the script
//! was actually reached (by observing the effect it has on `custom_vars`).

use std::io::Write;

use rustic_scripting::{LuaValue, ScriptManager};

fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("rustic-hscript-tests");
    std::fs::create_dir_all(&dir).expect("mk tmp dir");
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).expect("create tmp");
    f.write_all(body.as_bytes()).expect("write tmp");
    path
}

#[test]
fn hx_extension_dispatches_to_hscript_and_runs_callback() {
    let src = r#"
        function onUpdate(elapsed) {
            hitCount = (hitCount == null ? 0 : hitCount) + 1;
            lastDt = elapsed;
        }
    "#;
    let path = write_tmp("smoke_onupdate.hx", src);

    let mut mgr = ScriptManager::new();
    mgr.load_script(&path);
    assert!(mgr.has_scripts(), "script should have loaded");

    mgr.call_with_elapsed("onUpdate", 0.016);
    mgr.call_with_elapsed("onUpdate", 0.016);

    let hits = mgr.state.custom_vars.get("hitCount").cloned();
    match hits {
        Some(LuaValue::Int(n)) => assert_eq!(n, 2, "expected 2 updates, got {n}"),
        Some(LuaValue::Float(n)) => assert_eq!(n as i64, 2, "expected 2 updates, got {n}"),
        other => panic!("expected hitCount to be numeric, got {other:?}"),
    }
}

#[test]
fn hscript_can_write_property_paths() {
    // Names containing '.' are routed to the property_writes queue, same as
    // Lua's setProperty bridge.
    let src = r#"
        function onCreate() {
            boyfriend.x = 42.5;
        }
    "#;
    let path = write_tmp("smoke_property.hx", src);

    let mut mgr = ScriptManager::new();
    mgr.load_script(&path);
    mgr.call("onCreate");

    let found = mgr.state.property_writes.iter().any(|(k, v)| {
        k == "boyfriend.x" && matches!(v, LuaValue::Float(f) if (*f - 42.5).abs() < 1e-6)
    });
    assert!(
        found,
        "expected boyfriend.x=42.5 in property_writes, got {:?}",
        mgr.state.property_writes
    );
}

#[test]
fn hscript_psych_globals_queue_engine_state() {
    let src = r#"
        function onCreate() {
            setProperty("defaultCamZoom", 1.35);
            triggerEvent("Camera Follow Pos", "120", "240");
            cameraShake("camGame", 0.01, 0.25);
            playSound("confirmMenu", 0.7, "confirm");
            FlxG.sound.playMusic(Paths.music("breakfast"), 0.5, true);
        }
    "#;
    let path = write_tmp("psych_globals.hx", src);

    let mut mgr = ScriptManager::new();
    mgr.load_script(&path);
    mgr.call("onCreate");

    assert!((mgr.state.default_cam_zoom - 1.35).abs() < 0.001);
    assert_eq!(
        mgr.state.triggered_events,
        vec![(
            "Camera Follow Pos".to_string(),
            "120".to_string(),
            "240".to_string()
        )]
    );
    assert_eq!(mgr.state.camera_shake_requests.len(), 1);
    assert_eq!(mgr.state.audio_requests.len(), 2);
}

#[test]
fn hscript_flxsprite_objects_become_lua_sprites() {
    let src = r#"
        var bg;
        function onCreate() {
            bg = new FlxSprite(-50, 25);
            bg.loadGraphic(Paths.image("stages/test/bg"));
            bg.scrollFactor.set(0.8, 0.9);
            bg.scale.set(2, 3);
            bg.alpha = 0.5;
            add(bg);
            bg.x = 100;
        }
    "#;
    let path = write_tmp("flxsprite_stage.hx", src);

    let mut mgr = ScriptManager::new();
    mgr.load_script(&path);
    mgr.call("onCreate");

    assert_eq!(mgr.state.lua_sprites.len(), 1);
    let sprite = mgr.state.lua_sprites.values().next().expect("sprite");
    assert_eq!(sprite.x, 100.0);
    assert_eq!(sprite.y, 25.0);
    assert_eq!(sprite.scroll_x, 0.8);
    assert_eq!(sprite.scroll_y, 0.9);
    assert_eq!(sprite.scale_x, 2.0);
    assert_eq!(sprite.scale_y, 3.0);
    assert_eq!(sprite.alpha, 0.5);
    assert_eq!(mgr.state.sprites_to_add.len(), 1);
}

#[test]
fn hscript_common_haxe_classes_are_backed() {
    let src = r#"
        function onCreate() {
            setVar("rounded", Math.round(1.6));
            setVar("parsed", Std.parseInt("42"));
            setVar("starts", StringTools.startsWith("psych", "psy"));
            var obj = { name: "bf" };
            Reflect.setField(obj, "x", 123);
            setVar("reflectX", Reflect.field(obj, "x"));
        }
    "#;
    let path = write_tmp("haxe_classes.hx", src);

    let mut mgr = ScriptManager::new();
    mgr.load_script(&path);
    mgr.call("onCreate");

    assert!(matches!(
        mgr.state.custom_vars.get("rounded"),
        Some(LuaValue::Int(2))
    ));
    assert!(matches!(
        mgr.state.custom_vars.get("parsed"),
        Some(LuaValue::Int(42))
    ));
    assert!(matches!(
        mgr.state.custom_vars.get("starts"),
        Some(LuaValue::Bool(true))
    ));
    assert!(matches!(
        mgr.state.custom_vars.get("reflectX"),
        Some(LuaValue::Int(123))
    ));
}

#[test]
fn hscript_atlas_animation_and_camera_assignment_survive_add() {
    let src = r#"
        var spr;
        function onCreate() {
            spr = new FlxSprite(10, 20);
            spr.frames = Paths.getSparrowAtlas("characters/test");
            spr.animation.addByPrefix("idle", "idle prefix", 12, true);
            spr.animation.play("idle");
            spr.cameras = [game.camHUD];
            add(spr);
        }
    "#;
    let path = write_tmp("atlas_sprite.hx", src);

    let mut mgr = ScriptManager::new();
    mgr.load_script(&path);
    mgr.call("onCreate");

    let sprite = mgr.state.lua_sprites.values().next().expect("sprite");
    assert_eq!(sprite.camera, "camHUD");
    assert_eq!(sprite.current_anim, "idle");
    assert!(sprite.animations.contains_key("idle"));
}
