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

    /// Set character names (for Lua globals and runHaxeCode switch resolution).
    pub fn set_char_names(&mut self, bf: &str, dad: &str, gf: &str) {
        self.state.bf_name = bf.to_string();
        self.state.dad_name = dad.to_string();
        self.state.gf_name = gf.to_string();
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

    /// Call a note-hit callback with Psych Engine's standard arguments.
    pub fn call_note_hit(&mut self, callback: &str, members_index: usize, note_data: usize, note_type: &str, is_sustain: bool) {
        for script in &mut self.scripts {
            if let Err(e) = script.call_note_callback(callback, &mut self.state, members_index, note_data, note_type, is_sustain) {
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
            // Variable tweens: update custom_vars directly
            if let Some(var_name) = target.strip_prefix("__var_") {
                self.state.custom_vars.insert(var_name.to_string(), LuaValue::Float(val as f64));
                continue;
            }

            // Character/group tweens → emit property writes
            let is_char_target = matches!(target.as_str(),
                "dad" | "dadGroup" | "boyfriend" | "boyfriendGroup" | "gf" | "gfGroup");
            if is_char_target {
                let char_prefix = match target.as_str() {
                    "dad" | "dadGroup" => "dad",
                    "boyfriend" | "boyfriendGroup" => "boyfriend",
                    "gf" | "gfGroup" => "gf",
                    _ => continue,
                };
                let prop_name = match prop {
                    TweenProperty::X => format!("{}.x", char_prefix),
                    TweenProperty::Y => format!("{}.y", char_prefix),
                    TweenProperty::Alpha => format!("{}.alpha", char_prefix),
                    TweenProperty::Angle => format!("{}.angle", char_prefix),
                    _ => continue,
                };
                self.state.property_writes.push((
                    prop_name,
                    LuaValue::Float(val as f64),
                ));
                continue;
            }

            let prop_name = match prop {
                TweenProperty::Zoom => {
                    if target == "camGame" { "camera.zoom" }
                    else if target == "camHUD" { "hud.zoom" }
                    else { continue; }
                }
                _ => continue,
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

    /// Populate note data into all Lua VMs so modcharts can query/modify individual notes.
    /// Each tuple is (strum_time, lane, must_press, sustain_length).
    pub fn populate_note_data(&mut self, notes: &[(f64, usize, bool, f64)]) {
        self.state.note_count = notes.len();
        self.state.note_read_data = notes.to_vec();

        for script in &mut self.scripts {
            script.populate_note_data(notes);
        }
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

    /// Set a string global on all loaded scripts.
    pub fn set_str_on_all(&mut self, name: &str, value: &str) {
        for script in &mut self.scripts {
            script.set_global_string(name, value);
        }
    }

    /// Call onEvent(name, value1, value2) on all scripts.
    pub fn call_event(&mut self, name: &str, value1: &str, value2: &str) {
        for script in &mut self.scripts {
            if let Err(e) = script.call_callback_str("onEvent", &mut self.state, &[name, value1, value2]) {
                log::error!("onEvent error: {}", e);
            }
        }
    }

    /// Call a named Lua function with a single string argument across all scripts.
    /// Used by the "Wildcard" event type (VS Retrospecter) to invoke arbitrary Lua functions.
    pub fn call_lua_function(&mut self, func_name: &str, arg: &str) {
        for script in &mut self.scripts {
            if let Err(e) = script.call_callback_str(func_name, &mut self.state, &[arg]) {
                log::warn!("Wildcard {}({}): {}", func_name, arg, e);
            }
        }
    }
}

impl Default for ScriptManager {
    fn default() -> Self {
        Self::new()
    }
}
