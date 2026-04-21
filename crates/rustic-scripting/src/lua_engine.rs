use std::path::{Path, PathBuf};

use mlua::prelude::*;

use crate::lua_functions;
use crate::script_state::ScriptState;

/// Parse a strum target name like "__strum_opponent_2" or "__strum_player_1" to an index 0-7.
fn parse_strum_index(target: &str) -> Option<usize> {
    if let Some(n) = target.strip_prefix("__strum_opponent_") {
        n.parse::<usize>().ok().filter(|&i| i < 4)
    } else if let Some(n) = target.strip_prefix("__strum_player_") {
        n.parse::<usize>().ok().filter(|&i| i < 4).map(|i| i + 4)
    } else {
        None
    }
}

/// A single Lua script instance.
pub struct LuaScript {
    lua: Lua,
    source_path: PathBuf,
    script_name: String,
    /// Whether this script has been closed (via close() from Lua).
    closed: bool,
}

impl LuaScript {
    /// Inject a Psych-created stage sprite into Lua before `onCreate` runs.
    /// Stage JSON speakers are regular modchart sprites in several mods, so
    /// scripts must be able to mutate `speaker.x`, `speaker.visible`, etc.
    pub fn inject_animated_sprite(
        &self,
        tag: &str,
        image: &str,
        x: f32,
        y: f32,
        anim: &str,
        prefix: &str,
        in_front: bool,
    ) -> Result<(), String> {
        let globals = self.lua.globals();
        let tbl = self
            .lua
            .create_table()
            .map_err(|e| format!("Failed to create sprite table: {}", e))?;
        tbl.set("tag", tag)
            .map_err(|e| format!("Failed to set tag: {}", e))?;
        tbl.set("kind", "animated")
            .map_err(|e| format!("Failed to set kind: {}", e))?;
        tbl.set("image", image)
            .map_err(|e| format!("Failed to set image: {}", e))?;
        tbl.set("x", x)
            .map_err(|e| format!("Failed to set x: {}", e))?;
        tbl.set("y", y)
            .map_err(|e| format!("Failed to set y: {}", e))?;
        tbl.set("scale_x", 1.0)
            .map_err(|e| format!("Failed to set scale_x: {}", e))?;
        tbl.set("scale_y", 1.0)
            .map_err(|e| format!("Failed to set scale_y: {}", e))?;
        tbl.set("scroll_x", 1.0)
            .map_err(|e| format!("Failed to set scroll_x: {}", e))?;
        tbl.set("scroll_y", 1.0)
            .map_err(|e| format!("Failed to set scroll_y: {}", e))?;
        tbl.set("alpha", 1.0)
            .map_err(|e| format!("Failed to set alpha: {}", e))?;
        tbl.set("visible", true)
            .map_err(|e| format!("Failed to set visible: {}", e))?;
        tbl.set("flip_x", false)
            .map_err(|e| format!("Failed to set flip_x: {}", e))?;
        tbl.set("antialiasing", true)
            .map_err(|e| format!("Failed to set antialiasing: {}", e))?;

        let anims = self
            .lua
            .create_table()
            .map_err(|e| format!("Failed to create anim table: {}", e))?;
        let def = self
            .lua
            .create_table()
            .map_err(|e| format!("Failed to create anim def: {}", e))?;
        def.set("prefix", prefix)
            .map_err(|e| format!("Failed to set anim prefix: {}", e))?;
        def.set("fps", 24.0)
            .map_err(|e| format!("Failed to set anim fps: {}", e))?;
        def.set("looping", true)
            .map_err(|e| format!("Failed to set anim looping: {}", e))?;
        def.set(
            "indices",
            self.lua
                .create_table()
                .map_err(|e| format!("Failed to create anim indices: {}", e))?,
        )
        .map_err(|e| format!("Failed to set anim indices: {}", e))?;
        anims
            .set(anim, def)
            .map_err(|e| format!("Failed to set animation: {}", e))?;
        tbl.set("__anims", anims)
            .map_err(|e| format!("Failed to set anims: {}", e))?;
        tbl.set("current_anim", anim)
            .map_err(|e| format!("Failed to set current anim: {}", e))?;

        let sprite_data: LuaTable = globals
            .get("__sprite_data")
            .map_err(|e| format!("Failed to get __sprite_data: {}", e))?;
        sprite_data
            .set(tag, tbl.clone())
            .map_err(|e| format!("Failed to register sprite data: {}", e))?;

        let pending_sprites: LuaTable = globals
            .get("__pending_sprites")
            .map_err(|e| format!("Failed to get __pending_sprites: {}", e))?;
        pending_sprites
            .set(pending_sprites.len().unwrap_or(0) + 1, tbl)
            .map_err(|e| format!("Failed to queue sprite: {}", e))?;

        let pending_adds: LuaTable = globals
            .get("__pending_adds")
            .map_err(|e| format!("Failed to get __pending_adds: {}", e))?;
        let add_tbl = self
            .lua
            .create_table()
            .map_err(|e| format!("Failed to create add table: {}", e))?;
        add_tbl
            .set("tag", tag)
            .map_err(|e| format!("Failed to set add tag: {}", e))?;
        add_tbl
            .set("in_front", in_front)
            .map_err(|e| format!("Failed to set add front: {}", e))?;
        pending_adds
            .set(pending_adds.len().unwrap_or(0) + 1, add_tbl)
            .map_err(|e| format!("Failed to queue add: {}", e))?;

        Ok(())
    }

    /// Load and execute a Lua script file.
    /// Sets up built-in variables and functions, then executes the file.
    pub fn load(path: &Path, state: &mut ScriptState) -> Result<Self, String> {
        let lua = Lua::new();

        // Register Lua functions first so they don't overwrite our state
        lua_functions::register_all(&lua)
            .map_err(|e| format!("Failed to register Lua functions: {}", e))?;

        // Set built-in global variables
        {
            let globals = lua.globals();
            globals
                .set("songName", state.song_name.clone())
                .map_err(|e| format!("Failed to set songName: {}", e))?;
            globals
                .set("isStoryMode", state.is_story_mode)
                .map_err(|e| format!("Failed to set isStoryMode: {}", e))?;
            globals
                .set("screenWidth", state.screen_width as i32)
                .map_err(|e| format!("Failed to set screenWidth: {}", e))?;
            globals
                .set("screenHeight", state.screen_height as i32)
                .map_err(|e| format!("Failed to set screenHeight: {}", e))?;
            globals
                .set("curBeat", 0)
                .map_err(|e| format!("Failed to set curBeat: {}", e))?;
            globals
                .set("curStep", 0)
                .map_err(|e| format!("Failed to set curStep: {}", e))?;
            globals
                .set("curSection", 0)
                .map_err(|e| format!("Failed to set curSection: {}", e))?;
            // Psych Engine compatibility
            globals
                .set("modcharts", true)
                .map_err(|e| format!("Failed to set modcharts: {}", e))?;
            globals
                .set("difficulty", 1) // normal = 1
                .map_err(|e| format!("Failed to set difficulty: {}", e))?;

            // Song metadata globals — override the defaults from register_all
            let song_path = state.song_name.to_lowercase().replace(' ', "-");
            globals
                .set("songPath", song_path.clone())
                .map_err(|e| format!("Failed to set songPath: {}", e))?;
            globals
                .set("loadedSongName", state.song_name.clone())
                .map_err(|e| format!("Failed to set loadedSongName: {}", e))?;
            globals
                .set("loadedSongPath", song_path)
                .map_err(|e| format!("Failed to set loadedSongPath: {}", e))?;
            globals
                .set("curStage", state.cur_stage.clone())
                .map_err(|e| format!("Failed to set curStage: {}", e))?;
            globals
                .set("songLength", state.song_length)
                .map_err(|e| format!("Failed to set songLength: {}", e))?;
            globals
                .set("bpm", state.bpm)
                .map_err(|e| format!("Failed to set bpm: {}", e))?;
            globals
                .set("curBpm", state.bpm)
                .map_err(|e| format!("Failed to set curBpm: {}", e))?;
            globals
                .set("scrollSpeed", state.scroll_speed)
                .map_err(|e| format!("Failed to set scrollSpeed: {}", e))?;
            if state.bpm > 0.0 {
                let crochet = 60000.0 / state.bpm;
                globals
                    .set("crochet", crochet)
                    .map_err(|e| format!("Failed to set crochet: {}", e))?;
                globals
                    .set("stepCrochet", crochet / 4.0)
                    .map_err(|e| format!("Failed to set stepCrochet: {}", e))?;
            }
            if !state.difficulty_name.is_empty() {
                globals
                    .set("difficultyName", state.difficulty_name.clone())
                    .map_err(|e| format!("Failed to set difficultyName: {}", e))?;
            }
            if !state.mod_folder.is_empty() {
                globals
                    .set("modFolder", state.mod_folder.clone())
                    .map_err(|e| format!("Failed to set modFolder: {}", e))?;
            }
            globals
                .set("downscroll", state.downscroll)
                .map_err(|e| format!("Failed to set downscroll: {}", e))?;
        }

        // Store image search roots so Lua functions can resolve image paths
        {
            let roots_table = lua
                .create_table()
                .map_err(|e| format!("Failed to create roots table: {}", e))?;
            for (i, root) in state.image_roots.iter().enumerate() {
                roots_table
                    .set(i + 1, root.to_string_lossy().to_string())
                    .map_err(|e| format!("Failed to set root: {}", e))?;
            }
            lua.globals()
                .set("__image_roots", roots_table)
                .map_err(|e| format!("Failed to set __image_roots: {}", e))?;
        }

        // Execute the script file
        let source = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;

        let script_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        lua.load(&source)
            .set_name(script_name)
            .exec()
            .map_err(|e| format!("Lua exec error in {:?}: {}", path, e))?;

        Ok(Self {
            lua,
            source_path: path.to_path_buf(),
            script_name: script_name.to_string(),
            closed: false,
        })
    }

    pub fn matches_target(&self, target: &str) -> bool {
        script_target_matches(&self.source_path, &self.script_name, target)
    }

    pub fn source_name(&self) -> String {
        self.source_path.to_string_lossy().replace('\\', "/")
    }

    /// Set a numeric global variable on this script's Lua VM.
    pub fn set_global_number(&self, name: &str, value: f64) {
        self.lua.globals().set(name, value).ok();
    }

    /// Set a boolean global variable on this script's Lua VM.
    pub fn set_global_bool(&self, name: &str, value: bool) {
        self.lua.globals().set(name, value).ok();
    }

    /// Set a string global variable on this script's Lua VM.
    pub fn set_global_string(&self, name: &str, value: &str) {
        self.lua.globals().set(name, value.to_string()).ok();
    }

    pub fn set_global_value(&self, name: &str, value: &crate::script_state::LuaValue) {
        if let Ok(value) = script_value_to_lua(&self.lua, value) {
            self.lua.globals().set(name, value).ok();
        }
    }

    /// Sync object order mapping to this script's Lua VM.
    pub fn set_object_orders(&self, tags: &[String]) {
        let globals = self.lua.globals();
        if let Ok(tbl) = self.lua.create_table() {
            for (i, tag) in tags.iter().enumerate() {
                tbl.set(tag.as_str(), i as i32).ok();
            }
            globals.set("__object_orders", tbl).ok();
        }
    }

    /// Sync shared game state from Rust into this Lua VM before a callback.
    /// Ensures all scripts see consistent strum positions, camera state, etc.
    fn sync_shared_state(&self, state: &ScriptState) {
        let globals = self.lua.globals();
        globals.set("curBeat", state.cur_beat).ok();
        globals.set("curStep", state.cur_step).ok();
        globals.set("curSection", state.cur_section).ok();
        globals.set("__songPosition", state.song_position).ok();
        globals
            .set("defaultCamZoom", state.default_cam_zoom as f64)
            .ok();
        globals.set("cameraSpeed", state.camera_speed as f64).ok();
        globals.set("__health", state.health as f64).ok();
        globals.set("score", state.score).ok();
        globals.set("misses", state.misses).ok();
        globals.set("hits", state.hits).ok();
        globals.set("combo", state.combo).ok();
        globals.set("rating", state.rating).ok();
        globals.set("ratingName", state.rating_name.as_str()).ok();
        globals.set("ratingFC", state.rating_fc.as_str()).ok();
        globals
            .set("__dad_anim_name", state.dad_anim_name.as_str())
            .ok();
        globals
            .set("__bf_anim_name", state.bf_anim_name.as_str())
            .ok();
        globals
            .set("__gf_anim_name", state.gf_anim_name.as_str())
            .ok();
        globals
            .set("__dad_anim_frame", state.dad_anim_frame as i64)
            .ok();
        globals
            .set("__bf_anim_frame", state.bf_anim_frame as i64)
            .ok();
        globals
            .set("__gf_anim_frame", state.gf_anim_frame as i64)
            .ok();
        globals
            .set("__dad_anim_finished", state.dad_anim_finished)
            .ok();
        globals
            .set("__bf_anim_finished", state.bf_anim_finished)
            .ok();
        globals
            .set("__gf_anim_finished", state.gf_anim_finished)
            .ok();
        globals.set("__dad_x", state.dad_pos.0 as f64).ok();
        globals.set("__dad_y", state.dad_pos.1 as f64).ok();
        globals.set("__bf_x", state.bf_pos.0 as f64).ok();
        globals.set("__bf_y", state.bf_pos.1 as f64).ok();
        globals.set("__gf_x", state.gf_pos.0 as f64).ok();
        globals.set("__gf_y", state.gf_pos.1 as f64).ok();
        globals
            .set("__cam_game_scroll_x", state.camera_scroll.0 as f64)
            .ok();
        globals
            .set("__cam_game_scroll_y", state.camera_scroll.1 as f64)
            .ok();
        globals
            .set("__dad_group_x", state.dad_group_pos.0 as f64)
            .ok();
        globals
            .set("__dad_group_y", state.dad_group_pos.1 as f64)
            .ok();
        globals
            .set("__bf_group_x", state.bf_group_pos.0 as f64)
            .ok();
        globals
            .set("__bf_group_y", state.bf_group_pos.1 as f64)
            .ok();
        globals
            .set("__gf_group_x", state.gf_group_pos.0 as f64)
            .ok();
        globals
            .set("__gf_group_y", state.gf_group_pos.1 as f64)
            .ok();
        globals
            .set(
                "__opponent_camera_offset_x",
                state.opponent_camera_offset.0 as f64,
            )
            .ok();
        globals
            .set(
                "__opponent_camera_offset_y",
                state.opponent_camera_offset.1 as f64,
            )
            .ok();
        globals
            .set("__bf_camera_offset_x", state.bf_camera_offset.0 as f64)
            .ok();
        globals
            .set("__bf_camera_offset_y", state.bf_camera_offset.1 as f64)
            .ok();
        globals
            .set("__dad_camera_x", state.dad_camera_position.0 as f64)
            .ok();
        globals
            .set("__dad_camera_y", state.dad_camera_position.1 as f64)
            .ok();
        globals
            .set("__bf_camera_x", state.bf_camera_position.0 as f64)
            .ok();
        globals
            .set("__bf_camera_y", state.bf_camera_position.1 as f64)
            .ok();
        globals
            .set("__gf_camera_x", state.gf_camera_position.0 as f64)
            .ok();
        globals
            .set("__gf_camera_y", state.gf_camera_position.1 as f64)
            .ok();

        if let Ok(running) = self.lua.create_table() {
            for (i, script) in state.running_scripts.iter().enumerate() {
                running.set(i + 1, script.as_str()).ok();
            }
            globals.set("__running_scripts", running).ok();
        }
        self.sync_string_set("__input_pressed", &state.input_pressed);
        self.sync_string_set("__input_just_pressed", &state.input_just_pressed);
        self.sync_string_set("__input_just_released", &state.input_just_released);
        globals.set("__mouse_x", state.mouse_position.0 as f64).ok();
        globals.set("__mouse_y", state.mouse_position.1 as f64).ok();
        globals.set("__mouse_pressed", state.mouse_pressed).ok();
        globals
            .set("__mouse_just_pressed", state.mouse_just_pressed)
            .ok();
        globals
            .set("__mouse_just_released", state.mouse_just_released)
            .ok();
        self.sync_number_map("__sound_volumes", &state.sound_volumes);
        self.sync_number_map("__sound_times", &state.sound_times);
        self.sync_number_map("__sound_pitches", &state.sound_pitches);
        self.sync_string_set("__sound_exists", &state.sound_tags);

        // Sync shared custom variables so all scripts see each other's setProperty values
        if let Ok(custom) = globals.get::<LuaTable>("__custom_vars") {
            for (key, val) in &state.custom_vars {
                match val {
                    crate::script_state::LuaValue::Float(n) => {
                        custom.set(key.as_str(), *n).ok();
                    }
                    crate::script_state::LuaValue::Int(n) => {
                        custom.set(key.as_str(), *n).ok();
                    }
                    crate::script_state::LuaValue::Bool(b) => {
                        custom.set(key.as_str(), *b).ok();
                    }
                    crate::script_state::LuaValue::String(s) => {
                        custom.set(key.as_str(), s.as_str()).ok();
                    }
                    _ => {}
                }
            }
        }

        // Sync strum properties so all scripts see current positions
        if let Ok(strum_tbl) = globals.get::<LuaTable>("__strum_props") {
            for i in 0..8 {
                if let Ok(tbl) = strum_tbl.get::<LuaTable>(i as i64 + 1) {
                    let sp = &state.strum_props[i];
                    tbl.set("x", sp.x as f64).ok();
                    tbl.set("y", sp.y as f64).ok();
                    tbl.set("alpha", sp.alpha as f64).ok();
                    tbl.set("angle", sp.angle as f64).ok();
                    tbl.set("scale_x", sp.scale_x as f64).ok();
                    tbl.set("scale_y", sp.scale_y as f64).ok();
                    let effective_ds = sp.down_scroll.unwrap_or(state.downscroll);
                    tbl.set("downScroll", effective_ds).ok();
                    tbl.set("custom", sp.custom).ok();
                }
            }
        }

        // Sync lua_sprites properties so all scripts see tweened values
        // This prevents completed tweens from reverting to their pre-tween Lua table values
        if let Ok(sprite_data) = globals.get::<LuaTable>("__sprite_data") {
            for (tag, sprite) in &state.lua_sprites {
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                    tbl.set("x", sprite.x as f64).ok();
                    tbl.set("y", sprite.y as f64).ok();
                    tbl.set("alpha", sprite.alpha as f64).ok();
                    tbl.set("angle", sprite.angle as f64).ok();
                    tbl.set("scale_x", sprite.scale_x as f64).ok();
                    tbl.set("scale_y", sprite.scale_y as f64).ok();
                    tbl.set("offset_x", sprite.offset_x as f64).ok();
                    tbl.set("offset_y", sprite.offset_y as f64).ok();
                    tbl.set("ct_red", sprite.color_red_offset as f64).ok();
                    tbl.set("ct_green", sprite.color_green_offset as f64).ok();
                    tbl.set("ct_blue", sprite.color_blue_offset as f64).ok();
                    tbl.set("current_anim", sprite.current_anim.as_str()).ok();
                    tbl.set("animation.name", sprite.current_anim.as_str()).ok();
                    tbl.set("animation.finished", sprite.anim_finished).ok();
                    tbl.set("anim_frame", sprite.anim_frame as i64).ok();
                    tbl.set("anim_finished", sprite.anim_finished).ok();
                    tbl.set("anim_fps", sprite.anim_fps as f64).ok();

                    let anim_tbl = match tbl.get::<LuaTable>("animation") {
                        Ok(t) => t,
                        Err(_) => match self.lua.create_table() {
                            Ok(t) => {
                                tbl.set("animation", t.clone()).ok();
                                t
                            }
                            Err(_) => continue,
                        },
                    };
                    anim_tbl.set("name", sprite.current_anim.as_str()).ok();
                    anim_tbl.set("finished", sprite.anim_finished).ok();
                    let cur_anim_tbl = match anim_tbl.get::<LuaTable>("curAnim") {
                        Ok(t) => t,
                        Err(_) => match self.lua.create_table() {
                            Ok(t) => {
                                anim_tbl.set("curAnim", t.clone()).ok();
                                t
                            }
                            Err(_) => continue,
                        },
                    };
                    cur_anim_tbl.set("name", sprite.current_anim.as_str()).ok();
                    cur_anim_tbl.set("curFrame", sprite.anim_frame as i64).ok();
                    cur_anim_tbl.set("finished", sprite.anim_finished).ok();
                    cur_anim_tbl.set("frameRate", sprite.anim_fps as f64).ok();
                }
            }
        }
    }

    pub fn call_callback(
        &mut self,
        name: &str,
        state: &mut ScriptState,
        args: &[f64],
    ) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }

        self.sync_shared_state(state);
        let globals = self.lua.globals();

        // Check if the callback exists
        let func: Option<LuaFunction> = globals.get(name).ok();
        let func = match func {
            Some(f) => f,
            None => return Ok(()), // callback doesn't exist, that's fine
        };

        // Build MultiValue from args
        let mut multi = mlua::MultiValue::new();
        for &v in args {
            multi.push_back(mlua::Value::Number(v));
        }

        func.call::<()>(multi)
            .map_err(|e| format!("Lua callback '{}' error: {}", name, e))?;

        // Check if script was closed
        if let Ok(closed) = globals.get::<bool>("__script_closed") {
            if closed {
                self.closed = true;
            }
        }

        // Drain pending sprite operations from Lua registry
        self.drain_sprite_ops(state);

        // Sync sprite properties from Lua tables to Rust structs
        self.sync_sprite_data(state);

        Ok(())
    }

    /// Call a Lua callback with dynamic Psych-style arguments.
    pub fn call_callback_values(
        &mut self,
        name: &str,
        state: &mut ScriptState,
        args: &[crate::script_state::LuaValue],
    ) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }

        self.sync_shared_state(state);

        let func: Option<LuaFunction> = self.lua.globals().get(name).ok();
        let Some(func) = func else {
            return Ok(());
        };

        let mut multi = mlua::MultiValue::new();
        for arg in args {
            multi.push_back(script_value_to_lua(&self.lua, arg).map_err(|e| e.to_string())?);
        }

        func.call::<()>(multi)
            .map_err(|e| format!("Lua callback '{}' error: {}", name, e))?;

        if let Ok(closed) = self.lua.globals().get::<bool>("__script_closed") {
            if closed {
                self.closed = true;
            }
        }
        self.drain_sprite_ops(state);
        self.sync_sprite_data(state);
        Ok(())
    }

    /// Call a Lua callback with string arguments (for onTweenCompleted etc.).
    pub fn call_callback_str(
        &mut self,
        name: &str,
        state: &mut ScriptState,
        str_args: &[&str],
    ) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.sync_shared_state(state);

        let func: Option<LuaFunction> = self.lua.globals().get(name).ok();
        let func = match func {
            Some(f) => f,
            None => return Ok(()),
        };

        let mut multi = mlua::MultiValue::new();
        for &s in str_args {
            multi.push_back(mlua::Value::String(
                self.lua.create_string(s).map_err(|e| e.to_string())?,
            ));
        }
        func.call::<()>(multi)
            .map_err(|e| format!("Lua callback '{}' error: {}", name, e))?;

        if let Ok(closed) = self.lua.globals().get::<bool>("__script_closed") {
            if closed {
                self.closed = true;
            }
        }
        self.drain_sprite_ops(state);
        self.sync_sprite_data(state);
        Ok(())
    }

    /// Call a Lua callback with mixed args: (string, int, int) for onTimerCompleted.
    pub fn call_callback_with_mixed(
        &mut self,
        name: &str,
        state: &mut ScriptState,
        tag: &str,
        loops: i32,
        left: i32,
    ) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.sync_shared_state(state);

        let func: Option<LuaFunction> = self.lua.globals().get(name).ok();
        let func = match func {
            Some(f) => f,
            None => return Ok(()),
        };

        let mut multi = mlua::MultiValue::new();
        multi.push_back(mlua::Value::String(
            self.lua.create_string(tag).map_err(|e| e.to_string())?,
        ));
        multi.push_back(mlua::Value::Integer(loops as i64));
        multi.push_back(mlua::Value::Integer(left as i64));
        func.call::<()>(multi)
            .map_err(|e| format!("Lua callback '{}' error: {}", name, e))?;

        if let Ok(closed) = self.lua.globals().get::<bool>("__script_closed") {
            if closed {
                self.closed = true;
            }
        }
        self.drain_sprite_ops(state);
        self.sync_sprite_data(state);
        Ok(())
    }

    /// Call a note-hit-style callback: (membersIndex, noteData, noteType, isSustainNote).
    pub fn call_note_callback(
        &mut self,
        name: &str,
        state: &mut ScriptState,
        members_index: usize,
        note_data: usize,
        note_type: &str,
        is_sustain: bool,
    ) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.sync_shared_state(state);

        let func: Option<LuaFunction> = self.lua.globals().get(name).ok();
        let func = match func {
            Some(f) => f,
            None => return Ok(()),
        };

        let mut multi = mlua::MultiValue::new();
        multi.push_back(mlua::Value::Integer(members_index as i64));
        multi.push_back(mlua::Value::Integer(note_data as i64));
        multi.push_back(mlua::Value::String(
            self.lua
                .create_string(note_type)
                .map_err(|e| e.to_string())?,
        ));
        multi.push_back(mlua::Value::Boolean(is_sustain));
        func.call::<()>(multi)
            .map_err(|e| format!("Lua callback '{}' error: {}", name, e))?;

        if let Ok(closed) = self.lua.globals().get::<bool>("__script_closed") {
            if closed {
                self.closed = true;
            }
        }
        self.drain_sprite_ops(state);
        self.sync_sprite_data(state);
        Ok(())
    }

    /// Populate __note_read_data Lua table with basic note info for modchart access.
    /// Each tuple is (strum_time, lane, must_press, sustain_length).
    pub fn populate_note_data(&self, notes: &[(f64, usize, bool, f64)]) {
        let globals = self.lua.globals();
        if let Ok(tbl) = self.lua.create_table() {
            for (i, &(strum_time, lane, must_press, sustain_length)) in notes.iter().enumerate() {
                if let Ok(entry) = self.lua.create_table() {
                    entry.set("strumTime", strum_time).ok();
                    entry.set("lane", lane as i64).ok();
                    entry.set("mustPress", must_press).ok();
                    entry.set("isSustainNote", sustain_length > 0.0).ok();
                    entry.set("sustainLength", sustain_length).ok();
                    tbl.set(i as i64 + 1, entry).ok();
                }
            }
            globals.set("__note_read_data", tbl).ok();
        }
        // Set the note count as a global for getProperty('unspawnNotes.length')
        globals.set("__unspawnNotesLength", notes.len() as i64).ok();
    }

    fn sync_string_set(&self, global_name: &str, values: &std::collections::HashSet<String>) {
        if let Ok(tbl) = self.lua.create_table() {
            for value in values {
                tbl.set(value.as_str(), true).ok();
            }
            self.lua.globals().set(global_name, tbl).ok();
        }
    }

    fn sync_number_map(&self, global_name: &str, values: &std::collections::HashMap<String, f64>) {
        if let Ok(tbl) = self.lua.create_table() {
            for (key, value) in values {
                tbl.set(key.as_str(), *value).ok();
            }
            self.lua.globals().set(global_name, tbl).ok();
        }
    }

    /// Transfer sprite creation/property data from Lua's internal tables to ScriptState.
    fn drain_sprite_ops(&self, state: &mut ScriptState) {
        let globals = self.lua.globals();

        // Drain __pending_sprites
        if let Ok(pending) = globals.get::<LuaTable>("__pending_sprites") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    if let (Ok(tag), Ok(kind), Ok(x), Ok(y)) = (
                        tbl.get::<String>("tag"),
                        tbl.get::<String>("kind"),
                        tbl.get::<f32>("x"),
                        tbl.get::<f32>("y"),
                    ) {
                        let sprite_kind = match kind.as_str() {
                            "image" => {
                                let image: String = tbl.get("image").unwrap_or_default();
                                crate::script_state::LuaSpriteKind::Image(image)
                            }
                            "graphic" => {
                                let w: i32 = tbl.get("width").unwrap_or(1);
                                let h: i32 = tbl.get("height").unwrap_or(1);
                                let color: String =
                                    tbl.get("color").unwrap_or_else(|_| "FFFFFF".to_string());
                                crate::script_state::LuaSpriteKind::Graphic {
                                    width: w,
                                    height: h,
                                    color,
                                }
                            }
                            "animated" => {
                                let image: String = tbl.get("image").unwrap_or_default();
                                crate::script_state::LuaSpriteKind::Animated(image)
                            }
                            _ => continue,
                        };
                        let mut sprite =
                            crate::script_state::LuaSprite::new(&tag, sprite_kind, x, y);
                        // Apply any properties set before addLuaSprite
                        if let Ok(sx) = tbl.get::<f32>("scale_x") {
                            sprite.scale_x = sx;
                        }
                        if let Ok(sy) = tbl.get::<f32>("scale_y") {
                            sprite.scale_y = sy;
                        }
                        if let Ok(sfx) = tbl.get::<f32>("scroll_x") {
                            sprite.scroll_x = sfx;
                        }
                        if let Ok(sfy) = tbl.get::<f32>("scroll_y") {
                            sprite.scroll_y = sfy;
                        }
                        if let Ok(a) = tbl.get::<f32>("alpha") {
                            sprite.alpha = a;
                        }
                        if let Ok(v) = tbl.get::<bool>("visible") {
                            sprite.visible = v;
                        }
                        if let Ok(f) = tbl.get::<bool>("flip_x") {
                            sprite.flip_x = f;
                        }
                        if let Ok(f) = tbl.get::<bool>("flip_y") {
                            sprite.flip_y = f;
                        }
                        if let Ok(aa) = tbl.get::<bool>("antialiasing") {
                            sprite.antialiasing = aa;
                        }
                        if let Ok(cam) = tbl.get::<String>("camera") {
                            sprite.camera = cam;
                        }
                        if let Ok(v) = tbl.get::<f32>("offset_x") {
                            sprite.offset_x = v;
                        }
                        if let Ok(v) = tbl.get::<f32>("offset_y") {
                            sprite.offset_y = v;
                        }
                        if let Ok(v) = tbl.get::<f32>("origin_x") {
                            sprite.origin_x = Some(v);
                        }
                        if let Ok(v) = tbl.get::<f32>("origin_y") {
                            sprite.origin_y = Some(v);
                        }
                        if let Ok(v) = tbl.get::<f32>("ct_red") {
                            sprite.color_red_offset = v;
                        }
                        if let Ok(v) = tbl.get::<f32>("ct_green") {
                            sprite.color_green_offset = v;
                        }
                        if let Ok(v) = tbl.get::<f32>("ct_blue") {
                            sprite.color_blue_offset = v;
                        }
                        if let Ok(color) = tbl.get::<String>("color") {
                            sprite.color = parse_lua_color(&color);
                        }
                        state.lua_sprites.insert(tag, sprite);
                    }
                }
            }
            // Clear pending
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_sprites", new_tbl).ok();
            }
        }

        // Drain __pending_adds
        if let Ok(pending) = globals.get::<LuaTable>("__pending_adds") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    if let (Ok(tag), Ok(in_front)) =
                        (tbl.get::<String>("tag"), tbl.get::<bool>("in_front"))
                    {
                        state.sprites_to_add.push((tag, in_front));
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_adds", new_tbl).ok();
            }
        }

        // Drain __pending_texts (text object creation)
        if let Ok(pending) = globals.get::<LuaTable>("__pending_texts") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    if let Ok(tag) = tbl.get::<String>("tag") {
                        let text = crate::script_state::LuaText::new(
                            &tag,
                            &tbl.get::<String>("text").unwrap_or_default(),
                            tbl.get::<f32>("width").unwrap_or(0.0),
                            tbl.get::<f32>("x").unwrap_or(0.0),
                            tbl.get::<f32>("y").unwrap_or(0.0),
                        );
                        let mut t = text;
                        if let Ok(v) = tbl.get::<f32>("alpha") {
                            t.alpha = v;
                        }
                        if let Ok(v) = tbl.get::<bool>("visible") {
                            t.visible = v;
                        }
                        if let Ok(v) = tbl.get::<f32>("angle") {
                            t.angle = v;
                        }
                        if let Ok(v) = tbl.get::<String>("font") {
                            t.font = v;
                        }
                        if let Ok(v) = tbl.get::<f32>("size") {
                            t.size = v;
                        }
                        if let Ok(v) = tbl.get::<String>("color") {
                            t.color = v;
                        }
                        if let Ok(v) = tbl.get::<f32>("border_size") {
                            t.border_size = v;
                        }
                        if let Ok(v) = tbl.get::<String>("border_color") {
                            t.border_color = v;
                        }
                        if let Ok(v) = tbl.get::<String>("alignment") {
                            t.alignment = v;
                        }
                        if let Ok(v) = tbl.get::<String>("camera") {
                            t.camera = v;
                        }
                        if let Ok(v) = tbl.get::<bool>("antialiasing") {
                            t.antialiasing = v;
                        }
                        state.lua_texts.insert(tag, t);
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_texts", new_tbl).ok();
            }
        }

        // Drain reflection-created Character instances.
        if let Ok(pending) = globals.get::<LuaTable>("__pending_character_instances") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    if let (Ok(tag), Ok(character)) =
                        (tbl.get::<String>("tag"), tbl.get::<String>("character"))
                    {
                        state.character_instances_to_create.push(
                            crate::script_state::CharacterInstanceCreate {
                                tag,
                                character,
                                x: tbl.get::<f32>("x").unwrap_or(0.0),
                                y: tbl.get::<f32>("y").unwrap_or(0.0),
                                is_player: tbl.get::<bool>("is_player").unwrap_or(false),
                            },
                        );
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_character_instances", new_tbl).ok();
            }
        }

        if let Ok(pending) = globals.get::<LuaTable>("__pending_character_adds") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    if let (Ok(tag), Ok(in_front)) =
                        (tbl.get::<String>("tag"), tbl.get::<bool>("in_front"))
                    {
                        state.character_instances_to_add.push((tag, in_front));
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_character_adds", new_tbl).ok();
            }
        }

        // Drain __pending_text_adds
        if let Ok(pending) = globals.get::<LuaTable>("__pending_text_adds") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    if let (Ok(tag), Ok(in_front)) =
                        (tbl.get::<String>("tag"), tbl.get::<bool>("in_front"))
                    {
                        state.texts_to_add.push((tag, in_front));
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_text_adds", new_tbl).ok();
            }
        }

        // Drain __pending_tween_cancels BEFORE creating new tweens.
        // Lua scripts call cancelTween() then startTween() with the same tag;
        // if we create first and cancel second, the new tween gets killed.
        if let Ok(pending) = globals.get::<LuaTable>("__pending_tween_cancels") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tag) = pending.get::<String>(i) {
                    state.tweens.cancel_tween(&tag);
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_tween_cancels", new_tbl).ok();
            }
        }

        // Drain __pending_tweens
        if let Ok(pending) = globals.get::<LuaTable>("__pending_tweens") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    if let (Ok(tag), Ok(target), Ok(property), Ok(value), Ok(duration), Ok(ease)) = (
                        tbl.get::<String>("tag"),
                        tbl.get::<String>("target"),
                        tbl.get::<String>("property"),
                        tbl.get::<f64>("value"),
                        tbl.get::<f64>("duration"),
                        tbl.get::<String>("ease"),
                    ) {
                        let prop = match property.as_str() {
                            "x" => crate::tweens::TweenProperty::X,
                            "y" => crate::tweens::TweenProperty::Y,
                            "alpha" => crate::tweens::TweenProperty::Alpha,
                            "angle" => crate::tweens::TweenProperty::Angle,
                            "zoom" => crate::tweens::TweenProperty::Zoom,
                            "scale_x" => crate::tweens::TweenProperty::ScaleX,
                            "scale_y" => crate::tweens::TweenProperty::ScaleY,
                            "red_offset" => crate::tweens::TweenProperty::RedOffset,
                            "green_offset" => crate::tweens::TweenProperty::GreenOffset,
                            "blue_offset" => crate::tweens::TweenProperty::BlueOffset,
                            "offset_x" => crate::tweens::TweenProperty::OffsetX,
                            "offset_y" => crate::tweens::TweenProperty::OffsetY,
                            _ => continue,
                        };
                        // Get current value as start value (explicit start overrides auto-detection).
                        // For strums, read from the Lua __strum_props table (not state.strum_props)
                        // because setPropertyFromGroup may have just updated the Lua table and
                        // state.strum_props hasn't been synced yet.
                        let start = if let Ok(s) = tbl.get::<f64>("start") {
                            s as f32
                        } else if let Some(si) = parse_strum_index(&target) {
                            if let Ok(strum_tbl) = globals.get::<LuaTable>("__strum_props") {
                                if let Ok(entry) = strum_tbl.get::<LuaTable>(si as i64 + 1) {
                                    match &prop {
                                        crate::tweens::TweenProperty::X => {
                                            entry.get::<f64>("x").unwrap_or(0.0) as f32
                                        }
                                        crate::tweens::TweenProperty::Y => {
                                            entry.get::<f64>("y").unwrap_or(0.0) as f32
                                        }
                                        crate::tweens::TweenProperty::Alpha => {
                                            entry.get::<f64>("alpha").unwrap_or(1.0) as f32
                                        }
                                        crate::tweens::TweenProperty::Angle => {
                                            entry.get::<f64>("angle").unwrap_or(0.0) as f32
                                        }
                                        crate::tweens::TweenProperty::ScaleX => {
                                            entry.get::<f64>("scale_x").unwrap_or(0.7) as f32
                                        }
                                        crate::tweens::TweenProperty::ScaleY => {
                                            entry.get::<f64>("scale_y").unwrap_or(0.7) as f32
                                        }
                                        _ => 0.0,
                                    }
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            }
                        } else if target.starts_with("__var_") {
                            let var_name = &target["__var_".len()..];
                            match state.custom_vars.get(var_name) {
                                Some(crate::script_state::LuaValue::Float(f)) => *f as f32,
                                Some(crate::script_state::LuaValue::Int(i)) => *i as f32,
                                _ => 0.0,
                            }
                        } else if matches!(
                            target.as_str(),
                            "dad" | "dadGroup" | "boyfriend" | "boyfriendGroup" | "gf" | "gfGroup"
                        ) {
                            // Character/group tween — read current position from synced state
                            let (cx, cy) = match target.as_str() {
                                "dad" => state.dad_pos,
                                "dadGroup" => state.dad_group_pos,
                                "boyfriend" => state.bf_pos,
                                "boyfriendGroup" => state.bf_group_pos,
                                "gf" => state.gf_pos,
                                "gfGroup" => state.gf_group_pos,
                                _ => (0.0, 0.0),
                            };
                            match &prop {
                                crate::tweens::TweenProperty::X => cx,
                                crate::tweens::TweenProperty::Y => cy,
                                crate::tweens::TweenProperty::Alpha => 1.0,
                                _ => 0.0,
                            }
                        } else {
                            let s = state.lua_sprites.get(&target);
                            match &prop {
                                crate::tweens::TweenProperty::X => s.map(|s| s.x).unwrap_or(0.0),
                                crate::tweens::TweenProperty::Y => s.map(|s| s.y).unwrap_or(0.0),
                                crate::tweens::TweenProperty::Alpha => {
                                    s.map(|s| s.alpha).unwrap_or(1.0)
                                }
                                crate::tweens::TweenProperty::Angle => {
                                    s.map(|s| s.angle).unwrap_or(0.0)
                                }
                                crate::tweens::TweenProperty::Zoom => state.camera_zoom,
                                crate::tweens::TweenProperty::ScaleX => {
                                    s.map(|s| s.scale_x).unwrap_or(1.0)
                                }
                                crate::tweens::TweenProperty::ScaleY => {
                                    s.map(|s| s.scale_y).unwrap_or(1.0)
                                }
                                crate::tweens::TweenProperty::RedOffset => {
                                    s.map(|s| s.color_red_offset).unwrap_or(0.0)
                                }
                                crate::tweens::TweenProperty::GreenOffset => {
                                    s.map(|s| s.color_green_offset).unwrap_or(0.0)
                                }
                                crate::tweens::TweenProperty::BlueOffset => {
                                    s.map(|s| s.color_blue_offset).unwrap_or(0.0)
                                }
                                crate::tweens::TweenProperty::OffsetX => {
                                    s.map(|s| s.offset_x).unwrap_or(0.0)
                                }
                                crate::tweens::TweenProperty::OffsetY => {
                                    s.map(|s| s.offset_y).unwrap_or(0.0)
                                }
                            }
                        };
                        let tween = crate::tweens::Tween {
                            tag: tag.clone(),
                            target,
                            property: prop,
                            start_value: start,
                            end_value: value as f32,
                            duration: duration as f32,
                            elapsed: 0.0,
                            ease: crate::tweens::EaseFunc::from_string(&ease),
                            finished: false,
                        };
                        state.tweens.add_tween(tween);
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_tweens", new_tbl).ok();
            }
        }

        // Drain __pending_timers
        if let Ok(pending) = globals.get::<LuaTable>("__pending_timers") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    if let (Ok(tag), Ok(duration), Ok(loops)) = (
                        tbl.get::<String>("tag"),
                        tbl.get::<f64>("duration"),
                        tbl.get::<i32>("loops"),
                    ) {
                        let timer = crate::tweens::LuaTimer {
                            tag: tag.clone(),
                            duration: duration as f32,
                            elapsed: 0.0,
                            loops_total: loops,
                            loops_done: 0,
                            finished: false,
                        };
                        state.tweens.add_timer(timer);
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_timers", new_tbl).ok();
            }
        }

        // Drain __pending_timer_cancels
        if let Ok(pending) = globals.get::<LuaTable>("__pending_timer_cancels") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tag) = pending.get::<String>(i) {
                    state.tweens.cancel_timer(&tag);
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_timer_cancels", new_tbl).ok();
            }
        }

        // Drain __pending_prop_writes (game-level property writes + rustic extension commands)
        if let Ok(pending) = globals.get::<LuaTable>("__pending_props") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    // Check for rustic extension type first
                    if let Ok(ty) = tbl.get::<String>("type") {
                        match ty.as_str() {
                            "stage_color" => {
                                let side: String = tbl.get("side").unwrap_or_default();
                                let r: f32 = tbl.get("r").unwrap_or(0.0);
                                let g: f32 = tbl.get("g").unwrap_or(0.0);
                                let b: f32 = tbl.get("b").unwrap_or(0.0);
                                let a: f32 = tbl.get("a").unwrap_or(1.0);
                                let dur: f32 = tbl.get("duration").unwrap_or(0.3);
                                state.stage_color_requests.push((side, r, g, b, a, dur));
                            }
                            "stage_color_swap" => {
                                let dur: f32 = tbl.get("duration").unwrap_or(0.15);
                                state.stage_color_swap_requests.push(dur);
                            }
                            "stage_lights" => {
                                let on: bool = tbl.get("on").unwrap_or(true);
                                state.stage_lights_request = Some(on);
                            }
                            "postprocess" => {
                                let enabled: bool = tbl.get("enabled").unwrap_or(false);
                                let dur: f32 = tbl.get("duration").unwrap_or(1.0);
                                state.postprocess_requests.push((enabled, dur));
                            }
                            "postprocess_param" => {
                                let param: String = tbl.get("param").unwrap_or_default();
                                let value: f32 = tbl.get("value").unwrap_or(0.0);
                                state.postprocess_param_requests.push((param, value));
                            }
                            "healthbar_color" => {
                                let side: String = tbl.get("side").unwrap_or_default();
                                let r: f32 = tbl.get("r").unwrap_or(0.0);
                                let g: f32 = tbl.get("g").unwrap_or(0.0);
                                let b: f32 = tbl.get("b").unwrap_or(0.0);
                                let a: f32 = tbl.get("a").unwrap_or(1.0);
                                let dur: f32 = tbl.get("duration").unwrap_or(1.0);
                                state.healthbar_color_requests.push((side, r, g, b, a, dur));
                            }
                            "reflections" => {
                                let enabled: bool = tbl.get("enabled").unwrap_or(false);
                                state.reflections_request = Some(enabled);
                            }
                            "video" => {
                                let filename: String = tbl.get("filename").unwrap_or_default();
                                let callback: Option<String> = tbl.get("callback").ok();
                                // Lua-initiated videos (e.g. pre-song intros) block gameplay by default.
                                let blocks_gameplay: bool =
                                    tbl.get("blocks_gameplay").unwrap_or(true);
                                if !filename.is_empty() {
                                    state.video_requests.push((
                                        filename,
                                        callback,
                                        blocks_gameplay,
                                    ));
                                }
                            }
                            "add_script" => {
                                let script_name: String =
                                    tbl.get("script_name").unwrap_or_default();
                                if !script_name.is_empty() {
                                    state.script_load_requests.push(script_name);
                                }
                            }
                            "remove_script" => {
                                let script_name: String =
                                    tbl.get("script_name").unwrap_or_default();
                                if !script_name.is_empty() {
                                    state.script_remove_requests.push(script_name);
                                }
                            }
                            "call_script" | "call_luas" => {
                                let function: String = tbl.get("function").unwrap_or_default();
                                if function.is_empty() {
                                    continue;
                                }
                                let target = if ty == "call_script" {
                                    tbl.get::<String>("target").ok().filter(|s| !s.is_empty())
                                } else {
                                    None
                                };
                                let args = tbl
                                    .get::<LuaTable>("args")
                                    .ok()
                                    .map(|args| table_to_script_values(&args))
                                    .unwrap_or_default();
                                state.script_call_requests.push(
                                    crate::script_state::ScriptCallRequest {
                                        target,
                                        function,
                                        args,
                                    },
                                );
                            }
                            "set_global" => {
                                let name: String = tbl.get("name").unwrap_or_default();
                                if !name.is_empty() {
                                    let value = tbl_to_lua_value(&tbl, "value");
                                    state.script_global_sets.push((name.clone(), value.clone()));
                                    state.custom_vars.insert(name, value);
                                }
                            }
                            "song_control" => {
                                let action: String = tbl.get("action").unwrap_or_default();
                                match action.as_str() {
                                    "start_countdown" => state.control_requests.push(
                                        crate::script_state::SongControlRequest::StartCountdown,
                                    ),
                                    "end_song" => state
                                        .control_requests
                                        .push(crate::script_state::SongControlRequest::EndSong),
                                    "exit_song" => state
                                        .control_requests
                                        .push(crate::script_state::SongControlRequest::ExitSong),
                                    "restart_song" => state
                                        .control_requests
                                        .push(crate::script_state::SongControlRequest::RestartSong),
                                    "load_song" => {
                                        let song: String = tbl.get("song").unwrap_or_default();
                                        let difficulty: Option<i32> = tbl.get("difficulty").ok();
                                        if !song.is_empty() {
                                            state.control_requests.push(
                                                crate::script_state::SongControlRequest::LoadSong {
                                                    song,
                                                    difficulty,
                                                },
                                            );
                                        }
                                    }
                                    "start_dialogue" => {
                                        let dialogue: String =
                                            tbl.get("dialogue").unwrap_or_default();
                                        let music: Option<String> = tbl.get("music").ok();
                                        state.control_requests.push(
                                            crate::script_state::SongControlRequest::StartDialogue {
                                                dialogue,
                                                music,
                                            },
                                        );
                                    }
                                    "open_substate" => {
                                        let name: String = tbl.get("name").unwrap_or_default();
                                        let pause_game: bool = tbl.get("pause_game").unwrap_or(true);
                                        state.control_requests.push(
                                            crate::script_state::SongControlRequest::OpenCustomSubstate {
                                                name,
                                                pause_game,
                                            },
                                        );
                                    }
                                    "close_substate" => state.control_requests.push(
                                        crate::script_state::SongControlRequest::CloseCustomSubstate,
                                    ),
                                    _ => {}
                                }
                            }
                            "precache" => {
                                let kind: String = tbl.get("kind").unwrap_or_default();
                                let name: String = tbl.get("name").unwrap_or_default();
                                if !name.is_empty() {
                                    match kind.as_str() {
                                        "image" => state.precache_requests.push(
                                            crate::script_state::PrecacheRequest::Image {
                                                name,
                                                allow_gpu: tbl.get("allow_gpu").unwrap_or(true),
                                            },
                                        ),
                                        "sound" => state.precache_requests.push(
                                            crate::script_state::PrecacheRequest::Sound { name },
                                        ),
                                        "music" => state.precache_requests.push(
                                            crate::script_state::PrecacheRequest::Music { name },
                                        ),
                                        "character" => state.precache_requests.push(
                                            crate::script_state::PrecacheRequest::Character {
                                                name,
                                                character_type: tbl
                                                    .get("character_type")
                                                    .unwrap_or_else(|_| "dad".to_string()),
                                            },
                                        ),
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    } else if let Ok(prop) = tbl.get::<String>("prop") {
                        let val = tbl_to_lua_value(&tbl, "value");
                        state.property_writes.push((prop, val));
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_props", new_tbl).ok();
            }
        }

        // Drain __pending_removes
        if let Ok(pending) = globals.get::<LuaTable>("__pending_removes") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tag) = pending.get::<String>(i) {
                    state.sprites_to_remove.push(tag);
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_removes", new_tbl).ok();
            }
        }

        // Drain __pending_cam_targets
        if let Ok(pending) = globals.get::<LuaTable>("__pending_cam_targets") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(target) = pending.get::<String>(i) {
                    state.camera_target_requests.push(target);
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_cam_targets", new_tbl).ok();
            }
        }

        // Drain __pending_events (triggerEvent calls)
        if let Ok(pending) = globals.get::<LuaTable>("__pending_events") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    if let (Ok(name), Ok(v1), Ok(v2)) = (
                        tbl.get::<String>("name"),
                        tbl.get::<String>("v1"),
                        tbl.get::<String>("v2"),
                    ) {
                        state.triggered_events.push((name, v1, v2));
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_events", new_tbl).ok();
            }
        }

        // Drain __pending_cam_sections (moveCameraSection requests)
        if let Ok(pending) = globals.get::<LuaTable>("__pending_cam_sections") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(section) = pending.get::<i32>(i) {
                    state.camera_section_requests.push(section);
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_cam_sections", new_tbl).ok();
            }
        }

        // Drain __pending_char_positions (runHaxeCode character position adjustments)
        if let Ok(pending) = globals.get::<LuaTable>("__pending_char_positions") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    let character: String = tbl.get("character").unwrap_or_default();
                    let field: String = tbl.get("field").unwrap_or_default();
                    let value: f64 = tbl.get("value").unwrap_or(0.0);
                    state
                        .char_position_adjustments
                        .push((character, field, value));
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_char_positions", new_tbl).ok();
            }
        }

        // Drain __pending_audio (playMusic / pauseSounds / setSoundTime)
        if let Ok(pending) = globals.get::<LuaTable>("__pending_audio") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    let kind: String = tbl.get("kind").unwrap_or_default();
                    match kind.as_str() {
                        "play_sound" => {
                            let path: String = tbl.get("path").unwrap_or_default();
                            let volume: f64 = tbl.get("volume").unwrap_or(1.0);
                            let tag: Option<String> = tbl.get("tag").ok();
                            let looping: bool = tbl.get("looping").unwrap_or(false);
                            state.audio_requests.push(
                                crate::script_state::AudioRequest::PlaySound {
                                    path,
                                    volume,
                                    tag,
                                    looping,
                                },
                            );
                        }
                        "play_music" => {
                            let path: String = tbl.get("path").unwrap_or_default();
                            let volume: f64 = tbl.get("volume").unwrap_or(1.0);
                            let looping: bool = tbl.get("looping").unwrap_or(true);
                            state.audio_requests.push(
                                crate::script_state::AudioRequest::PlayMusic {
                                    path,
                                    volume,
                                    looping,
                                },
                            );
                        }
                        "stop_music" => state
                            .audio_requests
                            .push(crate::script_state::AudioRequest::StopMusic),
                        "pause_music" => state
                            .audio_requests
                            .push(crate::script_state::AudioRequest::PauseMusic),
                        "resume_music" => state
                            .audio_requests
                            .push(crate::script_state::AudioRequest::ResumeMusic),
                        "set_music_volume" => {
                            let volume: f64 = tbl.get("volume").unwrap_or(1.0);
                            state
                                .audio_requests
                                .push(crate::script_state::AudioRequest::SetMusicVolume(volume));
                        }
                        "set_music_time" => {
                            let time: f64 = tbl.get("time").unwrap_or(0.0);
                            state
                                .audio_requests
                                .push(crate::script_state::AudioRequest::SetMusicTime(time));
                        }
                        "stop_sound" => {
                            let tag: Option<String> = tbl.get("tag").ok();
                            state
                                .audio_requests
                                .push(crate::script_state::AudioRequest::StopSound(tag));
                        }
                        "pause_sound" => {
                            let tag: Option<String> = tbl.get("tag").ok();
                            state
                                .audio_requests
                                .push(crate::script_state::AudioRequest::PauseSound(tag));
                        }
                        "resume_sound" => {
                            let tag: Option<String> = tbl.get("tag").ok();
                            state
                                .audio_requests
                                .push(crate::script_state::AudioRequest::ResumeSound(tag));
                        }
                        "set_sound_volume" => {
                            let tag: Option<String> = tbl.get("tag").ok();
                            let volume: f64 = tbl.get("volume").unwrap_or(1.0);
                            state.audio_requests.push(
                                crate::script_state::AudioRequest::SetSoundVolume { tag, volume },
                            );
                        }
                        "set_sound_time" => {
                            let tag: Option<String> = tbl.get("tag").ok();
                            let time: f64 = tbl.get("time").unwrap_or(0.0);
                            state.audio_requests.push(
                                crate::script_state::AudioRequest::SetSoundTime { tag, time },
                            );
                        }
                        "sound_fade" => {
                            let tag: Option<String> = tbl.get("tag").ok();
                            let from: Option<f64> = tbl.get("from").ok();
                            let to: f64 = tbl.get("to").unwrap_or(1.0);
                            let duration: f64 = tbl.get("duration").unwrap_or(0.0);
                            let stop_when_done: bool = tbl.get("stop_when_done").unwrap_or(false);
                            state.audio_requests.push(
                                crate::script_state::AudioRequest::SoundFade {
                                    tag,
                                    from,
                                    to,
                                    duration,
                                    stop_when_done,
                                },
                            );
                        }
                        "set_sound_pitch" => {
                            let tag: Option<String> = tbl.get("tag").ok();
                            let pitch: f64 = tbl.get("pitch").unwrap_or(1.0);
                            state.audio_requests.push(
                                crate::script_state::AudioRequest::SetSoundPitch { tag, pitch },
                            );
                        }
                        _ => {}
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_audio", new_tbl).ok();
            }
        }

        // Drain __pending_cam_fx (camera shake/flash requests)
        if let Ok(pending) = globals.get::<LuaTable>("__pending_cam_fx") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    let kind: String = tbl.get("kind").unwrap_or_default();
                    let camera: String =
                        tbl.get("camera").unwrap_or_else(|_| "camGame".to_string());
                    match kind.as_str() {
                        "shake" => {
                            let intensity = tbl.get::<f64>("intensity").unwrap_or(0.0) as f32;
                            let duration = tbl.get::<f64>("duration").unwrap_or(0.0) as f32;
                            state
                                .camera_shake_requests
                                .push((camera, intensity, duration));
                        }
                        "flash" => {
                            let color: String =
                                tbl.get("color").unwrap_or_else(|_| "FFFFFF".to_string());
                            let duration = tbl.get::<f64>("duration").unwrap_or(0.5) as f32;
                            let alpha = tbl.get::<f64>("alpha").unwrap_or(1.0) as f32;
                            state
                                .camera_flash_requests
                                .push((camera, color, duration, alpha));
                        }
                        "fade" => {
                            let color: String =
                                tbl.get("color").unwrap_or_else(|_| "000000".to_string());
                            let duration = tbl.get::<f64>("duration").unwrap_or(0.5) as f32;
                            let fade_in = tbl.get::<bool>("fade_in").unwrap_or(false);
                            state
                                .camera_fade_requests
                                .push((camera, color, duration, fade_in));
                        }
                        _ => {}
                    }
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_cam_fx", new_tbl).ok();
            }
        }

        // Drain __pending_subtitles
        if let Ok(pending) = globals.get::<LuaTable>("__pending_subtitles") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    let text: String = tbl.get("text").unwrap_or_default();
                    let font: String = tbl.get("font").unwrap_or_default();
                    let color: String = tbl.get("color").unwrap_or_else(|_| "FFFFFF".to_string());
                    let size = tbl.get::<f64>("size").unwrap_or(32.0) as f32;
                    let duration = tbl.get::<f64>("duration").unwrap_or(3.0) as f32;
                    let border: String = tbl.get("border").unwrap_or_else(|_| "000000".to_string());
                    state
                        .subtitle_requests
                        .push((text, font, color, size, duration, border));
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_subtitles", new_tbl).ok();
            }
        }

        // Sync __strum_props table to Rust state
        // Skip properties that have active tweens (tweens are authoritative when running).
        if let Ok(strum_tbl) = globals.get::<LuaTable>("__strum_props") {
            for i in 0..8 {
                if let Ok(tbl) = strum_tbl.get::<LuaTable>(i as i64 + 1) {
                    let custom: bool = tbl.get("custom").unwrap_or(false);
                    if custom {
                        let strum_name = if i < 4 {
                            format!("__strum_opponent_{}", i)
                        } else {
                            format!("__strum_player_{}", i - 4)
                        };
                        let has_active_tween = |prop: &crate::tweens::TweenProperty| -> bool {
                            // Include finished tweens still pending removal — their
                            // final value is authoritative until they're cleaned up.
                            state
                                .tweens
                                .tweens
                                .values()
                                .any(|t| t.target == strum_name && &t.property == prop)
                        };
                        if !has_active_tween(&crate::tweens::TweenProperty::X) {
                            state.strum_props[i].x = tbl.get::<f64>("x").unwrap_or(0.0) as f32;
                        }
                        if !has_active_tween(&crate::tweens::TweenProperty::Y) {
                            state.strum_props[i].y = tbl.get::<f64>("y").unwrap_or(0.0) as f32;
                        }
                        if !has_active_tween(&crate::tweens::TweenProperty::Alpha) {
                            state.strum_props[i].alpha =
                                tbl.get::<f64>("alpha").unwrap_or(1.0) as f32;
                        }
                        if !has_active_tween(&crate::tweens::TweenProperty::Angle) {
                            state.strum_props[i].angle =
                                tbl.get::<f64>("angle").unwrap_or(0.0) as f32;
                        }
                        if !has_active_tween(&crate::tweens::TweenProperty::ScaleX) {
                            state.strum_props[i].scale_x =
                                tbl.get::<f64>("scale_x").unwrap_or(0.7) as f32;
                        }
                        if !has_active_tween(&crate::tweens::TweenProperty::ScaleY) {
                            state.strum_props[i].scale_y =
                                tbl.get::<f64>("scale_y").unwrap_or(0.7) as f32;
                        }
                        state.strum_props[i].down_scroll = tbl.get::<bool>("downScroll").ok();
                        state.strum_props[i].custom = true;
                    }
                }
            }
        }

        // Drain __dirty_notes and sync note overrides to ScriptState
        if let Ok(dirty) = globals.get::<LuaTable>("__dirty_notes") {
            if let Ok(overrides) = globals.get::<LuaTable>("__note_overrides") {
                for pair in dirty.pairs::<i64, bool>() {
                    let Ok((lua_idx, true)) = pair else { continue };
                    let note_idx = (lua_idx - 1) as usize;
                    if let Ok(note_tbl) = overrides.get::<LuaTable>(lua_idx) {
                        let entry = state
                            .note_overrides
                            .entry(note_idx)
                            .or_insert_with(std::collections::HashMap::new);
                        for field_pair in note_tbl.pairs::<String, LuaValue>() {
                            let Ok((field, val)) = field_pair else {
                                continue;
                            };
                            let num = match &val {
                                LuaValue::Number(n) => *n,
                                LuaValue::Integer(n) => *n as f64,
                                LuaValue::Boolean(b) => {
                                    if *b {
                                        1.0
                                    } else {
                                        0.0
                                    }
                                }
                                _ => continue,
                            };
                            entry.insert(field, num);
                        }
                    }
                }
            }
            // Clear dirty set
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__dirty_notes", new_tbl).ok();
            }
        }

        // Sync custom variables from this script's __custom_vars to shared state
        if let Ok(custom) = globals.get::<LuaTable>("__custom_vars") {
            for pair in custom.pairs::<String, LuaValue>() {
                if let Ok((key, val)) = pair {
                    let lua_val = match val {
                        LuaValue::Number(n) => crate::script_state::LuaValue::Float(n),
                        LuaValue::Integer(n) => crate::script_state::LuaValue::Int(n),
                        LuaValue::Boolean(b) => crate::script_state::LuaValue::Bool(b),
                        LuaValue::String(s) => {
                            crate::script_state::LuaValue::String(s.to_string_lossy().to_string())
                        }
                        _ => continue,
                    };
                    state.custom_vars.insert(key, lua_val);
                }
            }
        }

        // Drain __pending_note_types (registerNoteType calls)
        if let Ok(pending) = globals.get::<LuaTable>("__pending_note_types") {
            let len = pending.len().unwrap_or(0);
            for i in 1..=len {
                if let Ok(tbl) = pending.get::<LuaTable>(i) {
                    let name: String = tbl.get("name").unwrap_or_default();
                    if name.is_empty() {
                        continue;
                    }

                    // Parse animation arrays (4 directions each)
                    let parse_anims = |key: &str| -> Option<[String; 4]> {
                        let arr: LuaTable = tbl.get(key).ok()?;
                        let a: String = arr.get(1).ok()?;
                        let b: String = arr.get(2).ok()?;
                        let c: String = arr.get(3).ok()?;
                        let d: String = arr.get(4).ok()?;
                        Some([a, b, c, d])
                    };

                    state
                        .note_type_registrations
                        .push(crate::script_state::NoteTypeRegistration {
                            name,
                            hit_causes_miss: tbl.get::<bool>("hitCausesMiss").unwrap_or(false),
                            hit_damage: tbl.get::<f64>("hitDamage").unwrap_or(0.0) as f32,
                            ignore_miss: tbl.get::<bool>("ignoreMiss").unwrap_or(false),
                            note_skin: tbl.get::<String>("noteSkin").ok(),
                            hit_sfx: tbl.get::<String>("hitSfx").ok(),
                            health_drain_pct: tbl.get::<f64>("healthDrainPct").unwrap_or(0.0)
                                as f32,
                            drain_death_safe: tbl.get::<bool>("drainDeathSafe").unwrap_or(false),
                            note_anims: parse_anims("noteAnims"),
                            strum_anims: parse_anims("strumAnims"),
                            confirm_anims: parse_anims("confirmAnims"),
                        });
                }
            }
            if let Ok(new_tbl) = self.lua.create_table() {
                globals.set("__pending_note_types", new_tbl).ok();
            }
        }
    }

    /// Sync all sprite properties from Lua `__sprite_data` tables to Rust `LuaSprite` structs.
    /// Called after each callback so that `setProperty` changes are reflected in rendering.
    fn sync_sprite_data(&self, state: &mut ScriptState) {
        let globals = self.lua.globals();
        let Ok(sprite_data) = globals.get::<LuaTable>("__sprite_data") else {
            return;
        };

        for pair in sprite_data.pairs::<String, LuaTable>() {
            let Ok((tag, tbl)) = pair else { continue };
            let Some(sprite) = state.lua_sprites.get_mut(&tag) else {
                continue;
            };

            // Skip properties that have active tweens (tweens are authoritative when running).
            let has_tween = |prop: &crate::tweens::TweenProperty| -> bool {
                state
                    .tweens
                    .tweens
                    .values()
                    .any(|t| !t.finished && t.target == tag && &t.property == prop)
            };

            if !has_tween(&crate::tweens::TweenProperty::X) {
                if let Ok(x) = tbl.get::<f32>("x") {
                    sprite.x = x;
                }
            }
            if !has_tween(&crate::tweens::TweenProperty::Y) {
                if let Ok(y) = tbl.get::<f32>("y") {
                    sprite.y = y;
                }
            }
            if !has_tween(&crate::tweens::TweenProperty::Alpha) {
                if let Ok(a) = tbl.get::<f32>("alpha") {
                    sprite.alpha = a;
                }
            }
            if let Ok(v) = tbl.get::<bool>("visible") {
                sprite.visible = v;
            }
            if !has_tween(&crate::tweens::TweenProperty::Angle) {
                if let Ok(a) = tbl.get::<f32>("angle") {
                    sprite.angle = a;
                }
            }
            if !has_tween(&crate::tweens::TweenProperty::ScaleX) {
                if let Ok(sx) = tbl.get::<f32>("scale_x") {
                    sprite.scale_x = sx;
                }
            }
            if !has_tween(&crate::tweens::TweenProperty::ScaleY) {
                if let Ok(sy) = tbl.get::<f32>("scale_y") {
                    sprite.scale_y = sy;
                }
            }
            if let Ok(sfx) = tbl.get::<f32>("scroll_x") {
                sprite.scroll_x = sfx;
            }
            if let Ok(sfy) = tbl.get::<f32>("scroll_y") {
                sprite.scroll_y = sfy;
            }
            if let Ok(f) = tbl.get::<bool>("flip_x") {
                sprite.flip_x = f;
            }
            if let Ok(f) = tbl.get::<bool>("flip_y") {
                sprite.flip_y = f;
            }
            if let Ok(aa) = tbl.get::<bool>("antialiasing") {
                sprite.antialiasing = aa;
            }
            if let Ok(cam) = tbl.get::<String>("camera") {
                sprite.camera = cam;
            }
            if !has_tween(&crate::tweens::TweenProperty::OffsetX) {
                if let Ok(v) = tbl.get::<f32>("offset_x") {
                    sprite.offset_x = v;
                }
            }
            if !has_tween(&crate::tweens::TweenProperty::OffsetY) {
                if let Ok(v) = tbl.get::<f32>("offset_y") {
                    sprite.offset_y = v;
                }
            }
            if let Ok(v) = tbl.get::<f32>("origin_x") {
                sprite.origin_x = Some(v);
            }
            if let Ok(v) = tbl.get::<f32>("origin_y") {
                sprite.origin_y = Some(v);
            }
            if !has_tween(&crate::tweens::TweenProperty::RedOffset) {
                if let Ok(v) = tbl.get::<f32>("ct_red") {
                    sprite.color_red_offset = v;
                }
            }
            if !has_tween(&crate::tweens::TweenProperty::GreenOffset) {
                if let Ok(v) = tbl.get::<f32>("ct_green") {
                    sprite.color_green_offset = v;
                }
            }
            if !has_tween(&crate::tweens::TweenProperty::BlueOffset) {
                if let Ok(v) = tbl.get::<f32>("ct_blue") {
                    sprite.color_blue_offset = v;
                }
            }
            if let Ok(color) = tbl.get::<String>("color") {
                sprite.color = parse_lua_color(&color);
            }
            if let Ok(frame) = tbl.get::<i64>("anim_frame") {
                sprite.anim_frame = frame.max(0) as usize;
                sprite.anim_finished = false;
            }
            if let Ok(fps) = tbl.get::<f32>("anim_fps") {
                sprite.anim_fps = fps.max(0.0);
            }
            if let Ok(finished) = tbl.get::<bool>("anim_finished") {
                sprite.anim_finished = finished;
            }
            if let Ok(anim_tbl) = tbl.get::<LuaTable>("animation") {
                if let Ok(name) = anim_tbl.get::<String>("name") {
                    if !name.is_empty() && sprite.current_anim != name {
                        sprite.current_anim = name;
                        sprite.anim_frame = 0;
                        sprite.anim_timer = 0.0;
                        sprite.anim_finished = false;
                    }
                }
                if let Ok(finished) = anim_tbl.get::<bool>("finished") {
                    sprite.anim_finished = finished;
                }
                if let Ok(cur_anim) = anim_tbl.get::<LuaTable>("curAnim") {
                    if let Ok(name) = cur_anim.get::<String>("name") {
                        if !name.is_empty() && sprite.current_anim != name {
                            sprite.current_anim = name;
                            sprite.anim_frame = 0;
                            sprite.anim_timer = 0.0;
                            sprite.anim_finished = false;
                        }
                    }
                    if let Ok(frame) = cur_anim.get::<i64>("curFrame") {
                        sprite.anim_frame = frame.max(0) as usize;
                        sprite.anim_finished = false;
                    }
                    if let Ok(finished) = cur_anim.get::<bool>("finished") {
                        sprite.anim_finished = finished;
                    }
                }
            }

            // Sync animation definitions from __anims subtable
            if let Ok(anims) = tbl.get::<LuaTable>("__anims") {
                for anim_pair in anims.pairs::<String, LuaTable>() {
                    let Ok((name, def)) = anim_pair else { continue };
                    if sprite.animations.contains_key(&name) {
                        continue;
                    }
                    let prefix: String = def.get("prefix").unwrap_or_default();
                    let fps: f32 = def.get::<f64>("fps").unwrap_or(24.0) as f32;
                    let looping: bool = def.get("looping").unwrap_or(true);
                    let mut indices = Vec::new();
                    if let Ok(idx_tbl) = def.get::<LuaTable>("indices") {
                        let len = idx_tbl.len().unwrap_or(0);
                        for i in 1..=len {
                            if let Ok(v) = idx_tbl.get::<i32>(i) {
                                indices.push(v);
                            }
                        }
                    }
                    sprite.animations.insert(
                        name.clone(),
                        crate::script_state::LuaAnimDef {
                            prefix,
                            fps,
                            looping,
                            indices,
                        },
                    );
                    if sprite.current_anim.is_empty() {
                        sprite.current_anim = name.clone();
                        sprite.anim_frame = 0;
                        sprite.anim_timer = 0.0;
                        sprite.anim_fps = fps;
                        sprite.anim_looping = looping;
                        sprite.anim_finished = false;
                    }
                }
            }

            // Sync animation offsets from __offsets subtable
            if let Ok(offsets) = tbl.get::<LuaTable>("__offsets") {
                for off_pair in offsets.pairs::<String, LuaTable>() {
                    let Ok((name, off)) = off_pair else { continue };
                    let x: f32 = off.get::<f64>("x").unwrap_or(0.0) as f32;
                    let y: f32 = off.get::<f64>("y").unwrap_or(0.0) as f32;
                    sprite.anim_offsets.insert(name, (x, y));
                }
            }

            // Process pending animation play commands
            if let Ok(pending) = tbl.get::<LuaTable>("__pending_anim") {
                let anim: String = pending.get("anim").unwrap_or_default();
                let forced: bool = pending.get("forced").unwrap_or(false);
                let start_frame = pending.get::<i64>("frame").ok();
                if !anim.is_empty() {
                    if forced
                        || sprite.current_anim != anim
                        || sprite.anim_finished
                        || start_frame.is_some()
                    {
                        sprite.current_anim = anim;
                        sprite.anim_frame =
                            start_frame.map(|frame| frame.max(0) as usize).unwrap_or(0);
                        sprite.anim_timer = 0.0;
                        sprite.anim_finished = false;
                        if let Some(def) = sprite.animations.get(&sprite.current_anim) {
                            sprite.anim_fps = def.fps;
                            sprite.anim_looping = def.looping;
                        }
                    }
                }
                tbl.set("__pending_anim", mlua::Value::Nil).ok();
            }
        }

        // Sync text object properties from Lua __text_data to Rust LuaText structs
        let Ok(text_data) = globals.get::<LuaTable>("__text_data") else {
            return;
        };
        for pair in text_data.pairs::<String, LuaTable>() {
            let Ok((tag, tbl)) = pair else { continue };
            let Some(text) = state.lua_texts.get_mut(&tag) else {
                continue;
            };
            if let Ok(v) = tbl.get::<String>("text") {
                text.text = v;
            }
            if let Ok(v) = tbl.get::<f32>("x") {
                text.x = v;
            }
            if let Ok(v) = tbl.get::<f32>("y") {
                text.y = v;
            }
            if let Ok(v) = tbl.get::<f32>("alpha") {
                text.alpha = v;
            }
            if let Ok(v) = tbl.get::<bool>("visible") {
                text.visible = v;
            }
            if let Ok(v) = tbl.get::<f32>("angle") {
                text.angle = v;
            }
            if let Ok(v) = tbl.get::<String>("font") {
                text.font = v;
            }
            if let Ok(v) = tbl.get::<f32>("size") {
                text.size = v;
            }
            if let Ok(v) = tbl.get::<String>("color") {
                text.color = v;
            }
            if let Ok(v) = tbl.get::<f32>("border_size") {
                text.border_size = v;
            }
            if let Ok(v) = tbl.get::<String>("border_color") {
                text.border_color = v;
            }
            if let Ok(v) = tbl.get::<String>("alignment") {
                text.alignment = v;
            }
            if let Ok(v) = tbl.get::<String>("camera") {
                text.camera = v;
            }
            if let Ok(v) = tbl.get::<bool>("antialiasing") {
                text.antialiasing = v;
            }
        }
    }
}

fn table_to_script_values(tbl: &LuaTable) -> Vec<crate::script_state::LuaValue> {
    let len = tbl.len().unwrap_or(0);
    let mut values = Vec::with_capacity(len as usize);
    for i in 1..=len {
        values.push(lua_value_to_script_value(
            tbl.get::<LuaValue>(i).unwrap_or(LuaValue::Nil),
        ));
    }
    values
}

fn lua_value_to_script_value(value: LuaValue) -> crate::script_state::LuaValue {
    match value {
        LuaValue::Nil => crate::script_state::LuaValue::Nil,
        LuaValue::Boolean(v) => crate::script_state::LuaValue::Bool(v),
        LuaValue::Integer(v) => crate::script_state::LuaValue::Int(v),
        LuaValue::Number(v) => crate::script_state::LuaValue::Float(v),
        LuaValue::String(v) => {
            crate::script_state::LuaValue::String(v.to_string_lossy().to_string())
        }
        LuaValue::Table(tbl) => crate::script_state::LuaValue::Array(table_to_script_values(&tbl)),
        _ => crate::script_state::LuaValue::Nil,
    }
}

fn script_value_to_lua(lua: &Lua, value: &crate::script_state::LuaValue) -> LuaResult<LuaValue> {
    match value {
        crate::script_state::LuaValue::Nil => Ok(LuaValue::Nil),
        crate::script_state::LuaValue::Bool(v) => Ok(LuaValue::Boolean(*v)),
        crate::script_state::LuaValue::Int(v) => Ok(LuaValue::Integer(*v)),
        crate::script_state::LuaValue::Float(v) => Ok(LuaValue::Number(*v)),
        crate::script_state::LuaValue::String(v) => Ok(LuaValue::String(lua.create_string(v)?)),
        crate::script_state::LuaValue::Array(values) => {
            let tbl = lua.create_table()?;
            for (i, item) in values.iter().enumerate() {
                tbl.set(i + 1, script_value_to_lua(lua, item)?)?;
            }
            Ok(LuaValue::Table(tbl))
        }
    }
}

fn parse_lua_color(color: &str) -> [u8; 3] {
    let normalized = color
        .trim()
        .trim_start_matches('#')
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    let hex = if normalized.len() >= 6 {
        &normalized[normalized.len() - 6..]
    } else {
        normalized
    };
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return [r, g, b];
        }
    }
    match normalized.to_ascii_uppercase().as_str() {
        "BLACK" => [0, 0, 0],
        "RED" => [255, 0, 0],
        "GREEN" => [0, 255, 0],
        "BLUE" => [0, 0, 255],
        _ => [255, 255, 255],
    }
}

pub(crate) fn script_target_matches(path: &Path, script_name: &str, target: &str) -> bool {
    fn normalize(value: &str) -> String {
        value
            .replace('\\', "/")
            .trim_start_matches("./")
            .trim_end_matches(".lua")
            .trim_end_matches(".hx")
            .to_ascii_lowercase()
    }

    let target = normalize(target);
    if target.is_empty() {
        return false;
    }

    let path = normalize(&path.to_string_lossy());
    let name = normalize(script_name);
    path.ends_with(&target) || path.ends_with(&format!("/{target}")) || name == target
}

fn tbl_to_lua_value(tbl: &LuaTable, key: &str) -> crate::script_state::LuaValue {
    match tbl.get::<LuaValue>(key) {
        Ok(LuaValue::Integer(n)) => crate::script_state::LuaValue::Int(n),
        Ok(LuaValue::Number(n)) => crate::script_state::LuaValue::Float(n),
        Ok(LuaValue::Boolean(b)) => crate::script_state::LuaValue::Bool(b),
        Ok(LuaValue::String(s)) => {
            crate::script_state::LuaValue::String(s.to_string_lossy().to_string())
        }
        _ => crate::script_state::LuaValue::Nil,
    }
}
