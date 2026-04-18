mod hscript_engine;
mod lua_engine;
mod lua_functions;
mod script_state;
pub mod tweens;

pub use hscript_engine::HScriptEngine;
pub use lua_engine::LuaScript;
pub use script_state::{
    AudioRequest, LuaSprite, LuaSpriteKind, LuaValue, NoteTypeRegistration, ScriptCallRequest,
    ScriptState, StrumProps,
};
pub use tweens::{EaseFunc, LuaTimer, Tween, TweenManager, TweenProperty};

use std::path::{Path, PathBuf};

/// A loaded script — dispatches Lua vs HScript by file extension at load time.
/// Callbacks are fanned out across both flavors so `onUpdate` etc. fire
/// regardless of which language the mod used.
enum Script {
    Lua(LuaScript),
    HScript(HScriptEngine),
}

/// Manages all scripts for a song (stage + song + custom event scripts).
pub struct ScriptManager {
    scripts: Vec<Script>,
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

    /// Set song metadata globals (BPM, stage, scroll speed, difficulty, etc.).
    pub fn set_song_metadata(
        &mut self,
        bpm: f64,
        scroll_speed: f64,
        song_length: f64,
        cur_stage: &str,
        difficulty_name: &str,
        mod_folder: &str,
    ) {
        self.state.bpm = bpm;
        self.state.scroll_speed = scroll_speed;
        self.state.song_length = song_length;
        self.state.cur_stage = cur_stage.to_string();
        self.state.difficulty_name = difficulty_name.to_string();
        self.state.mod_folder = mod_folder.to_string();
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

    /// Load a script file. Dispatches by extension: `.hx` → HScript,
    /// anything else → Lua.
    pub fn load_script(&mut self, path: &Path) {
        let is_hscript = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("hx"))
            .unwrap_or(false);

        if is_hscript {
            match HScriptEngine::load(path, &mut self.state) {
                Ok(engine) => {
                    log::info!("Loaded HScript: {:?}", path);
                    self.scripts.push(Script::HScript(engine));
                    self.refresh_running_scripts();
                }
                Err(e) => {
                    log::error!("Failed to load HScript {:?}: {}", path, e);
                }
            }
        } else {
            match LuaScript::load(path, &mut self.state) {
                Ok(script) => {
                    log::info!("Loaded Lua script: {:?}", path);
                    self.scripts.push(Script::Lua(script));
                    self.refresh_running_scripts();
                }
                Err(e) => {
                    log::error!("Failed to load Lua script {:?}: {}", path, e);
                }
            }
        }
    }

    /// Call a named callback on all scripts with no arguments.
    pub fn call(&mut self, callback: &str) {
        self.refresh_running_scripts();
        for script in &mut self.scripts {
            let result = match script {
                Script::Lua(s) => s.call_callback(callback, &mut self.state, &[]),
                Script::HScript(s) => s.call_callback(callback, &mut self.state, &[]),
            };
            if let Err(e) = result {
                log::error!("callback '{}' error: {}", callback, e);
            }
        }
        self.process_script_control_requests();
    }

    /// Call a named callback on all scripts, passing `elapsed` (dt) as the first arg.
    pub fn call_with_elapsed(&mut self, callback: &str, elapsed: f64) {
        self.refresh_running_scripts();
        for script in &mut self.scripts {
            let result = match script {
                Script::Lua(s) => s.call_callback(callback, &mut self.state, &[elapsed]),
                Script::HScript(s) => s.call_callback(callback, &mut self.state, &[elapsed]),
            };
            if let Err(e) = result {
                log::error!("callback '{}' error: {}", callback, e);
            }
        }
        self.process_script_control_requests();
    }

    /// Call a note-hit callback with Psych Engine's standard arguments.
    /// HScript side receives them as four positional args in the same order.
    pub fn call_note_hit(
        &mut self,
        callback: &str,
        members_index: usize,
        note_data: usize,
        note_type: &str,
        is_sustain: bool,
    ) {
        self.refresh_running_scripts();
        for script in &mut self.scripts {
            let result = match script {
                Script::Lua(s) => s.call_note_callback(
                    callback,
                    &mut self.state,
                    members_index,
                    note_data,
                    note_type,
                    is_sustain,
                ),
                Script::HScript(s) => {
                    // HScript bridge takes numeric-only or string-only right now;
                    // wrap as strings so mods can use them uniformly.
                    let mi = members_index.to_string();
                    let nd = note_data.to_string();
                    let sus = if is_sustain { "true" } else { "false" };
                    s.call_callback_str(callback, &mut self.state, &[&mi, &nd, note_type, sus])
                }
            };
            if let Err(e) = result {
                log::error!("callback '{}' error: {}", callback, e);
            }
        }
        self.process_script_control_requests();
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
        self.refresh_running_scripts();
        self.state.tweens.update(dt);

        // Apply tween values to sprites and strums
        let game_tweens = self
            .state
            .tweens
            .apply_to_sprites(&mut self.state.lua_sprites, &mut self.state.strum_props);

        // Convert game tweens to property writes so PlayScreen can process them
        for (target, prop, val) in game_tweens {
            // Variable tweens: update custom_vars directly
            if let Some(var_name) = target.strip_prefix("__var_") {
                self.state
                    .custom_vars
                    .insert(var_name.to_string(), LuaValue::Float(val as f64));
                continue;
            }

            // Character/group tweens → emit property writes
            let is_char_target = matches!(
                target.as_str(),
                "dad" | "dadGroup" | "boyfriend" | "boyfriendGroup" | "gf" | "gfGroup"
            );
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
                self.state
                    .property_writes
                    .push((prop_name, LuaValue::Float(val as f64)));
                continue;
            }

            let prop_name = match prop {
                TweenProperty::Zoom => {
                    if target == "camGame" {
                        "camera.zoom"
                    } else if target == "camHUD" {
                        "hud.zoom"
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };
            self.state
                .property_writes
                .push((prop_name.to_string(), LuaValue::Float(val as f64)));
        }

        // Fire onTweenCompleted callbacks
        let completed: Vec<(String, String)> =
            self.state.tweens.completed_tweens.drain(..).collect();
        for (tag, vars) in completed {
            for script in &mut self.scripts {
                let result = match script {
                    Script::Lua(s) => {
                        s.call_callback_str("onTweenCompleted", &mut self.state, &[&tag, &vars])
                    }
                    Script::HScript(s) => {
                        s.call_callback_str("onTweenCompleted", &mut self.state, &[&tag, &vars])
                    }
                };
                if let Err(e) = result {
                    log::error!("onTweenCompleted error: {}", e);
                }
            }
        }

        // Fire onTimerCompleted callbacks
        let timer_completed: Vec<(String, i32, i32)> =
            self.state.tweens.completed_timers.drain(..).collect();
        for (tag, loops_done, loops_left) in timer_completed {
            for script in &mut self.scripts {
                let result = match script {
                    Script::Lua(s) => s.call_callback_with_mixed(
                        "onTimerCompleted",
                        &mut self.state,
                        &tag,
                        loops_done,
                        loops_left,
                    ),
                    Script::HScript(s) => {
                        let done = loops_done.to_string();
                        let left = loops_left.to_string();
                        s.call_callback_str(
                            "onTimerCompleted",
                            &mut self.state,
                            &[&tag, &done, &left],
                        )
                    }
                };
                if let Err(e) = result {
                    log::error!("onTimerCompleted error: {}", e);
                }
            }
        }
        self.process_script_control_requests();
    }

    pub fn has_scripts(&self) -> bool {
        !self.scripts.is_empty()
    }

    /// Populate note data into all script VMs so modcharts can query/modify
    /// individual notes. HScript side stores the same info but doesn't expose
    /// a per-note API yet, so it's a no-op there.
    /// Each tuple is (strum_time, lane, must_press, sustain_length).
    pub fn populate_note_data(&mut self, notes: &[(f64, usize, bool, f64)]) {
        self.state.note_count = notes.len();
        self.state.note_read_data = notes.to_vec();

        for script in &mut self.scripts {
            if let Script::Lua(s) = script {
                s.populate_note_data(notes);
            }
        }
    }

    /// Set a numeric global on all loaded scripts (like Psych Engine's setOnScripts).
    pub fn set_on_all(&mut self, name: &str, value: f64) {
        for script in &mut self.scripts {
            match script {
                Script::Lua(s) => s.set_global_number(name, value),
                Script::HScript(s) => s.set_global_number(name, value),
            }
        }
    }

    /// Set a boolean global on all loaded scripts.
    pub fn set_bool_on_all(&mut self, name: &str, value: bool) {
        for script in &mut self.scripts {
            match script {
                Script::Lua(s) => s.set_global_bool(name, value),
                Script::HScript(s) => s.set_global_bool(name, value),
            }
        }
    }

    /// Set a string global on all loaded scripts.
    pub fn set_str_on_all(&mut self, name: &str, value: &str) {
        for script in &mut self.scripts {
            match script {
                Script::Lua(s) => s.set_global_string(name, value),
                Script::HScript(s) => s.set_global_string(name, value),
            }
        }
    }

    /// Call onEvent(name, value1, value2) on all scripts.
    pub fn call_event(&mut self, name: &str, value1: &str, value2: &str) {
        self.refresh_running_scripts();
        for script in &mut self.scripts {
            let result = match script {
                Script::Lua(s) => {
                    s.call_callback_str("onEvent", &mut self.state, &[name, value1, value2])
                }
                Script::HScript(s) => {
                    s.call_callback_str("onEvent", &mut self.state, &[name, value1, value2])
                }
            };
            if let Err(e) = result {
                log::error!("onEvent error: {}", e);
            }
        }
        self.process_script_control_requests();
    }

    /// Call a named function with a single string argument across all scripts.
    /// Used by the "Wildcard" event type (VS Retrospecter) to invoke arbitrary
    /// script functions by name.
    pub fn call_lua_function(&mut self, func_name: &str, arg: &str) {
        self.refresh_running_scripts();
        for script in &mut self.scripts {
            let result = match script {
                Script::Lua(s) => s.call_callback_str(func_name, &mut self.state, &[arg]),
                Script::HScript(s) => s.call_callback_str(func_name, &mut self.state, &[arg]),
            };
            if let Err(e) = result {
                log::warn!("Wildcard {}({}): {}", func_name, arg, e);
            }
        }
        self.process_script_control_requests();
    }

    fn process_script_control_requests(&mut self) {
        loop {
            let removes: Vec<String> = self.state.script_remove_requests.drain(..).collect();
            if !removes.is_empty() {
                self.scripts.retain_mut(|script| match script {
                    Script::Lua(s) => !removes.iter().any(|target| s.matches_target(target)),
                    Script::HScript(_) => true,
                });
                self.refresh_running_scripts();
            }

            let requests: Vec<ScriptCallRequest> =
                self.state.script_call_requests.drain(..).collect();
            if requests.is_empty() {
                break;
            }

            for request in requests {
                for script in &mut self.scripts {
                    let should_call = match &request.target {
                        Some(target) => match script {
                            Script::Lua(s) => s.matches_target(target),
                            Script::HScript(_) => false,
                        },
                        None => true,
                    };
                    if !should_call {
                        continue;
                    }

                    let result = match script {
                        Script::Lua(s) => s.call_callback_values(
                            &request.function,
                            &mut self.state,
                            &request.args,
                        ),
                        Script::HScript(s) => {
                            if request.target.is_some() {
                                Ok(())
                            } else {
                                let args: Vec<String> =
                                    request.args.iter().map(lua_value_to_arg_string).collect();
                                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                                s.call_callback_str(&request.function, &mut self.state, &refs)
                            }
                        }
                    };

                    if let Err(e) = result {
                        log::warn!("script call {}: {}", request.function, e);
                    }
                }
            }
        }
    }

    fn refresh_running_scripts(&mut self) {
        self.state.running_scripts = self
            .scripts
            .iter()
            .filter_map(|script| match script {
                Script::Lua(s) => Some(s.source_name()),
                Script::HScript(_) => None,
            })
            .collect();
    }
}

impl Default for ScriptManager {
    fn default() -> Self {
        Self::new()
    }
}

fn lua_value_to_arg_string(value: &LuaValue) -> String {
    match value {
        LuaValue::Nil => String::new(),
        LuaValue::Bool(v) => v.to_string(),
        LuaValue::Int(v) => v.to_string(),
        LuaValue::Float(v) => v.to_string(),
        LuaValue::String(v) => v.clone(),
        LuaValue::Array(_) => String::new(),
    }
}
