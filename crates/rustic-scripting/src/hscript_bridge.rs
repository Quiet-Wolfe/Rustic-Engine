//! Psych-shaped host bridge for HScript.
//!
//! `rustic-hscript` owns syntax/evaluation. This module owns the engine-facing
//! compatibility layer: globals, classes, FlxSprite-like objects, and requests
//! queued into `ScriptState`.

use std::collections::HashMap;

use rustic_hscript::{HostBridge, Interp, Value as HValue};

use crate::script_state::{
    AudioRequest, LuaAnimDef, LuaSprite, LuaSpriteKind, LuaText, LuaValue, PrecacheRequest,
    ScriptCallRequest, ScriptState, SongControlRequest,
};
use crate::tweens::{EaseFunc, LuaTimer, Tween, TweenProperty};

const HOST_HANDLE_TAG: &str = "psych";
const HOST_HANDLES: &[&str] = &[
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
    "PlayState",
    "FlxG",
    "FlxG.sound",
    "FlxG.camera",
    "FlxG.random",
    "FlxG.keys.justPressed",
    "FlxG.keys.pressed",
    "FlxG.keys.justReleased",
    "FlxG.mouse",
    "FlxTween",
    "FlxEase",
    "Paths",
    "Conductor",
    "ClientPrefs",
    "ClientPrefs.data",
    "Math",
    "Std",
    "StringTools",
    "Reflect",
    "FlxColor",
];

pub(crate) struct ScriptStateBridge<'a> {
    pub(crate) state: &'a mut ScriptState,
}

pub(crate) fn seed_globals(interp: &mut Interp, state: &ScriptState) {
    interp.set_global("songName", HValue::from_str(&state.song_name));
    interp.set_global("curStage", HValue::from_str(&state.cur_stage));
    interp.set_global("difficultyName", HValue::from_str(&state.difficulty_name));
    interp.set_global("isStoryMode", HValue::Bool(state.is_story_mode));
    interp.set_global("screenWidth", HValue::Int(state.screen_width as i64));
    interp.set_global("screenHeight", HValue::Int(state.screen_height as i64));
    interp.set_global("modcharts", HValue::Bool(true));
}

pub(crate) fn refresh_engine_globals(interp: &mut Interp, state: &ScriptState) {
    interp.set_global("curBeat", HValue::Int(state.cur_beat as i64));
    interp.set_global("curStep", HValue::Int(state.cur_step as i64));
    interp.set_global("curSection", HValue::Int(state.cur_section as i64));
    interp.set_global("songPosition", HValue::Float(state.song_position));
    interp.set_global("health", HValue::Float(state.health as f64));
    interp.set_global("score", HValue::Int(state.score as i64));
    interp.set_global("misses", HValue::Int(state.misses as i64));
}

impl HostBridge for ScriptStateBridge<'_> {
    fn global_get(&mut self, name: &str) -> Result<HValue, String> {
        match name {
            "curBeat" => return Ok(HValue::Int(self.state.cur_beat as i64)),
            "curStep" => return Ok(HValue::Int(self.state.cur_step as i64)),
            "curSection" => return Ok(HValue::Int(self.state.cur_section as i64)),
            "songName" => return Ok(HValue::from_str(&self.state.song_name)),
            "curStage" => return Ok(HValue::from_str(&self.state.cur_stage)),
            "difficultyName" => return Ok(HValue::from_str(&self.state.difficulty_name)),
            "songPosition" => return Ok(HValue::Float(self.state.song_position)),
            "health" => return Ok(HValue::Float(self.state.health as f64)),
            "score" => return Ok(HValue::Int(self.state.score as i64)),
            "misses" => return Ok(HValue::Int(self.state.misses as i64)),
            "hits" => return Ok(HValue::Int(self.state.hits as i64)),
            "combo" => return Ok(HValue::Int(self.state.combo as i64)),
            _ => {}
        }

        if let Some(id) = host_handle_id(name) {
            return Ok(host_handle(id));
        }
        if let Some(v) = self.state.custom_vars.get(name) {
            return Ok(lua_value_to_h(v));
        }
        Ok(HValue::Null)
    }

    fn global_set(&mut self, name: &str, value: &HValue) -> Result<bool, String> {
        if is_property_path(name) {
            set_property_path(self.state, name, value);
            return Ok(true);
        }
        self.state
            .custom_vars
            .insert(name.to_string(), h_value_to_lua(value));
        Ok(true)
    }

    fn global_call(&mut self, name: &str, args: &[HValue]) -> Result<Option<HValue>, String> {
        Ok(dispatch_global(self.state, name, args)?)
    }

    fn field_get(&mut self, target: &HValue, field: &str) -> Result<HValue, String> {
        if let Some(class) = object_class(target) {
            return object_field_get(target, &class, field);
        }
        let Some(name) = handle_name(target) else {
            return Err(format!(
                "hscript bridge: field '{field}' not wired on {target:?}"
            ));
        };
        handle_field_get(self.state, name, field)
    }

    fn field_set(&mut self, target: &HValue, field: &str, value: &HValue) -> Result<(), String> {
        if let Some(class) = object_class(target) {
            return object_field_set(self.state, target, &class, field, value);
        }
        let Some(name) = handle_name(target) else {
            return Err(format!(
                "hscript bridge: field '{field}' set not wired on {target:?}"
            ));
        };
        set_property_path(self.state, &format!("{name}.{field}"), value);
        Ok(())
    }

    fn method_call(
        &mut self,
        target: &HValue,
        method: &str,
        args: &[HValue],
    ) -> Result<HValue, String> {
        if let Some(class) = object_class(target) {
            return object_method_call(self.state, target, &class, method, args);
        }
        let Some(name) = handle_name(target) else {
            return Err(format!(
                "hscript bridge: method '{method}' not wired on {target:?}"
            ));
        };
        handle_method_call(self.state, name, method, args)
    }

    fn construct(&mut self, type_name: &str, args: &[HValue]) -> Result<HValue, String> {
        match short_type_name(type_name) {
            "FlxSprite" | "ModchartSprite" => Ok(make_sprite_object(
                self.state,
                arg_f64(args, 0, 0.0),
                arg_f64(args, 1, 0.0),
            )),
            "FlxText" => Ok(make_text_object(
                self.state,
                arg_f64(args, 0, 0.0),
                arg_f64(args, 1, 0.0),
                arg_f64(args, 2, 0.0),
                &arg_str(args, 3, ""),
            )),
            _ => Err(format!(
                "hscript bridge: construction of '{type_name}' is not wired"
            )),
        }
    }
}

fn dispatch_global(
    state: &mut ScriptState,
    name: &str,
    args: &[HValue],
) -> Result<Option<HValue>, String> {
    let out = match name {
        "trace" | "debugPrint" => {
            log::info!(
                "[hscript] {}",
                args.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            HValue::Null
        }
        "getProperty" => get_property_path(state, &arg_str(args, 0, "")),
        "setProperty" => {
            set_property_path(
                state,
                &arg_str(args, 0, ""),
                args.get(1).unwrap_or(&HValue::Null),
            );
            HValue::Null
        }
        "getVar" => state
            .custom_vars
            .get(&arg_str(args, 0, ""))
            .map(lua_value_to_h)
            .unwrap_or(HValue::Null),
        "setVar" => {
            state.custom_vars.insert(
                arg_str(args, 0, ""),
                h_value_to_lua(args.get(1).unwrap_or(&HValue::Null)),
            );
            HValue::Null
        }
        "removeVar" => HValue::Bool(state.custom_vars.remove(&arg_str(args, 0, "")).is_some()),
        "add" | "insert" => {
            if let Some(obj) = args.last() {
                add_display_object(state, obj, true)?;
            }
            HValue::Null
        }
        "remove" => {
            if let Some(obj) = args.first() {
                remove_display_object(state, obj);
            }
            HValue::Null
        }
        "makeLuaSprite" | "makeAnimatedLuaSprite" => {
            let tag = arg_str(args, 0, "");
            let image = arg_str(args, 1, "");
            let mut sprite = LuaSprite::new(
                &tag,
                LuaSpriteKind::Image(image),
                arg_f64(args, 2, 0.0) as f32,
                arg_f64(args, 3, 0.0) as f32,
            );
            if name == "makeAnimatedLuaSprite" {
                if let LuaSpriteKind::Image(image) = sprite.kind {
                    sprite.kind = LuaSpriteKind::Animated(image);
                }
            }
            state.lua_sprites.insert(tag, sprite);
            HValue::Null
        }
        "addLuaSprite" => {
            state
                .sprites_to_add
                .push((arg_str(args, 0, ""), arg_bool(args, 1, false)));
            HValue::Null
        }
        "removeLuaSprite" => {
            state.sprites_to_remove.push(arg_str(args, 0, ""));
            HValue::Null
        }
        "addAnimationByPrefix" | "luaSpriteAddAnimationByPrefix" => {
            if let Some(sprite) = state.lua_sprites.get_mut(&arg_str(args, 0, "")) {
                let anim = arg_str(args, 1, "");
                sprite.animations.insert(
                    anim.clone(),
                    LuaAnimDef {
                        prefix: arg_str(args, 2, &anim),
                        fps: arg_f64(args, 3, 24.0) as f32,
                        looping: arg_bool(args, 4, true),
                        indices: Vec::new(),
                    },
                );
            }
            HValue::Null
        }
        "objectPlayAnimation" | "playAnim" | "luaSpritePlayAnimation" => {
            if let Some(sprite) = state.lua_sprites.get_mut(&arg_str(args, 0, "")) {
                sprite.current_anim = arg_str(args, 1, "");
                sprite.anim_frame = 0;
                sprite.anim_timer = 0.0;
                sprite.anim_finished = false;
            }
            HValue::Null
        }
        "setScrollFactor" | "setLuaSpriteScrollFactor" => {
            if let Some(sprite) = state.lua_sprites.get_mut(&arg_str(args, 0, "")) {
                sprite.scroll_x = arg_f64(args, 1, 1.0) as f32;
                sprite.scroll_y = arg_f64(args, 2, arg_f64(args, 1, 1.0)) as f32;
            }
            HValue::Null
        }
        "scaleObject" | "scaleLuaSprite" => {
            if let Some(sprite) = state.lua_sprites.get_mut(&arg_str(args, 0, "")) {
                sprite.scale_x = arg_f64(args, 1, 1.0) as f32;
                sprite.scale_y = arg_f64(args, 2, arg_f64(args, 1, 1.0)) as f32;
            }
            HValue::Null
        }
        "setObjectCamera" | "setLuaSpriteCamera" => {
            if let Some(sprite) = state.lua_sprites.get_mut(&arg_str(args, 0, "")) {
                sprite.camera = normalize_camera(&arg_str(args, 1, "camGame"));
            }
            HValue::Null
        }
        "makeLuaText" => {
            let text = LuaText::new(
                &arg_str(args, 0, ""),
                &arg_str(args, 1, ""),
                arg_f64(args, 2, 0.0) as f32,
                arg_f64(args, 3, 0.0) as f32,
                arg_f64(args, 4, 0.0) as f32,
            );
            state.lua_texts.insert(text.tag.clone(), text);
            HValue::Null
        }
        "addLuaText" => {
            state
                .texts_to_add
                .push((arg_str(args, 0, ""), arg_bool(args, 1, false)));
            HValue::Null
        }
        "removeLuaText" => {
            state.lua_texts.remove(&arg_str(args, 0, ""));
            HValue::Null
        }
        "playSound" => {
            state.audio_requests.push(AudioRequest::PlaySound {
                path: arg_str(args, 0, ""),
                volume: arg_f64(args, 1, 1.0),
                tag: opt_arg_str(args, 2),
                looping: false,
            });
            HValue::Null
        }
        "playMusic" => {
            state.audio_requests.push(AudioRequest::PlayMusic {
                path: arg_str(args, 0, ""),
                volume: arg_f64(args, 1, 1.0),
                looping: arg_bool(args, 2, false),
            });
            HValue::Null
        }
        "stopSound" => {
            state
                .audio_requests
                .push(AudioRequest::StopSound(opt_arg_str(args, 0)));
            HValue::Null
        }
        "cameraSetTarget" => {
            state.camera_target_requests.push(arg_str(args, 0, ""));
            HValue::Null
        }
        "cameraShake" => {
            state.camera_shake_requests.push((
                arg_str(args, 0, "camGame"),
                arg_f64(args, 1, 0.0) as f32,
                arg_f64(args, 2, 0.0) as f32,
            ));
            HValue::Null
        }
        "cameraFlash" => {
            state.camera_flash_requests.push((
                arg_str(args, 0, "camGame"),
                arg_str(args, 1, "FFFFFF"),
                arg_f64(args, 2, 1.0) as f32,
                1.0,
            ));
            HValue::Null
        }
        "cameraFade" => {
            state.camera_fade_requests.push((
                arg_str(args, 0, "camGame"),
                arg_str(args, 1, "000000"),
                arg_f64(args, 2, 1.0) as f32,
                arg_bool(args, 3, false),
            ));
            HValue::Null
        }
        "triggerEvent" => {
            state.triggered_events.push((
                arg_str(args, 0, ""),
                arg_str(args, 1, ""),
                arg_str(args, 2, ""),
            ));
            HValue::Null
        }
        "startCountdown" => {
            state
                .control_requests
                .push(SongControlRequest::StartCountdown);
            HValue::Bool(true)
        }
        "endSong" => {
            state.control_requests.push(SongControlRequest::EndSong);
            HValue::Bool(true)
        }
        "exitSong" => {
            state.control_requests.push(SongControlRequest::ExitSong);
            HValue::Bool(true)
        }
        "restartSong" => {
            state.control_requests.push(SongControlRequest::RestartSong);
            HValue::Bool(true)
        }
        "runTimer" => {
            state.tweens.add_timer(LuaTimer {
                tag: arg_str(args, 0, ""),
                duration: arg_f64(args, 1, 1.0) as f32,
                elapsed: 0.0,
                loops_total: arg_i64(args, 2, 1) as i32,
                loops_done: 0,
                finished: false,
            });
            HValue::Null
        }
        "cancelTimer" => {
            state.tweens.cancel_timer(&arg_str(args, 0, ""));
            HValue::Null
        }
        "doTweenX" => {
            add_tween(state, args, TweenProperty::X);
            HValue::Null
        }
        "doTweenY" => {
            add_tween(state, args, TweenProperty::Y);
            HValue::Null
        }
        "doTweenAlpha" => {
            add_tween(state, args, TweenProperty::Alpha);
            HValue::Null
        }
        "doTweenAngle" => {
            add_tween(state, args, TweenProperty::Angle);
            HValue::Null
        }
        "doTweenZoom" => {
            add_tween(state, args, TweenProperty::Zoom);
            HValue::Null
        }
        "cancelTween" => {
            state.tweens.cancel_tween(&arg_str(args, 0, ""));
            HValue::Null
        }
        "precacheImage" => {
            state.precache_requests.push(PrecacheRequest::Image {
                name: arg_str(args, 0, ""),
                allow_gpu: arg_bool(args, 1, true),
            });
            HValue::Null
        }
        "precacheSound" => {
            state.precache_requests.push(PrecacheRequest::Sound {
                name: arg_str(args, 0, ""),
            });
            HValue::Null
        }
        "precacheMusic" => {
            state.precache_requests.push(PrecacheRequest::Music {
                name: arg_str(args, 0, ""),
            });
            HValue::Null
        }
        "callOnLuas" | "callOnScripts" | "callOnHScript" => {
            state.script_call_requests.push(ScriptCallRequest {
                target: None,
                function: arg_str(args, 0, ""),
                args: h_args_to_lua(args.get(1)),
            });
            HValue::Null
        }
        "setOnLuas" | "setOnScripts" | "setOnHScript" => {
            state.script_global_sets.push((
                arg_str(args, 0, ""),
                h_value_to_lua(args.get(1).unwrap_or(&HValue::Null)),
            ));
            HValue::Null
        }
        "getRandomInt" => HValue::Int(rand_int(arg_i64(args, 0, 0), arg_i64(args, 1, 100))),
        "getRandomFloat" => HValue::Float(rand_float(arg_f64(args, 0, 0.0), arg_f64(args, 1, 1.0))),
        "getRandomBool" => HValue::Bool(rand_float(0.0, 100.0) < arg_f64(args, 0, 50.0)),
        "getScreenWidth" => HValue::Int(state.screen_width as i64),
        "getScreenHeight" => HValue::Int(state.screen_height as i64),
        _ => return Ok(None),
    };
    Ok(Some(out))
}

fn handle_field_get(state: &ScriptState, name: &str, field: &str) -> Result<HValue, String> {
    match name {
        "FlxG" => match field {
            "width" => Ok(HValue::Int(state.screen_width as i64)),
            "height" => Ok(HValue::Int(state.screen_height as i64)),
            "sound" | "camera" | "mouse" | "random" => Ok(handle_by_name(&format!("FlxG.{field}"))),
            "keys" => Ok(handle_by_name("FlxG.keys.pressed")),
            _ => Ok(HValue::Null),
        },
        "FlxG.keys.pressed" | "FlxG.keys.justPressed" | "FlxG.keys.justReleased" => {
            let set = match name {
                "FlxG.keys.justPressed" => &state.input_just_pressed,
                "FlxG.keys.justReleased" => &state.input_just_released,
                _ => &state.input_pressed,
            };
            Ok(HValue::Bool(set.contains(&field.to_ascii_uppercase())))
        }
        "FlxG.mouse" => match field {
            "x" => Ok(HValue::Float(state.mouse_position.0 as f64)),
            "y" => Ok(HValue::Float(state.mouse_position.1 as f64)),
            "pressed" => Ok(HValue::Bool(state.mouse_pressed)),
            "justPressed" => Ok(HValue::Bool(state.mouse_just_pressed)),
            "justReleased" => Ok(HValue::Bool(state.mouse_just_released)),
            _ => Ok(HValue::Null),
        },
        "FlxEase" => Ok(HValue::from_str(field)),
        "FlxColor" => Ok(HValue::Int(color_name_to_int(field))),
        "Math" => match field {
            "PI" => Ok(HValue::Float(std::f64::consts::PI)),
            "NaN" => Ok(HValue::Float(f64::NAN)),
            "POSITIVE_INFINITY" => Ok(HValue::Float(f64::INFINITY)),
            "NEGATIVE_INFINITY" => Ok(HValue::Float(f64::NEG_INFINITY)),
            _ => Ok(HValue::Null),
        },
        "ClientPrefs" if field == "data" => Ok(handle_by_name("ClientPrefs.data")),
        "ClientPrefs.data" => Ok(client_pref_value(state, field)),
        "Conductor" => match field {
            "songPosition" => Ok(HValue::Float(state.song_position)),
            "bpm" => Ok(HValue::Float(state.bpm)),
            "crochet" => Ok(HValue::Float(60000.0 / state.bpm.max(1.0))),
            "stepCrochet" => Ok(HValue::Float(15000.0 / state.bpm.max(1.0))),
            _ => Ok(HValue::Null),
        },
        "PlayState" => handle_field_get(state, "game", field),
        "game" => match field {
            "camGame" => Ok(handle_by_name("camGame")),
            "camHUD" => Ok(handle_by_name("camHUD")),
            "camOther" => Ok(handle_by_name("camOther")),
            "boyfriend" => Ok(handle_by_name("boyfriend")),
            "dad" => Ok(handle_by_name("dad")),
            "gf" => Ok(handle_by_name("gf")),
            "boyfriendGroup" => Ok(handle_by_name("boyfriendGroup")),
            "dadGroup" => Ok(handle_by_name("dadGroup")),
            "gfGroup" => Ok(handle_by_name("gfGroup")),
            _ => Ok(get_property_path(state, field)),
        },
        _ => Ok(get_property_path(state, &format!("{name}.{field}"))),
    }
}

fn handle_method_call(
    state: &mut ScriptState,
    name: &str,
    method: &str,
    args: &[HValue],
) -> Result<HValue, String> {
    match (name, method) {
        ("game", "add") | ("game", "insert") => {
            if let Some(obj) = args.last() {
                add_display_object(state, obj, true)?;
            }
            Ok(HValue::Null)
        }
        ("game", "remove") => {
            if let Some(obj) = args.first() {
                remove_display_object(state, obj);
            }
            Ok(HValue::Null)
        }
        ("game", "triggerEvent") => {
            dispatch_global(state, "triggerEvent", args).map(|v| v.unwrap_or(HValue::Null))
        }
        ("FlxG.sound", "play") => {
            dispatch_global(state, "playSound", args).map(|v| v.unwrap_or(HValue::Null))
        }
        ("FlxG.sound", "playMusic") => {
            dispatch_global(state, "playMusic", args).map(|v| v.unwrap_or(HValue::Null))
        }
        ("FlxG.camera", "shake") => dispatch_global(
            state,
            "cameraShake",
            &[
                HValue::from_str("camGame"),
                args.get(0).cloned().unwrap_or(HValue::Float(0.0)),
                args.get(1).cloned().unwrap_or(HValue::Float(0.0)),
            ],
        )
        .map(|v| v.unwrap_or(HValue::Null)),
        ("FlxG.camera", "flash") => dispatch_global(
            state,
            "cameraFlash",
            &[
                HValue::from_str("camGame"),
                args.get(0).cloned().unwrap_or(HValue::from_str("FFFFFF")),
                args.get(1).cloned().unwrap_or(HValue::Float(1.0)),
            ],
        )
        .map(|v| v.unwrap_or(HValue::Null)),
        ("FlxG.camera", "fade") => dispatch_global(
            state,
            "cameraFade",
            &[
                HValue::from_str("camGame"),
                args.get(0).cloned().unwrap_or(HValue::from_str("000000")),
                args.get(1).cloned().unwrap_or(HValue::Float(1.0)),
            ],
        )
        .map(|v| v.unwrap_or(HValue::Null)),
        ("FlxG.random", "int") => Ok(HValue::Int(rand_int(
            arg_i64(args, 0, 0),
            arg_i64(args, 1, 100),
        ))),
        ("FlxG.random", "float") => Ok(HValue::Float(rand_float(
            arg_f64(args, 0, 0.0),
            arg_f64(args, 1, 1.0),
        ))),
        ("FlxG.random", "bool") => Ok(HValue::Bool(
            rand_float(0.0, 100.0) < arg_f64(args, 0, 50.0),
        )),
        ("FlxTween", "tween") => {
            start_object_tween(state, args);
            Ok(HValue::Null)
        }
        ("FlxTween", "cancelTweensOf") => Ok(HValue::Null),
        ("Paths", "image")
        | ("Paths", "sound")
        | ("Paths", "music")
        | ("Paths", "video")
        | ("Paths", "getSparrowAtlas")
        | ("Paths", "getPackerAtlas")
        | ("Paths", "txt")
        | ("Paths", "json")
        | ("Paths", "font") => Ok(HValue::from_str(arg_str(args, 0, ""))),
        ("Math", "sin") => Ok(HValue::Float(arg_f64(args, 0, 0.0).sin())),
        ("Math", "cos") => Ok(HValue::Float(arg_f64(args, 0, 0.0).cos())),
        ("Math", "tan") => Ok(HValue::Float(arg_f64(args, 0, 0.0).tan())),
        ("Math", "asin") => Ok(HValue::Float(arg_f64(args, 0, 0.0).asin())),
        ("Math", "acos") => Ok(HValue::Float(arg_f64(args, 0, 0.0).acos())),
        ("Math", "atan") => Ok(HValue::Float(arg_f64(args, 0, 0.0).atan())),
        ("Math", "atan2") => Ok(HValue::Float(
            arg_f64(args, 0, 0.0).atan2(arg_f64(args, 1, 0.0)),
        )),
        ("Math", "sqrt") => Ok(HValue::Float(arg_f64(args, 0, 0.0).sqrt())),
        ("Math", "abs") => Ok(HValue::Float(arg_f64(args, 0, 0.0).abs())),
        ("Math", "min") => Ok(HValue::Float(
            arg_f64(args, 0, 0.0).min(arg_f64(args, 1, 0.0)),
        )),
        ("Math", "max") => Ok(HValue::Float(
            arg_f64(args, 0, 0.0).max(arg_f64(args, 1, 0.0)),
        )),
        ("Math", "floor") => Ok(HValue::Int(arg_f64(args, 0, 0.0).floor() as i64)),
        ("Math", "ceil") => Ok(HValue::Int(arg_f64(args, 0, 0.0).ceil() as i64)),
        ("Math", "round") => Ok(HValue::Int(arg_f64(args, 0, 0.0).round() as i64)),
        ("Math", "random") => Ok(HValue::Float(rand_float(0.0, 1.0))),
        ("Std", "int") => Ok(HValue::Int(arg_f64(args, 0, 0.0) as i64)),
        ("Std", "string") => Ok(HValue::from_str(
            args.first().map(ToString::to_string).unwrap_or_default(),
        )),
        ("Std", "parseInt") => Ok(HValue::Int(
            arg_str(args, 0, "0").parse::<i64>().unwrap_or(0),
        )),
        ("Std", "parseFloat") => Ok(HValue::Float(
            arg_str(args, 0, "0").parse::<f64>().unwrap_or(f64::NAN),
        )),
        ("Std", "isOfType") | ("Std", "is") => Ok(HValue::Bool(!matches!(
            args.first(),
            None | Some(HValue::Null)
        ))),
        ("StringTools", "startsWith") => Ok(HValue::Bool(
            arg_str(args, 0, "").starts_with(&arg_str(args, 1, "")),
        )),
        ("StringTools", "endsWith") => Ok(HValue::Bool(
            arg_str(args, 0, "").ends_with(&arg_str(args, 1, "")),
        )),
        ("StringTools", "contains") => Ok(HValue::Bool(
            arg_str(args, 0, "").contains(&arg_str(args, 1, "")),
        )),
        ("StringTools", "trim") => Ok(HValue::from_str(arg_str(args, 0, "").trim())),
        ("StringTools", "replace") => Ok(HValue::from_str(
            arg_str(args, 0, "").replace(&arg_str(args, 1, ""), &arg_str(args, 2, "")),
        )),
        ("Reflect", "field") => Ok(reflect_field(args)),
        ("Reflect", "setField") => {
            if let Some(obj) = args.first() {
                obj_set(
                    obj,
                    &arg_str(args, 1, ""),
                    args.get(2).cloned().unwrap_or(HValue::Null),
                );
            }
            Ok(HValue::Null)
        }
        ("Reflect", "hasField") => Ok(HValue::Bool(
            obj_get(args.first().unwrap_or(&HValue::Null), &arg_str(args, 1, "")).is_some(),
        )),
        ("Reflect", "callMethod") => Ok(HValue::Null),
        ("FlxColor", "fromRGB") => {
            let r = arg_i64(args, 0, 0).clamp(0, 255);
            let g = arg_i64(args, 1, 0).clamp(0, 255);
            let b = arg_i64(args, 2, 0).clamp(0, 255);
            let a = arg_i64(args, 3, 255).clamp(0, 255);
            Ok(HValue::Int((a << 24) | (r << 16) | (g << 8) | b))
        }
        ("FlxColor", "fromString") => Ok(HValue::Int(color_string_to_int(&arg_str(
            args, 0, "FFFFFF",
        )))),
        _ => Err(format!(
            "hscript bridge: method '{name}.{method}' is not wired"
        )),
    }
}

fn object_method_call(
    state: &mut ScriptState,
    target: &HValue,
    class: &str,
    method: &str,
    args: &[HValue],
) -> Result<HValue, String> {
    match (class, method) {
        ("FlxSprite", "loadGraphic") => {
            obj_set(target, "image", HValue::from_str(arg_str(args, 0, "")));
            obj_set(target, "kind", HValue::from_str("image"));
            Ok(target.clone())
        }
        ("FlxSprite", "makeGraphic") => {
            obj_set(target, "width", HValue::Float(arg_f64(args, 0, 0.0)));
            obj_set(target, "height", HValue::Float(arg_f64(args, 1, 0.0)));
            obj_set(
                target,
                "color",
                HValue::from_str(format!("{:06X}", arg_i64(args, 2, 0xFFFFFF) & 0xFFFFFF)),
            );
            obj_set(target, "kind", HValue::from_str("graphic"));
            Ok(target.clone())
        }
        ("FlxSprite", "setGraphicSize") => {
            obj_set(target, "width", HValue::Float(arg_f64(args, 0, 0.0)));
            if args.len() > 1 {
                obj_set(target, "height", HValue::Float(arg_f64(args, 1, 0.0)));
            }
            sync_added_object(state, target);
            Ok(HValue::Null)
        }
        ("FlxSprite", "updateHitbox") => Ok(HValue::Null),
        ("FlxSprite", "screenCenter") => {
            obj_set(target, "x", HValue::Float(state.screen_width as f64 * 0.5));
            obj_set(target, "y", HValue::Float(state.screen_height as f64 * 0.5));
            sync_added_object(state, target);
            Ok(HValue::Null)
        }
        ("FlxSprite", "kill") | ("FlxSprite", "destroy") => {
            remove_display_object(state, target);
            Ok(HValue::Null)
        }
        ("FlxText", "setFormat") => {
            obj_set(target, "font", HValue::from_str(arg_str(args, 0, "")));
            obj_set(target, "size", HValue::Float(arg_f64(args, 1, 16.0)));
            obj_set(
                target,
                "color",
                HValue::from_str(format!("{:06X}", arg_i64(args, 2, 0xFFFFFF) & 0xFFFFFF)),
            );
            sync_added_object(state, target);
            Ok(target.clone())
        }
        ("PointProxy", "set") => {
            if let Some(owner) = obj_get(target, "__owner") {
                let x_key = obj_str(target, "__x", "x");
                let y_key = obj_str(target, "__y", "y");
                obj_set(&owner, &x_key, HValue::Float(arg_f64(args, 0, 0.0)));
                obj_set(
                    &owner,
                    &y_key,
                    HValue::Float(arg_f64(args, 1, arg_f64(args, 0, 0.0))),
                );
                sync_added_object(state, &owner);
            }
            Ok(HValue::Null)
        }
        ("AnimationProxy", "add")
        | ("AnimationProxy", "addByPrefix")
        | ("AnimationProxy", "addByIndices") => {
            if let Some(owner) = obj_get(target, "__owner") {
                let anim = arg_str(args, 0, "");
                let prefix = arg_str(args, 1, &anim);
                let fps = arg_f64(args, 2, 24.0) as f32;
                let looping = arg_bool(args, 3, true);
                add_object_anim(&owner, &anim, &prefix, fps, looping);
                sync_added_object(state, &owner);
            }
            Ok(HValue::Null)
        }
        ("AnimationProxy", "play") => {
            if let Some(owner) = obj_get(target, "__owner") {
                obj_set(&owner, "animation", HValue::from_str(arg_str(args, 0, "")));
                sync_added_object(state, &owner);
            }
            Ok(HValue::Null)
        }
        _ => Err(format!("hscript bridge: {class}.{method} is not wired")),
    }
}

fn object_field_get(target: &HValue, class: &str, field: &str) -> Result<HValue, String> {
    match (class, field) {
        ("FlxSprite", "scrollFactor") => Ok(point_proxy(target, "scroll_x", "scroll_y")),
        ("FlxSprite", "scale") => Ok(point_proxy(target, "scale_x", "scale_y")),
        ("FlxSprite", "offset") => Ok(point_proxy(target, "offset_x", "offset_y")),
        ("FlxSprite", "origin") => Ok(point_proxy(target, "origin_x", "origin_y")),
        ("FlxSprite", "animation") => Ok(proxy(target, "AnimationProxy")),
        _ => {
            obj_get(target, field).ok_or_else(|| format!("hscript bridge: {class}.{field} missing"))
        }
    }
}

fn object_field_set(
    state: &mut ScriptState,
    target: &HValue,
    _class: &str,
    field: &str,
    value: &HValue,
) -> Result<(), String> {
    obj_set(target, field, value.clone());
    sync_added_object(state, target);
    Ok(())
}

fn make_sprite_object(state: &mut ScriptState, x: f64, y: f64) -> HValue {
    let obj = HValue::new_object();
    obj_set(&obj, "__class", HValue::from_str("FlxSprite"));
    obj_set(
        &obj,
        "__tag",
        HValue::from_str(next_tag(state, "hscriptSprite")),
    );
    obj_set(&obj, "x", HValue::Float(x));
    obj_set(&obj, "y", HValue::Float(y));
    obj_set(&obj, "alpha", HValue::Float(1.0));
    obj_set(&obj, "visible", HValue::Bool(true));
    obj_set(&obj, "scale_x", HValue::Float(1.0));
    obj_set(&obj, "scale_y", HValue::Float(1.0));
    obj_set(&obj, "scroll_x", HValue::Float(1.0));
    obj_set(&obj, "scroll_y", HValue::Float(1.0));
    obj_set(&obj, "kind", HValue::from_str("image"));
    obj
}

fn make_text_object(state: &mut ScriptState, x: f64, y: f64, width: f64, text: &str) -> HValue {
    let obj = HValue::new_object();
    obj_set(&obj, "__class", HValue::from_str("FlxText"));
    obj_set(
        &obj,
        "__tag",
        HValue::from_str(next_tag(state, "hscriptText")),
    );
    obj_set(&obj, "x", HValue::Float(x));
    obj_set(&obj, "y", HValue::Float(y));
    obj_set(&obj, "width", HValue::Float(width));
    obj_set(&obj, "text", HValue::from_str(text));
    obj_set(&obj, "size", HValue::Float(16.0));
    obj
}

fn add_display_object(state: &mut ScriptState, obj: &HValue, in_front: bool) -> Result<(), String> {
    match object_class(obj).as_deref() {
        Some("FlxSprite") => {
            let tag = obj_str(obj, "__tag", &next_tag(state, "hscriptSprite"));
            let atlas_image = obj_str(obj, "frames", "");
            let kind = if obj_str(obj, "kind", "image") == "graphic" {
                LuaSpriteKind::Graphic {
                    width: obj_f64(obj, "width", 1.0) as i32,
                    height: obj_f64(obj, "height", 1.0) as i32,
                    color: obj_str(obj, "color", "FFFFFF"),
                }
            } else if !atlas_image.is_empty() {
                LuaSpriteKind::Animated(atlas_image)
            } else {
                LuaSpriteKind::Image(obj_str(obj, "image", ""))
            };
            let mut sprite = LuaSprite::new(
                &tag,
                kind,
                obj_f64(obj, "x", 0.0) as f32,
                obj_f64(obj, "y", 0.0) as f32,
            );
            apply_obj_to_sprite(obj, &mut sprite);
            state.lua_sprites.insert(tag.clone(), sprite);
            state.sprites_to_add.push((tag.clone(), in_front));
            obj_set(obj, "__added_tag", HValue::from_str(tag));
            Ok(())
        }
        Some("FlxText") => {
            let tag = obj_str(obj, "__tag", &next_tag(state, "hscriptText"));
            let text = LuaText::new(
                &tag,
                &obj_str(obj, "text", ""),
                obj_f64(obj, "width", 0.0) as f32,
                obj_f64(obj, "x", 0.0) as f32,
                obj_f64(obj, "y", 0.0) as f32,
            );
            state.lua_texts.insert(tag.clone(), text);
            state.texts_to_add.push((tag.clone(), in_front));
            obj_set(obj, "__added_tag", HValue::from_str(tag));
            Ok(())
        }
        _ => Err(format!("hscript bridge: cannot add {obj:?}")),
    }
}

fn remove_display_object(state: &mut ScriptState, obj: &HValue) {
    if let Some(tag) = obj_get(obj, "__added_tag").and_then(|v| v.as_str().map(str::to_string)) {
        state.sprites_to_remove.push(tag);
    }
}

fn sync_added_object(state: &mut ScriptState, obj: &HValue) {
    let Some(tag) = obj_get(obj, "__added_tag").and_then(|v| v.as_str().map(str::to_string)) else {
        return;
    };
    if let Some(sprite) = state.lua_sprites.get_mut(&tag) {
        apply_obj_to_sprite(obj, sprite);
    } else if let Some(text) = state.lua_texts.get_mut(&tag) {
        text.x = obj_f64(obj, "x", text.x as f64) as f32;
        text.y = obj_f64(obj, "y", text.y as f64) as f32;
        text.width = obj_f64(obj, "width", text.width as f64) as f32;
        text.text = obj_str(obj, "text", &text.text);
        text.size = obj_f64(obj, "size", text.size as f64) as f32;
        text.color = obj_str(obj, "color", &text.color);
        text.font = obj_str(obj, "font", &text.font);
    }
}

fn apply_obj_to_sprite(obj: &HValue, sprite: &mut LuaSprite) {
    sprite.x = obj_f64(obj, "x", sprite.x as f64) as f32;
    sprite.y = obj_f64(obj, "y", sprite.y as f64) as f32;
    sprite.alpha = obj_f64(obj, "alpha", sprite.alpha as f64) as f32;
    sprite.visible = obj_bool(obj, "visible", sprite.visible);
    sprite.scale_x = obj_f64(obj, "scale_x", sprite.scale_x as f64) as f32;
    sprite.scale_y = obj_f64(obj, "scale_y", sprite.scale_y as f64) as f32;
    sprite.scroll_x = obj_f64(obj, "scroll_x", sprite.scroll_x as f64) as f32;
    sprite.scroll_y = obj_f64(obj, "scroll_y", sprite.scroll_y as f64) as f32;
    sprite.angle = obj_f64(obj, "angle", sprite.angle as f64) as f32;
    sprite.current_anim = obj_str(obj, "animation", &sprite.current_anim);
    if let Some(camera) = obj_get(obj, "camera").or_else(|| obj_get(obj, "cameras")) {
        sprite.camera = h_value_to_camera(&camera);
    }
    if let Some(HValue::Object(map)) = obj_get(obj, "__animations") {
        for (name, value) in map.borrow().iter() {
            if let HValue::Object(def) = value {
                sprite.animations.insert(
                    name.clone(),
                    LuaAnimDef {
                        prefix: get_str(&def.borrow(), "prefix", name),
                        fps: get_f64(&def.borrow(), "fps", 24.0) as f32,
                        looping: get_bool(&def.borrow(), "looping", true),
                        indices: Vec::new(),
                    },
                );
            }
        }
    }
}

fn add_tween(state: &mut ScriptState, args: &[HValue], property: TweenProperty) {
    let tag = arg_str(args, 0, "");
    let target = arg_str(args, 1, "");
    let end_value = arg_f64(args, 2, 0.0) as f32;
    let duration = arg_f64(args, 3, 0.0).max(0.0001) as f32;
    let ease = EaseFunc::from_string(&arg_str(args, 4, "linear"));
    let start_value = get_property_path(state, &format!("{target}.{}", tween_prop_name(&property)))
        .as_f64()
        .unwrap_or(0.0) as f32;
    state.tweens.add_tween(Tween {
        tag,
        target,
        property,
        start_value,
        end_value,
        duration,
        elapsed: 0.0,
        ease,
        finished: false,
    });
}

fn start_object_tween(state: &mut ScriptState, args: &[HValue]) {
    let Some(target) = args.first() else {
        return;
    };
    let props = match args.get(1) {
        Some(HValue::Object(map)) => map.borrow(),
        _ => return,
    };
    for (prop, value) in props.iter() {
        let Some(property) = tween_property(prop) else {
            continue;
        };
        let tag = format!("hscriptTween_{}_{}", object_tag_or_path(target), prop);
        let hargs = [
            HValue::from_str(tag),
            HValue::from_str(object_tag_or_path(target)),
            value.clone(),
            args.get(2).cloned().unwrap_or(HValue::Float(1.0)),
            HValue::from_str("linear"),
        ];
        add_tween(state, &hargs, property);
    }
}

fn get_property_path(state: &ScriptState, path: &str) -> HValue {
    match path {
        "health" | "game.health" => HValue::Float(state.health as f64),
        "score" | "songScore" => HValue::Int(state.score as i64),
        "misses" | "songMisses" => HValue::Int(state.misses as i64),
        "hits" => HValue::Int(state.hits as i64),
        "songPosition" | "Conductor.songPosition" => HValue::Float(state.song_position),
        "camera.zoom" | "camGame.zoom" => HValue::Float(state.camera_zoom as f64),
        "defaultCamZoom" | "defaultCamZoom.value" => HValue::Float(state.default_cam_zoom as f64),
        "BF_X" => HValue::Float(state.bf_group_pos.0 as f64),
        "BF_Y" => HValue::Float(state.bf_group_pos.1 as f64),
        "DAD_X" => HValue::Float(state.dad_group_pos.0 as f64),
        "DAD_Y" => HValue::Float(state.dad_group_pos.1 as f64),
        "GF_X" => HValue::Float(state.gf_group_pos.0 as f64),
        "GF_Y" => HValue::Float(state.gf_group_pos.1 as f64),
        "dad.x" => HValue::Float(state.dad_pos.0 as f64),
        "dad.y" => HValue::Float(state.dad_pos.1 as f64),
        "dadGroup.x" => HValue::Float(state.dad_group_pos.0 as f64),
        "dadGroup.y" => HValue::Float(state.dad_group_pos.1 as f64),
        "boyfriend.x" | "bf.x" => HValue::Float(state.bf_pos.0 as f64),
        "boyfriend.y" | "bf.y" => HValue::Float(state.bf_pos.1 as f64),
        "boyfriendGroup.x" => HValue::Float(state.bf_group_pos.0 as f64),
        "boyfriendGroup.y" => HValue::Float(state.bf_group_pos.1 as f64),
        "gf.x" | "girlfriend.x" => HValue::Float(state.gf_pos.0 as f64),
        "gf.y" | "girlfriend.y" => HValue::Float(state.gf_pos.1 as f64),
        "gfGroup.x" => HValue::Float(state.gf_group_pos.0 as f64),
        "gfGroup.y" => HValue::Float(state.gf_group_pos.1 as f64),
        _ => {
            if let Some(v) = state
                .property_values
                .get(path)
                .or_else(|| state.custom_vars.get(path))
            {
                return lua_value_to_h(v);
            }
            if let Some((tag, field)) = path.split_once('.') {
                if let Some(sprite) = state.lua_sprites.get(tag) {
                    return sprite_field(sprite, field);
                }
            }
            HValue::Null
        }
    }
}

fn set_property_path(state: &mut ScriptState, path: &str, value: &HValue) {
    let lua = h_value_to_lua(value);
    state.property_writes.push((path.to_string(), lua.clone()));
    state.custom_vars.insert(path.to_string(), lua.clone());
    match path {
        "health" | "game.health" => {
            state.health = value.as_f64().unwrap_or(state.health as f64) as f32
        }
        "score" | "songScore" => state.score = value.as_i64().unwrap_or(state.score as i64) as i32,
        "misses" | "songMisses" => {
            state.misses = value.as_i64().unwrap_or(state.misses as i64) as i32
        }
        "defaultCamZoom" | "defaultCamZoom.value" => {
            state.default_cam_zoom = value.as_f64().unwrap_or(state.default_cam_zoom as f64) as f32
        }
        _ => {
            if let Some((tag, field)) = path.split_once('.') {
                if let Some(sprite) = state.lua_sprites.get_mut(tag) {
                    set_sprite_field(sprite, field, value);
                }
            }
        }
    }
}

fn handle_name(value: &HValue) -> Option<&'static str> {
    match value {
        HValue::Handle { tag, id } if *tag == HOST_HANDLE_TAG => {
            HOST_HANDLES.get(*id as usize).copied()
        }
        _ => None,
    }
}

fn host_handle_id(name: &str) -> Option<u64> {
    HOST_HANDLES
        .iter()
        .position(|n| *n == name)
        .map(|i| i as u64)
}

fn handle_by_name(name: &str) -> HValue {
    host_handle(host_handle_id(name).unwrap_or(0))
}

fn host_handle(id: u64) -> HValue {
    HValue::Handle {
        tag: HOST_HANDLE_TAG,
        id,
    }
}

fn object_class(value: &HValue) -> Option<String> {
    obj_get(value, "__class").and_then(|v| v.as_str().map(str::to_string))
}

fn proxy(owner: &HValue, class: &str) -> HValue {
    let out = HValue::new_object();
    obj_set(&out, "__class", HValue::from_str(class));
    obj_set(&out, "__owner", owner.clone());
    out
}

fn point_proxy(owner: &HValue, x: &str, y: &str) -> HValue {
    let out = proxy(owner, "PointProxy");
    obj_set(&out, "__x", HValue::from_str(x));
    obj_set(&out, "__y", HValue::from_str(y));
    out
}

fn obj_get(value: &HValue, key: &str) -> Option<HValue> {
    match value {
        HValue::Object(map) => map.borrow().get(key).cloned(),
        _ => None,
    }
}

fn obj_set(value: &HValue, key: &str, val: HValue) {
    if let HValue::Object(map) = value {
        map.borrow_mut().insert(key.to_string(), val);
    }
}

fn obj_f64(value: &HValue, key: &str, default: f64) -> f64 {
    obj_get(value, key)
        .and_then(|v| v.as_f64())
        .unwrap_or(default)
}

fn obj_bool(value: &HValue, key: &str, default: bool) -> bool {
    match obj_get(value, key) {
        Some(HValue::Bool(v)) => v,
        _ => default,
    }
}

fn obj_str(value: &HValue, key: &str, default: &str) -> String {
    obj_get(value, key)
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| default.to_string())
}

fn add_object_anim(owner: &HValue, anim: &str, prefix: &str, fps: f32, looping: bool) {
    let animations = obj_get(owner, "__animations").unwrap_or_else(HValue::new_object);
    let def = HValue::new_object();
    obj_set(&def, "prefix", HValue::from_str(prefix));
    obj_set(&def, "fps", HValue::Float(fps as f64));
    obj_set(&def, "looping", HValue::Bool(looping));
    obj_set(&animations, anim, def);
    obj_set(owner, "__animations", animations);
}

fn next_tag(state: &mut ScriptState, prefix: &str) -> String {
    let next = state
        .custom_vars
        .get("__hscript_next_tag")
        .and_then(|v| match v {
            LuaValue::Int(i) => Some(*i),
            LuaValue::Float(f) => Some(*f as i64),
            _ => None,
        })
        .unwrap_or(0)
        + 1;
    state
        .custom_vars
        .insert("__hscript_next_tag".into(), LuaValue::Int(next));
    format!("{prefix}{next}")
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
        _ => LuaValue::Nil,
    }
}

fn arg_str(args: &[HValue], i: usize, default: &str) -> String {
    args.get(i)
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| default.to_string())
}

fn opt_arg_str(args: &[HValue], i: usize) -> Option<String> {
    args.get(i)
        .and_then(|v| v.as_str().map(str::to_string))
        .filter(|s| !s.is_empty())
}

fn arg_f64(args: &[HValue], i: usize, default: f64) -> f64 {
    args.get(i).and_then(HValue::as_f64).unwrap_or(default)
}

fn arg_i64(args: &[HValue], i: usize, default: i64) -> i64 {
    args.get(i).and_then(HValue::as_i64).unwrap_or(default)
}

fn arg_bool(args: &[HValue], i: usize, default: bool) -> bool {
    match args.get(i) {
        Some(HValue::Bool(v)) => *v,
        Some(v) => v.is_truthy(),
        None => default,
    }
}

fn is_property_path(name: &str) -> bool {
    name.contains('.')
}

fn short_type_name(type_name: &str) -> &str {
    type_name.rsplit('.').next().unwrap_or(type_name)
}

fn client_pref_value(state: &ScriptState, field: &str) -> HValue {
    match field {
        "downScroll" | "downscroll" => HValue::Bool(state.downscroll),
        "ghostTapping" => HValue::Bool(true),
        "flashing" | "flashingLights" => HValue::Bool(true),
        "shaders" => HValue::Bool(true),
        "framerate" => HValue::Int(60),
        _ => HValue::Null,
    }
}

fn h_args_to_lua(value: Option<&HValue>) -> Vec<LuaValue> {
    match value {
        Some(HValue::Array(arr)) => arr.borrow().iter().map(h_value_to_lua).collect(),
        Some(v) => vec![h_value_to_lua(v)],
        None => Vec::new(),
    }
}

fn h_value_to_camera(value: &HValue) -> String {
    match value {
        HValue::Array(arr) => arr
            .borrow()
            .first()
            .map(h_value_to_camera)
            .unwrap_or_else(|| "camGame".to_string()),
        HValue::String(s) => normalize_camera(s.as_str()),
        _ => handle_name(value)
            .map(normalize_camera)
            .unwrap_or_else(|| "camGame".to_string()),
    }
}

fn normalize_camera(camera: &str) -> String {
    match camera.to_ascii_lowercase().as_str() {
        "camhud" | "hud" | "game.camhud" => "camHUD".to_string(),
        "camother" | "other" | "game.camother" => "camOther".to_string(),
        _ => "camGame".to_string(),
    }
}

fn sprite_field(sprite: &LuaSprite, field: &str) -> HValue {
    match field {
        "x" => HValue::Float(sprite.x as f64),
        "y" => HValue::Float(sprite.y as f64),
        "alpha" => HValue::Float(sprite.alpha as f64),
        "visible" => HValue::Bool(sprite.visible),
        "angle" => HValue::Float(sprite.angle as f64),
        "width" => HValue::Float(sprite.tex_w as f64),
        "height" => HValue::Float(sprite.tex_h as f64),
        _ => HValue::Null,
    }
}

fn set_sprite_field(sprite: &mut LuaSprite, field: &str, value: &HValue) {
    match field {
        "x" => sprite.x = value.as_f64().unwrap_or(sprite.x as f64) as f32,
        "y" => sprite.y = value.as_f64().unwrap_or(sprite.y as f64) as f32,
        "alpha" => sprite.alpha = value.as_f64().unwrap_or(sprite.alpha as f64) as f32,
        "visible" => sprite.visible = matches!(value, HValue::Bool(true)),
        "angle" => sprite.angle = value.as_f64().unwrap_or(sprite.angle as f64) as f32,
        _ => {}
    }
}

fn get_str(map: &HashMap<String, HValue>, key: &str, default: &str) -> String {
    map.get(key)
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| default.to_string())
}

fn get_f64(map: &HashMap<String, HValue>, key: &str, default: f64) -> f64 {
    map.get(key).and_then(HValue::as_f64).unwrap_or(default)
}

fn get_bool(map: &HashMap<String, HValue>, key: &str, default: bool) -> bool {
    match map.get(key) {
        Some(HValue::Bool(v)) => *v,
        _ => default,
    }
}

fn tween_property(prop: &str) -> Option<TweenProperty> {
    match prop {
        "x" => Some(TweenProperty::X),
        "y" => Some(TweenProperty::Y),
        "alpha" => Some(TweenProperty::Alpha),
        "angle" => Some(TweenProperty::Angle),
        "zoom" => Some(TweenProperty::Zoom),
        _ => None,
    }
}

fn tween_prop_name(prop: &TweenProperty) -> &'static str {
    match prop {
        TweenProperty::X => "x",
        TweenProperty::Y => "y",
        TweenProperty::Alpha => "alpha",
        TweenProperty::Angle => "angle",
        TweenProperty::Zoom => "zoom",
        _ => "x",
    }
}

fn object_tag_or_path(value: &HValue) -> String {
    obj_get(value, "__added_tag")
        .or_else(|| obj_get(value, "__tag"))
        .and_then(|v| v.as_str().map(str::to_string))
        .or_else(|| handle_name(value).map(str::to_string))
        .unwrap_or_default()
}

fn reflect_field(args: &[HValue]) -> HValue {
    let Some(target) = args.first() else {
        return HValue::Null;
    };
    let field = arg_str(args, 1, "");
    if let Some(value) = obj_get(target, &field) {
        return value;
    }
    HValue::Null
}

fn rand_float(min: f64, max: f64) -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let unit = (nanos as f64 / u32::MAX as f64).clamp(0.0, 1.0);
    min + (max - min) * unit
}

fn rand_int(min: i64, max: i64) -> i64 {
    rand_float(min as f64, (max + 1) as f64).floor() as i64
}

fn color_name_to_int(name: &str) -> i64 {
    match name.to_ascii_uppercase().as_str() {
        "BLACK" => 0xFF000000,
        "BLUE" => 0xFF0000FF,
        "CYAN" => 0xFF00FFFF,
        "GRAY" | "GREY" => 0xFF808080,
        "GREEN" => 0xFF008000,
        "LIME" => 0xFF00FF00,
        "MAGENTA" | "PINK" => 0xFFFF00FF,
        "ORANGE" => 0xFFFFA500,
        "PURPLE" => 0xFF800080,
        "RED" => 0xFFFF0000,
        "TRANSPARENT" => 0x00000000,
        "WHITE" => 0xFFFFFFFF,
        "YELLOW" => 0xFFFFFF00,
        other => color_string_to_int(other),
    }
}

fn color_string_to_int(value: &str) -> i64 {
    let hex = value
        .trim()
        .trim_start_matches("FlxColor.")
        .trim_start_matches('#')
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    match hex.len() {
        6 => i64::from_str_radix(hex, 16)
            .map(|rgb| 0xFF000000 | rgb)
            .unwrap_or(0xFFFFFFFF),
        8 => i64::from_str_radix(hex, 16).unwrap_or(0xFFFFFFFF),
        _ => 0xFFFFFFFF,
    }
}
