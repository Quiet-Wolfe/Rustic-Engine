mod lua_engine;
mod lua_functions;
mod script_state;

pub use lua_engine::LuaScript;
pub use script_state::{ScriptState, LuaSprite, LuaSpriteKind, LuaValue};

use std::path::{Path, PathBuf};

/// Manages all Lua scripts for a song (stage script + song scripts).
pub struct ScriptManager {
    scripts: Vec<LuaScript>,
    /// Shared mutable state that Lua functions read/write.
    pub state: ScriptState,
}

impl ScriptManager {
    pub fn new() -> Self {
        Self {
            scripts: Vec::new(),
            state: ScriptState::new(),
        }
    }

    /// Set built-in variables before loading scripts.
    pub fn set_globals(&mut self, song_name: &str, is_story_mode: bool) {
        self.state.song_name = song_name.to_string();
        self.state.is_story_mode = is_story_mode;
    }

    /// Set asset search roots so Lua functions can resolve image paths.
    pub fn set_image_roots(&mut self, roots: Vec<PathBuf>) {
        self.state.image_roots = roots;
    }

    /// Load a Lua script from a file path.
    pub fn load_script(&mut self, path: &Path) {
        match LuaScript::load(path, &mut self.state) {
            Ok(script) => {
                log::info!("Loaded Lua script: {:?}", path);
                self.scripts.push(script);
            }
            Err(e) => {
                log::error!("Failed to load Lua script {:?}: {}", path, e);
            }
        }
    }

    /// Call a named callback on all scripts with no arguments.
    pub fn call(&mut self, callback: &str) {
        for script in &mut self.scripts {
            if let Err(e) = script.call_callback(callback, &mut self.state, &[]) {
                log::error!("Lua callback '{}' error: {}", callback, e);
            }
        }
    }

    /// Call a named callback on all scripts, passing `elapsed` (dt) as the first arg.
    pub fn call_with_elapsed(&mut self, callback: &str, elapsed: f64) {
        for script in &mut self.scripts {
            if let Err(e) = script.call_callback(callback, &mut self.state, &[elapsed]) {
                log::error!("Lua callback '{}' error: {}", callback, e);
            }
        }
    }

    /// Call a callback that receives the current beat number.
    pub fn call_beat(&mut self, callback: &str, beat: i32) {
        self.state.cur_beat = beat;
        self.call(callback);
    }

    /// Call a callback that receives the current step number.
    pub fn call_step(&mut self, callback: &str, step: i32) {
        self.state.cur_step = step;
        self.call(callback);
    }

    pub fn has_scripts(&self) -> bool {
        !self.scripts.is_empty()
    }
}

impl Default for ScriptManager {
    fn default() -> Self {
        Self::new()
    }
}
