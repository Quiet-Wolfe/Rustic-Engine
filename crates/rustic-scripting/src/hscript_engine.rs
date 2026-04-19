//! HScript engine — a thin wrapper that gives .hx scripts access to the same
//! `ScriptState` as Lua scripts do.
//!
//! Psych mods lean on HScript for logic that's awkward in Lua (complex
//! data structures, proper closures). We don't need to match haxe semantics
//! exactly — just cover the subset Psych mods actually use. The evaluator
//! itself lives in `rustic-hscript`; this file only deals with the bridge
//! between our ScriptState and that interpreter's HostBridge trait.

use std::path::{Path, PathBuf};

use rustic_hscript::{Interp, Value as HValue};

use crate::hscript_bridge::{refresh_engine_globals, seed_globals, ScriptStateBridge};
use crate::script_state::ScriptState;

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

    /// Call a callback with mixed typed values. This is used for Psych
    /// callbacks like goodNoteHit(id, direction, noteType, isSustainNote).
    pub fn call_callback_values(
        &mut self,
        callback: &str,
        state: &mut ScriptState,
        args: &[crate::script_state::LuaValue],
    ) -> Result<(), String> {
        if !self.interp.has_function(callback) {
            return Ok(());
        }
        refresh_engine_globals(&mut self.interp, state);
        let hargs: Vec<HValue> = args.iter().map(lua_value_to_h).collect();
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

fn lua_value_to_h(value: &crate::script_state::LuaValue) -> HValue {
    match value {
        crate::script_state::LuaValue::Nil => HValue::Null,
        crate::script_state::LuaValue::Bool(v) => HValue::Bool(*v),
        crate::script_state::LuaValue::Int(v) => HValue::Int(*v),
        crate::script_state::LuaValue::Float(v) => HValue::Float(*v),
        crate::script_state::LuaValue::String(v) => HValue::from_str(v),
        crate::script_state::LuaValue::Array(values) => {
            HValue::new_array(values.iter().map(lua_value_to_h).collect())
        }
    }
}
