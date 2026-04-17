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

    let found = mgr
        .state
        .property_writes
        .iter()
        .any(|(k, v)| k == "boyfriend.x" && matches!(v, LuaValue::Float(f) if (*f - 42.5).abs() < 1e-6));
    assert!(
        found,
        "expected boyfriend.x=42.5 in property_writes, got {:?}",
        mgr.state.property_writes
    );
}
