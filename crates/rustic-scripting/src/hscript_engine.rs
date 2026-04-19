//! HScript engine — a thin wrapper that gives .hx scripts access to the same
//! `ScriptState` as Lua scripts do.
//!
//! Psych mods lean on HScript for logic that's awkward in Lua (complex
//! data structures, proper closures). We don't need to match haxe semantics
//! exactly — just cover the subset Psych mods actually use. The evaluator
//! itself lives in `rustic-hscript`; this file only deals with the bridge
//! between our ScriptState and that interpreter's HostBridge trait.

use std::path::{Path, PathBuf};

use rustic_hscript::{HostBridge, Interp, Value as HValue};

use crate::script_state::{LuaValue, ScriptState};

/// A single loaded HScript file.
pub struct HScriptEngine {
    interp: Interp,
    source_path: PathBuf,
    name: String,
}

impl HScriptEngine {
    /// Load an HScript file from disk. Executes its top-level (installing
    /// function/var definitions into globals) but does not call `onCreate` —
    /// the embedder does that after all scripts are loaded, same as Lua.
    pub fn load(path: &Path, state: &mut ScriptState) -> Result<Self, String> {
        let source =
            std::fs::read_to_string(path).map_err(|e| format!("failed to read {path:?}: {e}"))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut interp = Interp::new();
        seed_globals(&mut interp, state);

        let mut bridge = ScriptStateBridge { state };
        interp
            .load(&name, &source, &mut bridge)
            .map_err(|e| format!("{name}: {e}"))?;

        Ok(Self {
            interp,
            source_path: path.to_path_buf(),
            name,
        })
    }

    pub fn matches_target(&self, target: &str) -> bool {
        crate::lua_engine::script_target_matches(&self.source_path, &self.name, target)
    }

    /// Returns true if the script defines a callback with the given name.
    pub fn has_callback(&self, name: &str) -> bool {
        self.interp.has_function(name)
    }

    /// Call a callback with numeric args (matches the Lua side's convention
    /// for `onUpdate`, `onBeatHit`, etc.).
    pub fn call_callback(
        &mut self,
        callback: &str,
        state: &mut ScriptState,
        args: &[f64],
    ) -> Result<(), String> {
        if !self.interp.has_function(callback) {
            return Ok(());
        }
        refresh_engine_globals(&mut self.interp, state);
        let hargs: Vec<HValue> = args.iter().map(|x| HValue::Float(*x)).collect();
        let mut bridge = ScriptStateBridge { state };
        self.interp
            .call(callback, &hargs, &mut bridge)
            .map_err(|e| format!("{}: {}: {e}", self.name, callback))?;
        Ok(())
    }

    /// Call a callback where some args are strings (onEvent, onTweenCompleted).
    pub fn call_callback_str(
        &mut self,
        callback: &str,
        state: &mut ScriptState,
        args: &[&str],
    ) -> Result<(), String> {
        if !self.interp.has_function(callback) {
            return Ok(());
        }
        refresh_engine_globals(&mut self.interp, state);
        let hargs: Vec<HValue> = args.iter().map(|s| HValue::from_str(*s)).collect();
        let mut bridge = ScriptStateBridge { state };
        self.interp
            .call(callback, &hargs, &mut bridge)
            .map_err(|e| format!("{}: {}: {e}", self.name, callback))?;
        Ok(())
    }

    pub fn set_global_number(&mut self, name: &str, value: f64) {
        self.interp.set_global(name, HValue::Float(value));
    }

    pub fn set_global_string(&mut self, name: &str, value: &str) {
        self.interp.set_global(name, HValue::from_str(value));
    }

    pub fn set_global_bool(&mut self, name: &str, value: bool) {
        self.interp.set_global(name, HValue::Bool(value));
    }
}

/// Seed the interp's globals with the built-ins Psych mods expect at startup.
/// Called once per script load.
fn seed_globals(interp: &mut Interp, state: &ScriptState) {
    interp.set_global("songName", HValue::from_str(&state.song_name));
    interp.set_global("isStoryMode", HValue::Bool(state.is_story_mode));
    interp.set_global("screenWidth", HValue::Int(state.screen_width as i64));
    interp.set_global("screenHeight", HValue::Int(state.screen_height as i64));
    interp.set_global("modcharts", HValue::Bool(true));
}

/// Refresh the per-frame-ish globals before firing a callback so scripts
/// always see current values. Cheap — just a handful of writes.
fn refresh_engine_globals(interp: &mut Interp, state: &ScriptState) {
    interp.set_global("curBeat", HValue::Int(state.cur_beat as i64));
    interp.set_global("curStep", HValue::Int(state.cur_step as i64));
    interp.set_global("curSection", HValue::Int(state.cur_section as i64));
}

/// Known top-level host identifiers that map to "group.field" property paths.
/// When a script reads one of these names, the bridge returns an opaque
/// Handle whose `id` is the index into this table. A subsequent
/// `target.field = value` then resolves back to the name and becomes a
/// property_writes entry — the same convention Lua's setProperty uses.
const HOST_OBJECTS: &[&str] = &[
    "boyfriend",
    "dad",
    "gf",
    "boyfriendGroup",
    "dadGroup",
    "gfGroup",
    "camGame",
    "camHUD",
    "camOther",
    "game",
];

const HOST_OBJECT_TAG: &str = "game_obj";

fn host_object_id(name: &str) -> Option<u64> {
    HOST_OBJECTS
        .iter()
        .position(|n| *n == name)
        .map(|i| i as u64)
}

fn host_object_name(id: u64) -> Option<&'static str> {
    HOST_OBJECTS.get(id as usize).copied()
}

/// HostBridge impl — this is how .hx code reads/writes into ScriptState.
/// Identifier lookups read from `state.custom_vars` (shared with Lua) and
/// writes go to the same map or the `property_writes` queue. Known game
/// objects like `boyfriend`/`camGame` are returned as opaque handles so
/// `boyfriend.x = 1` routes through field_set → property_writes.
struct ScriptStateBridge<'a> {
    state: &'a mut ScriptState,
}

impl HostBridge for ScriptStateBridge<'_> {
    fn global_get(&mut self, name: &str) -> Result<HValue, String> {
        // Built-in dynamic values first — these change every frame and
        // aren't cached as interp globals.
        match name {
            "curBeat" => return Ok(HValue::Int(self.state.cur_beat as i64)),
            "curStep" => return Ok(HValue::Int(self.state.cur_step as i64)),
            "curSection" => return Ok(HValue::Int(self.state.cur_section as i64)),
            "songName" => return Ok(HValue::from_str(&self.state.song_name)),
            _ => {}
        }

        if let Some(id) = host_object_id(name) {
            return Ok(HValue::Handle {
                tag: HOST_OBJECT_TAG,
                id,
            });
        }

        if let Some(v) = self.state.custom_vars.get(name) {
            return Ok(lua_value_to_h(v));
        }
        Ok(HValue::Null)
    }

    fn global_set(&mut self, name: &str, value: &HValue) -> Result<bool, String> {
        // Route dotted names through property_writes so PlayScreen picks
        // them up the same way it does for Lua.
        if is_property_path(name) {
            self.state
                .property_writes
                .push((name.to_string(), h_value_to_lua(value)));
            return Ok(true);
        }
        // Otherwise stash in custom_vars (also visible to Lua).
        self.state
            .custom_vars
            .insert(name.to_string(), h_value_to_lua(value));
        Ok(true)
    }

    fn field_get(&mut self, target: &HValue, field: &str) -> Result<HValue, String> {
        if let HValue::Handle { tag, id } = target {
            if *tag == HOST_OBJECT_TAG {
                if let Some(obj) = host_object_name(*id) {
                    // Read from property_values — PlayScreen is expected to
                    // populate this map for any property a script has read.
                    let key = format!("{obj}.{field}");
                    if let Some(v) = self.state.property_values.get(&key) {
                        return Ok(lua_value_to_h(v));
                    }
                    return Ok(HValue::Null);
                }
            }
        }
        Err(format!(
            "hscript bridge: field '{field}' access on {target:?} is not wired"
        ))
    }

    fn field_set(&mut self, target: &HValue, field: &str, value: &HValue) -> Result<(), String> {
        if let HValue::Handle { tag, id } = target {
            if *tag == HOST_OBJECT_TAG {
                if let Some(obj) = host_object_name(*id) {
                    self.state
                        .property_writes
                        .push((format!("{obj}.{field}"), h_value_to_lua(value)));
                    return Ok(());
                }
            }
        }
        Err(format!(
            "hscript bridge: field '{field}' set on {target:?} is not wired"
        ))
    }

    fn method_call(
        &mut self,
        _target: &HValue,
        method: &str,
        _args: &[HValue],
    ) -> Result<HValue, String> {
        Err(format!(
            "hscript bridge: method '{method}' is not wired yet"
        ))
    }

    fn construct(&mut self, type_name: &str, _args: &[HValue]) -> Result<HValue, String> {
        Err(format!(
            "hscript bridge: construction of '{type_name}' is not wired yet"
        ))
    }
}

/// Anything that looks like `group.field` gets treated as a property write.
/// Matches the Lua bridge's convention.
fn is_property_path(name: &str) -> bool {
    name.contains('.')
}

fn lua_value_to_h(v: &LuaValue) -> HValue {
    match v {
        LuaValue::Nil => HValue::Null,
        LuaValue::Bool(b) => HValue::Bool(*b),
        LuaValue::Int(i) => HValue::Int(*i),
        LuaValue::Float(f) => HValue::Float(*f),
        LuaValue::String(s) => HValue::from_str(s.clone()),
        LuaValue::Array(items) => HValue::new_array(items.iter().map(lua_value_to_h).collect()),
    }
}

fn h_value_to_lua(v: &HValue) -> LuaValue {
    match v {
        HValue::Null => LuaValue::Nil,
        HValue::Bool(b) => LuaValue::Bool(*b),
        HValue::Int(i) => LuaValue::Int(*i),
        HValue::Float(f) => LuaValue::Float(*f),
        HValue::String(s) => LuaValue::String(s.as_str().to_string()),
        HValue::Array(arr) => LuaValue::Array(arr.borrow().iter().map(h_value_to_lua).collect()),
        // Objects/closures/handles have no Lua equivalent in our bridge — fall
        // back to Nil rather than lose data silently.
        _ => LuaValue::Nil,
    }
}
