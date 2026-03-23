mod lua_engine;
mod lua_functions;
mod script_state;
pub mod tweens;

pub use lua_engine::LuaScript;
pub use script_state::{ScriptState, LuaSprite, LuaSpriteKind, LuaValue, StrumProps};
pub use tweens::{TweenManager, Tween, TweenProperty, EaseFunc, LuaTimer};

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

    /// Update tweens/timers and fire completion callbacks.
    pub fn update_tweens(&mut self, dt: f32) {
        self.state.tweens.update(dt);

        // Apply tween values to sprites and strums
        let game_tweens = self.state.tweens.apply_to_sprites(&mut self.state.lua_sprites, &mut self.state.strum_props);

        // Convert game tweens to property writes so PlayScreen can process them
        for (target, prop, val) in game_tweens {
            let prop_name = match prop {
                TweenProperty::Zoom => {
                    if target == "camGame" { "camera.zoom" }
                    else if target == "camHUD" { "hud.zoom" }
                    else { continue; }
                }
                TweenProperty::X | TweenProperty::Y | TweenProperty::Alpha
                | TweenProperty::Angle | TweenProperty::ScaleX | TweenProperty::ScaleY => continue,
            };
            self.state.property_writes.push((
                prop_name.to_string(),
                LuaValue::Float(val as f64),
            ));
        }

        // Fire onTweenCompleted callbacks
        let completed: Vec<(String, String)> = self.state.tweens.completed_tweens.drain(..).collect();
        for (tag, vars) in completed {
            for script in &mut self.scripts {
                if let Err(e) = script.call_callback_str("onTweenCompleted", &mut self.state, &[&tag, &vars]) {
                    log::error!("onTweenCompleted error: {}", e);
                }
            }
        }

        // Fire onTimerCompleted callbacks
        let timer_completed: Vec<(String, i32, i32)> = self.state.tweens.completed_timers.drain(..).collect();
        for (tag, loops_done, loops_left) in timer_completed {
            for script in &mut self.scripts {
                if let Err(e) = script.call_callback_with_mixed(
                    "onTimerCompleted", &mut self.state, &tag, loops_done, loops_left,
                ) {
                    log::error!("onTimerCompleted error: {}", e);
                }
            }
        }
    }

    pub fn has_scripts(&self) -> bool {
        !self.scripts.is_empty()
    }

    /// Set a numeric global on all loaded scripts (like Psych Engine's setOnScripts).
    pub fn set_on_all(&mut self, name: &str, value: f64) {
        for script in &mut self.scripts {
            script.set_global_number(name, value);
        }
    }

    /// Set a boolean global on all loaded scripts.
    pub fn set_bool_on_all(&mut self, name: &str, value: bool) {
        for script in &mut self.scripts {
            script.set_global_bool(name, value);
        }
    }
}

impl Default for ScriptManager {
    fn default() -> Self {
        Self::new()
    }
}
