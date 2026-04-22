use std::collections::HashMap;
use std::io::Read as _;
use std::sync::{Mutex, OnceLock};

use mlua::prelude::*;

static SAVE_DATA: OnceLock<Mutex<HashMap<String, HashMap<String, SaveValue>>>> = OnceLock::new();

#[derive(Clone, Debug)]
enum SaveValue {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<SaveValue>),
}

fn save_data() -> &'static Mutex<HashMap<String, HashMap<String, SaveValue>>> {
    SAVE_DATA.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolve a Psych Engine object path to our internal tween target name.
/// "strumLineNotes.members[N]" → "__strum_opponent_N" or "__strum_player_N"
/// "opponentStrums.members[N]" or "playerStrums.members[N]" → same
/// Otherwise returns the target as-is (Lua sprite tag).
fn resolve_strum_target(target: &str) -> String {
    // Helper: extract index N from "prefix[N]" or "prefix[N].sub.prop"
    let extract_idx = |s: &str| -> Option<usize> {
        let bracket = s.find(']')?;
        s[..bracket].parse().ok()
    };

    // strumLineNotes.members[N] — N is 0-7 (0-3 opponent, 4-7 player)
    // Also matches strumLineNotes.members[N].colorTransform etc.
    if let Some(rest) = target.strip_prefix("strumLineNotes.members[") {
        if let Some(idx) = extract_idx(rest) {
            return if idx < 4 {
                format!("__strum_opponent_{}", idx)
            } else {
                format!("__strum_player_{}", idx - 4)
            };
        }
    }
    // opponentStrums.members[N] (with optional .sub.prop)
    if let Some(rest) = target.strip_prefix("opponentStrums.members[") {
        if let Some(idx) = extract_idx(rest) {
            return format!("__strum_opponent_{}", idx);
        }
    }
    // playerStrums.members[N] (with optional .sub.prop)
    if let Some(rest) = target.strip_prefix("playerStrums.members[") {
        if let Some(idx) = extract_idx(rest) {
            return format!("__strum_player_{}", idx);
        }
    }
    target.to_string()
}

fn normalize_camera_offset_prop(prop: &str) -> Option<&'static str> {
    match prop {
        "opponentCameraOffset[0]" | "opponentCameraOffset.x" => Some("opponentCameraOffset.x"),
        "opponentCameraOffset[1]" | "opponentCameraOffset.y" => Some("opponentCameraOffset.y"),
        "boyfriendCameraOffset[0]" | "boyfriendCameraOffset.x" => Some("boyfriendCameraOffset.x"),
        "boyfriendCameraOffset[1]" | "boyfriendCameraOffset.y" => Some("boyfriendCameraOffset.y"),
        _ => None,
    }
}

fn normalize_lua_camera_name(camera: &str) -> String {
    match camera.trim().to_lowercase().as_str() {
        "" | "camgame" | "game" => "camGame".to_string(),
        "camhud" | "hud" => "camHUD".to_string(),
        "camother" | "other" => "camOther".to_string(),
        _ => camera.trim().to_string(),
    }
}

fn table_arg_string(args: &Option<LuaTable>, idx: i64) -> Option<String> {
    args.as_ref()
        .and_then(|t| t.get::<LuaValue>(idx).ok())
        .and_then(|v| match v {
            LuaValue::String(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
}

fn table_arg_f32(args: &Option<LuaTable>, idx: i64, default: f32) -> f32 {
    args.as_ref()
        .and_then(|t| t.get::<LuaValue>(idx).ok())
        .and_then(|v| match v {
            LuaValue::Number(n) => Some(n as f32),
            LuaValue::Integer(i) => Some(i as f32),
            LuaValue::String(s) => s.to_string_lossy().trim().parse::<f32>().ok(),
            _ => None,
        })
        .unwrap_or(default)
}

fn apply_haxe_clip_rect(
    lua: &Lua,
    args: &Option<LuaTable>,
    clip_right_side: bool,
) -> LuaResult<()> {
    let Some(tag) = table_arg_string(args, 1) else {
        return Ok(());
    };
    let off_x = table_arg_f32(args, 2, 0.0);
    let mult = table_arg_f32(args, 3, 1.0);
    let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
    let custom: LuaTable = lua.globals().get("__custom_vars")?;
    let divider_tag = custom
        .get::<String>("__clip_rect_divider_tag")
        .unwrap_or_default();
    if divider_tag.is_empty() {
        return Ok(());
    }
    let divider: LuaTable = match sprite_data.get(divider_tag.as_str()) {
        Ok(tbl) => tbl,
        Err(_) => return Ok(()),
    };
    let sprite: LuaTable = match sprite_data.get(tag.as_str()) {
        Ok(tbl) => tbl,
        Err(_) => return Ok(()),
    };

    let divider_x = divider.get::<f32>("x").unwrap_or(0.0);
    let cam_scroll_x = lua
        .globals()
        .get::<LuaValue>("__cam_game_scroll_x")
        .ok()
        .and_then(|v| lua_val_to_f32(&v))
        .unwrap_or(0.0);
    let scroll_x = sprite.get::<f32>("scroll_x").unwrap_or(1.0);
    let scale_x = sprite.get::<f32>("scale_x").unwrap_or(1.0);
    let frame_w = sprite
        .get::<f32>("frame_w")
        .or_else(|_| sprite.get::<f32>("tex_w"))
        .unwrap_or(0.0);
    let frame_h = sprite
        .get::<f32>("frame_h")
        .or_else(|_| sprite.get::<f32>("tex_h"))
        .unwrap_or(0.0);
    if frame_w <= 0.0 || frame_h <= 0.0 {
        return Ok(());
    }

    let value = (divider_x - cam_scroll_x * scroll_x * scale_x * mult + off_x).clamp(0.0, frame_w);
    sprite.set("clip_y", 0.0)?;
    sprite.set("clip_h", frame_h)?;
    if clip_right_side {
        sprite.set("clip_x", value)?;
        sprite.set("clip_w", (frame_w - value).max(0.0))?;
    } else {
        sprite.set("clip_x", 0.0)?;
        sprite.set("clip_w", value)?;
    }
    Ok(())
}

fn extract_quoted_argument<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let after_marker = text.find(marker)? + marker.len();
    let rel = &text[after_marker..];
    let quote_offset = rel.find(|c| c == '\'' || c == '"')?;
    let quote = rel.as_bytes()[quote_offset] as char;
    let value_start = after_marker + quote_offset + 1;
    let value_end = value_start + text[value_start..].find(quote)?;
    Some(&text[value_start..value_end])
}

/// Read width and height from a PNG file's IHDR chunk (first 24 bytes).
fn read_png_dimensions(path: &std::path::Path) -> Option<(u32, u32)> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 24];
    f.read_exact(&mut header).ok()?;
    if &header[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
    let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
    Some((width, height))
}

fn resolve_case_insensitive(root: &std::path::Path, relative: &str) -> Option<std::path::PathBuf> {
    let mut current = root.to_path_buf();
    if !current.exists() {
        return None;
    }

    let relative = relative.replace('\\', "/");
    let components: Vec<&str> = relative.split('/').filter(|s| !s.is_empty()).collect();

    for comp in components {
        let direct = current.join(comp);
        if direct.exists() {
            current = direct;
            continue;
        }

        // Try case-insensitive
        let Ok(entries) = std::fs::read_dir(&current) else {
            return None;
        };
        let mut found = false;
        let comp_lower = comp.to_lowercase();
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().to_lowercase() == comp_lower {
                current.push(entry.file_name());
                found = true;
                break;
            }
        }
        if !found {
            return None;
        }
    }
    Some(current)
}

/// Resolve an image name to a full path using the stored search roots.
fn resolve_image_path(lua: &Lua, image: &str) -> Option<std::path::PathBuf> {
    let roots: LuaTable = lua.globals().get("__image_roots").ok()?;
    let len = roots.len().ok()?;
    for i in 1..=len {
        let root: String = roots.get(i).ok()?;
        let p = std::path::PathBuf::from(&root).join(format!("images/{image}.png"));
        if p.exists() {
            return Some(p);
        }
        if let Some(ci) =
            resolve_case_insensitive(std::path::Path::new(&root), &format!("images/{image}.png"))
        {
            return Some(ci);
        }
    }
    None
}

/// Register all Psych Engine Lua API functions.
pub fn register_all(lua: &Lua) -> LuaResult<()> {
    // Initialize pending operation tables
    let g = lua.globals();
    g.set("__pending_sprites", lua.create_table()?)?;
    g.set("__pending_adds", lua.create_table()?)?;
    g.set("__pending_props", lua.create_table()?)?;
    g.set("__pending_removes", lua.create_table()?)?;
    g.set("__sprite_data", lua.create_table()?)?;
    g.set("__script_closed", false)?;
    g.set("__custom_vars", lua.create_table()?)?;
    g.set("__running_scripts", lua.create_table()?)?;
    // Initialize strum properties table (8 strums: opponent 0-3, player 4-7)
    let strum_props = lua.create_table()?;
    for i in 0..8 {
        let tbl = lua.create_table()?;
        tbl.set("x", 0.0)?;
        tbl.set("y", 0.0)?;
        tbl.set("alpha", 1.0)?;
        tbl.set("angle", 0.0)?;
        tbl.set("scale_x", 0.7)?;
        tbl.set("scale_y", 0.7)?;
        tbl.set("downScroll", false)?;
        tbl.set("custom", false)?;
        strum_props.set(i + 1, tbl)?;
    }
    g.set("__strum_props", strum_props)?;
    // Initialize note data tables for per-note manipulation
    g.set("__note_read_data", lua.create_table()?)?;
    g.set("__note_overrides", lua.create_table()?)?;
    g.set("__dirty_notes", lua.create_table()?)?;
    g.set("__pending_cam_sections", lua.create_table()?)?;
    g.set("__text_data", lua.create_table()?)?;
    g.set("__pending_texts", lua.create_table()?)?;
    g.set("__pending_text_adds", lua.create_table()?)?;
    g.set("__character_instances", lua.create_table()?)?;
    g.set("__pending_character_instances", lua.create_table()?)?;
    g.set("__pending_character_adds", lua.create_table()?)?;
    g.set("__pending_cam_fx", lua.create_table()?)?;
    g.set("__pending_subtitles", lua.create_table()?)?;
    g.set("__pending_char_positions", lua.create_table()?)?;
    g.set("__pending_audio", lua.create_table()?)?;
    g.set("__shader_data", lua.create_table()?)?;
    g.set("__instances", lua.create_table()?)?;
    g.set("__camera_values", lua.create_table()?)?;
    g.set("__sound_volumes", lua.create_table()?)?;
    g.set("__sound_times", lua.create_table()?)?;
    g.set("__sound_pitches", lua.create_table()?)?;
    g.set("__sound_exists", lua.create_table()?)?;
    g.set("__mouse_x", 0.0)?;
    g.set("__mouse_y", 0.0)?;
    g.set("__mouse_pressed", false)?;
    g.set("__mouse_just_pressed", false)?;
    g.set("__mouse_just_released", false)?;

    // Global variables scripts expect
    g.set("Function_Stop", 1)?;
    g.set("Function_Continue", 0)?;
    g.set("Function_StopLua", 2)?;
    g.set("Function_StopHScript", 3)?;
    g.set("Function_StopAll", 4)?;
    g.set("flashingLights", true)?;
    g.set("lowQuality", false)?;
    g.set("shadersEnabled", false)?;
    g.set("startedCountdown", false)?;
    g.set("startingSong", true)?;
    g.set("inGameOver", false)?;
    g.set("mustHitSection", false)?;
    g.set("botPlay", false)?;
    g.set("practice", false)?;
    g.set("downscroll", false)?;
    g.set("middlescroll", false)?;
    g.set("framerate", 60)?;
    g.set("ghostTapping", true)?;
    g.set("hideHud", false)?;
    g.set("cameraZoomOnBeat", true)?;
    g.set("scoreZoom", true)?;
    g.set("healthBarAlpha", 1.0)?;
    g.set("noteOffset", 0)?;
    g.set("noResetButton", false)?;
    g.set("defaultCamZoom", 0.9)?;
    g.set("cameraSpeed", 1.0)?;
    g.set("score", 0)?;
    g.set("misses", 0)?;
    g.set("hits", 0)?;
    g.set("combo", 0)?;
    g.set("rating", 0.0)?;
    g.set("ratingName", "?")?;
    g.set("ratingFC", "?")?;
    g.set("version", "0.7.3")?;
    g.set("luaDebugMode", false)?;
    g.set("luaDeprecatedWarnings", false)?;
    g.set("buildTarget", "linux")?;

    // Song/week globals (Psych Engine FunkinLua.hx lines 90-114)
    // Defaults — lua_engine.rs overrides with actual values when available
    g.set("curBpm", 120.0)?;
    g.set("bpm", 120.0)?;
    g.set("scrollSpeed", 1.0)?;
    g.set("crochet", 500.0)?;
    g.set("stepCrochet", 125.0)?;
    g.set("songLength", 0.0)?;
    g.set("songName", "")?;
    g.set("songPath", "")?;
    g.set("loadedSongName", "")?;
    g.set("loadedSongPath", "")?;
    g.set("chartPath", "")?;
    g.set("curStage", "")?;
    g.set("isStoryMode", false)?;
    g.set("difficulty", 1)?;
    g.set("difficultyName", "Normal")?;
    g.set("difficultyPath", "")?;
    g.set("difficultyNameTranslation", "Normal")?;
    g.set("weekRaw", 0)?;
    g.set("week", "")?;
    g.set("seenCutscene", false)?;
    g.set("hasVocals", true)?;
    g.set("deaths", 0)?;
    g.set("totalPlayed", 0)?;
    g.set("totalNotesHit", 0)?;
    g.set("guitarHeroSustains", false)?;
    g.set("instakillOnMiss", false)?;
    g.set("modFolder", "")?;
    g.set("altAnim", false)?;
    g.set("gfSection", false)?;
    g.set("healthGainMult", 1.0)?;
    g.set("healthLossMult", 1.0)?;
    g.set("playbackRate", 1.0)?;

    register_sprite_functions(lua)?;
    register_property_functions(lua)?;
    register_utility_functions(lua)?;
    register_tween_functions(lua)?;
    register_sound_functions(lua)?;
    register_text_functions(lua)?;
    register_window_functions(lua)?;
    register_note_type_function(lua)?;
    register_noop_stubs(lua)?;

    Ok(())
}

fn register_sprite_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // makeLuaSprite(tag, image, x, y)
    globals.set(
        "makeLuaSprite",
        lua.create_function(
            |lua, (tag, image, x, y): (String, LuaValue, Option<f64>, Option<f64>)| {
                let image_str = match &image {
                    LuaValue::String(s) => s.to_string_lossy().to_string(),
                    _ => String::new(),
                };
                let x = x.unwrap_or(0.0) as f32;
                let y = y.unwrap_or(0.0) as f32;

                let tbl = lua.create_table()?;
                tbl.set("tag", tag.clone())?;
                if image_str.is_empty() {
                    tbl.set("kind", "graphic")?;
                    tbl.set("width", 1)?;
                    tbl.set("height", 1)?;
                    tbl.set("color", "FFFFFF")?;
                    tbl.set("tex_w", 1.0)?;
                    tbl.set("tex_h", 1.0)?;
                } else {
                    tbl.set("kind", "image")?;
                    tbl.set("image", image_str.clone())?;
                    // Read PNG dimensions so getProperty('tag.width') works immediately
                    if let Some(path) = resolve_image_path(lua, &image_str) {
                        if let Some((w, h)) = read_png_dimensions(&path) {
                            tbl.set("tex_w", w as f64)?;
                            tbl.set("tex_h", h as f64)?;
                        }
                    }
                }
                tbl.set("x", x)?;
                tbl.set("y", y)?;
                tbl.set("scale_x", 1.0)?;
                tbl.set("scale_y", 1.0)?;
                tbl.set("scroll_x", 1.0)?;
                tbl.set("scroll_y", 1.0)?;
                tbl.set("alpha", 1.0)?;
                tbl.set("visible", true)?;
                tbl.set("flip_x", false)?;
                tbl.set("antialiasing", true)?;

                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                sprite_data.set(tag, tbl)?;
                Ok(())
            },
        )?,
    )?;

    // makeAnimatedLuaSprite(tag, image, x, y, spriteType)
    globals.set(
        "makeAnimatedLuaSprite",
        lua.create_function(
            |lua,
             (tag, image, x, y, _spr_type): (
                String,
                String,
                Option<f64>,
                Option<f64>,
                Option<String>,
            )| {
                let x = x.unwrap_or(0.0) as f32;
                let y = y.unwrap_or(0.0) as f32;

                let tbl = lua.create_table()?;
                tbl.set("tag", tag.clone())?;
                tbl.set("kind", "animated")?;
                tbl.set("image", image.clone())?;
                tbl.set("x", x)?;
                tbl.set("y", y)?;
                tbl.set("scale_x", 1.0)?;
                tbl.set("scale_y", 1.0)?;
                tbl.set("scroll_x", 1.0)?;
                tbl.set("scroll_y", 1.0)?;
                tbl.set("alpha", 1.0)?;
                tbl.set("visible", true)?;
                tbl.set("flip_x", false)?;
                tbl.set("antialiasing", true)?;
                // Read PNG atlas dimensions
                if let Some(path) = resolve_image_path(lua, &image) {
                    if let Some((w, h)) = read_png_dimensions(&path) {
                        tbl.set("tex_w", w as f64)?;
                        tbl.set("tex_h", h as f64)?;
                    }
                }

                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                sprite_data.set(tag, tbl)?;
                Ok(())
            },
        )?,
    )?;

    // makeGraphic(tag, width, height, color)
    globals.set(
        "makeGraphic",
        lua.create_function(
            |lua, (tag, width, height, color): (String, i32, i32, Option<String>)| {
                let color = color.unwrap_or_else(|| "FFFFFF".to_string());
                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                    tbl.set("kind", "graphic")?;
                    tbl.set("width", width)?;
                    tbl.set("height", height)?;
                    tbl.set("color", color)?;
                    tbl.set("tex_w", width as f64)?;
                    tbl.set("tex_h", height as f64)?;
                }
                Ok(())
            },
        )?,
    )?;

    // addLuaSprite(tag, inFront)
    globals.set(
        "addLuaSprite",
        lua.create_function(|lua, (tag, in_front): (String, Option<bool>)| {
            let in_front = in_front.unwrap_or(false);
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                let pending_sprites: LuaTable = lua.globals().get("__pending_sprites")?;
                let len = pending_sprites.len()? as i64;
                pending_sprites.set(len + 1, tbl)?;
            }
            let pending_adds: LuaTable = lua.globals().get("__pending_adds")?;
            let add_tbl = lua.create_table()?;
            add_tbl.set("tag", tag)?;
            add_tbl.set("in_front", in_front)?;
            let len = pending_adds.len()? as i64;
            pending_adds.set(len + 1, add_tbl)?;
            Ok(())
        })?,
    )?;

    // makeVideoSprite(tag, videoFile, x, y, camera, shouldLoop)
    //
    // Psych mods address the created object as "{tag}_video". Rustic does not
    // yet render videos as positioned Lua sprites, but exposing the sprite tag
    // keeps mod scripts alive and lets their order/camera/alpha mutations land
    // on a real object instead of aborting the callback.
    globals.set(
        "makeVideoSprite",
        lua.create_function(
            |lua,
             (tag, video_file, x, y, camera, should_loop): (
                String,
                String,
                Option<f64>,
                Option<f64>,
                Option<String>,
                Option<bool>,
            )| {
                let sprite_tag = format!("{}_video", tag);
                let x = x.unwrap_or(0.0) as f32;
                let y = y.unwrap_or(0.0) as f32;
                let camera = camera
                    .as_deref()
                    .map(normalize_lua_camera_name)
                    .unwrap_or_else(|| "camGame".to_string());

                let tbl = lua.create_table()?;
                tbl.set("tag", sprite_tag.clone())?;
                tbl.set("kind", "graphic")?;
                tbl.set("width", 1)?;
                tbl.set("height", 1)?;
                tbl.set("color", "000000")?;
                tbl.set("tex_w", 1.0)?;
                tbl.set("tex_h", 1.0)?;
                tbl.set("x", x)?;
                tbl.set("y", y)?;
                tbl.set("scale_x", 1.0)?;
                tbl.set("scale_y", 1.0)?;
                tbl.set("scroll_x", 1.0)?;
                tbl.set("scroll_y", 1.0)?;
                tbl.set("alpha", 0.0)?;
                tbl.set("visible", true)?;
                tbl.set("flip_x", false)?;
                tbl.set("antialiasing", true)?;
                tbl.set("camera", camera)?;
                tbl.set("video_file", video_file)?;
                tbl.set("should_loop", should_loop.unwrap_or(false))?;

                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                sprite_data.set(sprite_tag.clone(), tbl.clone())?;

                let pending_sprites: LuaTable = lua.globals().get("__pending_sprites")?;
                pending_sprites.set(pending_sprites.len()? + 1, tbl)?;

                let pending_adds: LuaTable = lua.globals().get("__pending_adds")?;
                let add_tbl = lua.create_table()?;
                add_tbl.set("tag", sprite_tag)?;
                add_tbl.set("in_front", true)?;
                pending_adds.set(pending_adds.len()? + 1, add_tbl)?;
                Ok(true)
            },
        )?,
    )?;

    // removeLuaSprite(tag, destroy)
    globals.set(
        "removeLuaSprite",
        lua.create_function(|lua, (tag, _destroy): (String, Option<bool>)| {
            // Mark sprite as invisible so it stops rendering
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                tbl.set("visible", false)?;
            }
            // Queue for removal by the game engine
            let pending: LuaTable = lua.globals().get("__pending_removes")?;
            let len = pending.len()? as i64;
            pending.set(len + 1, tag.clone())?;
            sprite_data.set(tag, LuaValue::Nil)?;
            Ok(())
        })?,
    )?;

    // scaleObject(tag, scaleX, scaleY)
    globals.set(
        "scaleObject",
        lua.create_function(|lua, (tag, sx, sy): (String, LuaValue, LuaValue)| {
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
                let scale_x = match sx {
                    LuaValue::Number(n) => n as f32,
                    LuaValue::Integer(i) => i as f32,
                    LuaValue::String(s) => s.to_str()?.parse::<f32>().unwrap_or(1.0),
                    _ => 1.0,
                };
                let scale_y = match sy {
                    LuaValue::Number(n) => n as f32,
                    LuaValue::Integer(i) => i as f32,
                    LuaValue::String(s) => s.to_str()?.parse::<f32>().unwrap_or(scale_x), // Default to scale_x if invalid
                    LuaValue::Nil => scale_x, // Default to scale_x if omitted
                    _ => scale_x,
                };
                tbl.set("scale_x", scale_x)?;
                tbl.set("scale_y", scale_y)?;
            }
            Ok(())
        })?,
    )?;

    // setGraphicSize(tag, width, height) — matches HaxeFlixel's setGraphicSize:
    // scale = newSize / frameSize, with aspect-ratio preservation when one arg is 0.
    globals.set(
        "setGraphicSize",
        lua.create_function(
            |lua, (tag, width, height): (String, Option<LuaValue>, Option<LuaValue>)| {
                let new_w = lua_to_f32(&width);
                let new_h = lua_to_f32(&height);
                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                    let tex_w: f32 = tbl.get("tex_w").unwrap_or(1.0);
                    let tex_h: f32 = tbl.get("tex_h").unwrap_or(1.0);
                    if tex_w > 0.0 && tex_h > 0.0 {
                        let mut sx = if new_w > 0.0 { new_w / tex_w } else { 0.0 };
                        let mut sy = if new_h > 0.0 { new_h / tex_h } else { 0.0 };
                        // Preserve aspect ratio when one dimension is 0
                        if new_w <= 0.0 {
                            sx = sy;
                        }
                        if new_h <= 0.0 {
                            sy = sx;
                        }
                        tbl.set("scale_x", sx)?;
                        tbl.set("scale_y", sy)?;
                    }
                }
                Ok(())
            },
        )?,
    )?;

    // updateHitbox(tag)
    globals.set(
        "updateHitbox",
        lua.create_function(|_lua, _tag: String| Ok(()))?,
    )?;

    // setScrollFactor(tag, x, y)
    globals.set(
        "setScrollFactor",
        lua.create_function(|lua, (tag, x, y): (String, f64, f64)| {
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                tbl.set("scroll_x", x as f32)?;
                tbl.set("scroll_y", y as f32)?;
            }

            // Queue for rust to process characters (which aren't in sprite_data)
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let len = pending.len()? as i64;
            let tbl = lua.create_table()?;
            tbl.set("prop", format!("__charScroll.{}", tag))?;
            let val = lua.create_table()?;
            val.set(1, x)?;
            val.set(2, y)?;
            tbl.set("value", val)?;
            pending.set(len + 1, tbl)?;
            Ok(())
        })?,
    )?;

    // setObjectOrder(tag, order)
    globals.set(
        "setObjectOrder",
        lua.create_function(|lua, (tag, order): (String, i32)| {
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                tbl.set("order", order)?;
            }

            // Queue for rust to process
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let len = pending.len()? as i64;
            let tbl = lua.create_table()?;
            tbl.set("prop", format!("__object_order.{}", tag))?;
            tbl.set("value", order)?;
            pending.set(len + 1, tbl)?;
            Ok(())
        })?,
    )?;

    // getObjectOrder(tag)
    globals.set(
        "getObjectOrder",
        lua.create_function(|lua, tag: String| -> LuaResult<i32> {
            if let Ok(orders) = lua.globals().get::<LuaTable>("__object_orders") {
                if let Ok(order) = orders.get::<i32>(tag.clone()) {
                    return Ok(order);
                }
            }
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                return Ok(tbl.get::<i32>("order").unwrap_or(0));
            }
            let character_instances: LuaTable = lua.globals().get("__character_instances")?;
            if let Ok(tbl) = character_instances.get::<LuaTable>(tag) {
                return Ok(tbl.get::<i32>("order").unwrap_or(0));
            }
            Ok(0)
        })?,
    )?;

    // addAnimationByPrefix(tag, anim, prefix, fps, looping)
    globals.set(
        "addAnimationByPrefix",
        lua.create_function(
            |lua,
             (tag, anim, prefix, fps, looping): (
                String,
                String,
                String,
                Option<f64>,
                Option<bool>,
            )| {
                let fps = fps.unwrap_or(24.0);
                let looping = looping.unwrap_or(true);
                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                    let anims = match tbl.get::<LuaTable>("__anims") {
                        Ok(t) => t,
                        Err(_) => {
                            let t = lua.create_table()?;
                            tbl.set("__anims", t.clone())?;
                            t
                        }
                    };
                    let anim_tbl = lua.create_table()?;
                    anim_tbl.set("prefix", prefix)?;
                    anim_tbl.set("fps", fps)?;
                    anim_tbl.set("looping", looping)?;
                    anims.set(anim, anim_tbl)?;
                }
                Ok(())
            },
        )?,
    )?;

    // addAnimationByIndices(tag, anim, prefix, indices, fps, looping)
    globals.set(
        "addAnimationByIndices",
        lua.create_function(
            |lua,
             (tag, anim, prefix, indices, fps, looping): (
                String,
                String,
                String,
                LuaValue,
                Option<f64>,
                Option<bool>,
            )| {
                let fps = fps.unwrap_or(24.0);
                let looping = looping.unwrap_or(true);
                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                    let anims = match tbl.get::<LuaTable>("__anims") {
                        Ok(t) => t,
                        Err(_) => {
                            let t = lua.create_table()?;
                            tbl.set("__anims", t.clone())?;
                            t
                        }
                    };
                    let anim_tbl = lua.create_table()?;
                    anim_tbl.set("prefix", prefix)?;
                    anim_tbl.set("fps", fps)?;
                    anim_tbl.set("looping", looping)?;

                    // Store indices as a Lua table, handling string inputs
                    match &indices {
                        LuaValue::String(s) => {
                            let parsed: LuaTable = lua.create_table()?;
                            for (i, part) in s.to_string_lossy().split(',').enumerate() {
                                if let Ok(idx) = part.trim().parse::<i32>() {
                                    parsed.set(i + 1, idx)?;
                                }
                            }
                            anim_tbl.set("indices", parsed)?;
                        }
                        LuaValue::Table(t) => {
                            anim_tbl.set("indices", t.clone())?;
                        }
                        _ => {}
                    }
                    anims.set(anim, anim_tbl)?;
                }
                Ok(())
            },
        )?,
    )?;

    // addAnimationByIndicesLoop(tag, anim, prefix, indices, fps) — same as addAnimationByIndices with looping=true
    // Accepts indices as either a LuaTable or a comma-separated string
    globals.set("addAnimationByIndicesLoop", lua.create_function(|lua, (tag, anim, prefix, indices, fps): (String, String, String, LuaValue, Option<f64>)| {
        let fps = fps.unwrap_or(24.0);
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let anims = match tbl.get::<LuaTable>("__anims") {
                Ok(t) => t,
                Err(_) => {
                    let t = lua.create_table()?;
                    tbl.set("__anims", t.clone())?;
                    t
                }
            };
            let anim_tbl = lua.create_table()?;
            anim_tbl.set("prefix", prefix)?;
            anim_tbl.set("fps", fps)?;
            anim_tbl.set("looping", true)?;
            // Parse indices: accept both LuaTable and comma-separated string
            match &indices {
                LuaValue::String(s) => {
                    let parsed: LuaTable = lua.create_table()?;
                    for (i, part) in s.to_string_lossy().split(',').enumerate() {
                        if let Ok(idx) = part.trim().parse::<i32>() {
                            parsed.set(i + 1, idx)?;
                        }
                    }
                    anim_tbl.set("indices", parsed)?;
                }
                LuaValue::Table(t) => {
                    anim_tbl.set("indices", t.clone())?;
                }
                _ => {}
            }
            anims.set(anim, anim_tbl)?;
        }
        Ok(())
    })?)?;

    // addAnimation(tag, anim, frames, fps, looping) — same as addAnimationByPrefix for our purposes
    globals.set(
        "addAnimation",
        lua.create_function(
            |lua,
             (tag, anim, prefix, fps, looping): (
                String,
                String,
                LuaValue,
                Option<f64>,
                Option<bool>,
            )| {
                let fps = fps.unwrap_or(24.0);
                let looping = looping.unwrap_or(true);
                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                    let anims = match tbl.get::<LuaTable>("__anims") {
                        Ok(t) => t,
                        Err(_) => {
                            let t = lua.create_table()?;
                            tbl.set("__anims", t.clone())?;
                            t
                        }
                    };
                    let anim_tbl = lua.create_table()?;

                    // Handle prefix gracefully whether it's passed as a string or a table
                    match &prefix {
                        LuaValue::String(s) => {
                            anim_tbl.set("prefix", s.to_str()?)?;
                        }
                        LuaValue::Table(t) => {
                            // if they pass a table to addAnimation, they meant addAnimationByIndices.
                            anim_tbl.set("prefix", "")?;
                            anim_tbl.set("indices", t.clone())?;
                        }
                        _ => {
                            anim_tbl.set("prefix", "")?;
                        }
                    }

                    anim_tbl.set("fps", fps)?;
                    anim_tbl.set("looping", looping)?;
                    anims.set(anim, anim_tbl)?;
                }
                Ok(())
            },
        )?,
    )?;

    // addOffset(tag, anim, x, y)
    globals.set(
        "addOffset",
        lua.create_function(
            |lua, (tag, anim, x, y): (String, String, Option<f64>, Option<f64>)| {
                let x = x.unwrap_or(0.0);
                let y = y.unwrap_or(0.0);
                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
                    let offsets = match tbl.get::<LuaTable>("__offsets") {
                        Ok(t) => t,
                        Err(_) => {
                            let t = lua.create_table()?;
                            tbl.set("__offsets", t.clone())?;
                            t
                        }
                    };
                    let off_tbl = lua.create_table()?;
                    off_tbl.set("x", x)?;
                    off_tbl.set("y", y)?;
                    offsets.set(anim, off_tbl)?;
                }
                Ok(())
            },
        )?,
    )?;

    // playAnim(tag, anim, forced, reversed, frame)
    globals.set(
        "playAnim",
        lua.create_function(
            |lua,
             (tag, anim, forced, _reversed, frame): (
                String,
                String,
                Option<bool>,
                Option<bool>,
                Option<i32>,
            )| {
                let forced = forced.unwrap_or(false);
                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                    let play_tbl = lua.create_table()?;
                    play_tbl.set("anim", anim)?;
                    play_tbl.set("forced", forced)?;
                    if let Some(frame) = frame {
                        play_tbl.set("frame", frame)?;
                    }
                    tbl.set("__pending_anim", play_tbl)?;
                    return Ok(());
                }
                let character_instances: LuaTable = lua.globals().get("__character_instances")?;
                if let Ok(tbl) = character_instances.get::<LuaTable>(tag.clone()) {
                    tbl.set("current_anim", anim.clone())?;
                    tbl.set("anim_finished", false)?;
                    set_sprite_animation_value(
                        lua,
                        &tbl,
                        "name",
                        LuaValue::String(lua.create_string(&anim)?),
                    )?;
                    set_sprite_animation_value(lua, &tbl, "finished", LuaValue::Boolean(false))?;
                    let pending: LuaTable = lua.globals().get("__pending_props")?;
                    let queued = lua.create_table()?;
                    queued.set(
                        "prop",
                        format!(
                            "__luaCharacterPlayAnim{}{}",
                            if forced { "." } else { "Soft." },
                            tag
                        ),
                    )?;
                    queued.set("value", anim)?;
                    pending.set(pending.len()? + 1, queued)?;
                }
                Ok(())
            },
        )?,
    )?;

    // objectPlayAnimation — alias for playAnim
    globals.set(
        "objectPlayAnimation",
        lua.create_function(|lua, (tag, anim, forced): (String, String, Option<bool>)| {
            let forced = forced.unwrap_or(false);
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                let play_tbl = lua.create_table()?;
                play_tbl.set("anim", anim)?;
                play_tbl.set("forced", forced)?;
                tbl.set("__pending_anim", play_tbl)?;
                return Ok(());
            }
            let character_instances: LuaTable = lua.globals().get("__character_instances")?;
            if let Ok(tbl) = character_instances.get::<LuaTable>(tag.clone()) {
                tbl.set("current_anim", anim.clone())?;
                tbl.set("anim_finished", false)?;
                set_sprite_animation_value(
                    lua,
                    &tbl,
                    "name",
                    LuaValue::String(lua.create_string(&anim)?),
                )?;
                set_sprite_animation_value(lua, &tbl, "finished", LuaValue::Boolean(false))?;
                let pending: LuaTable = lua.globals().get("__pending_props")?;
                let queued = lua.create_table()?;
                queued.set(
                    "prop",
                    format!(
                        "__luaCharacterPlayAnim{}{}",
                        if forced { "." } else { "Soft." },
                        tag
                    ),
                )?;
                queued.set("value", anim)?;
                pending.set(pending.len()? + 1, queued)?;
            }
            Ok(())
        })?,
    )?;

    // characterPlayAnim(charType, anim, forced) — queue character anim via pending props
    globals.set(
        "characterPlayAnim",
        lua.create_function(
            |lua, (char_type, anim, forced): (String, String, Option<bool>)| {
                let forced = forced.unwrap_or(false);
                // Queue as a property write for the app layer to handle
                let pending: LuaTable = lua.globals().get("__pending_props")?;
                let tbl = lua.create_table()?;
                let prop = format!(
                    "__charPlayAnim{}.{}",
                    if forced { "" } else { "Soft" },
                    char_type
                );
                tbl.set("prop", prop)?;
                tbl.set("value", anim)?;
                pending.set(pending.len()? + 1, tbl)?;
                Ok(())
            },
        )?,
    )?;

    // addProperty(name, defaultValue) — creates a property (stored as custom var)
    globals.set(
        "addProperty",
        lua.create_function(|lua, (name, value): (String, LuaValue)| {
            let vars: LuaTable = lua.globals().get("__custom_vars")?;
            // Only set if not already defined
            let existing: LuaValue = vars.get(name.as_str())?;
            if matches!(existing, LuaNil) {
                vars.set(name, value)?;
            }
            Ok(())
        })?,
    )?;

    // screenCenter(tag, axis)
    globals.set(
        "screenCenter",
        lua.create_function(|lua, (tag, axis): (String, Option<String>)| {
            let axis = axis.unwrap_or_else(|| "xy".to_string());
            let sw: f32 = lua.globals().get("screenWidth").unwrap_or(1280.0);
            let sh: f32 = lua.globals().get("screenHeight").unwrap_or(720.0);
            // Try sprite data first
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                let tex_w: f32 = tbl.get("tex_w").unwrap_or(0.0);
                let tex_h: f32 = tbl.get("tex_h").unwrap_or(0.0);
                let sx: f32 = tbl.get("scale_x").unwrap_or(1.0);
                let sy: f32 = tbl.get("scale_y").unwrap_or(1.0);
                if axis.contains('x') || axis.contains('X') {
                    tbl.set("x", (sw - tex_w * sx.abs()) / 2.0)?;
                }
                if axis.contains('y') || axis.contains('Y') {
                    tbl.set("y", (sh - tex_h * sy.abs()) / 2.0)?;
                }
                return Ok(());
            }
            // Try text data
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                // Text objects don't have tex_w/tex_h; approximate with width/size
                if axis.contains('x') || axis.contains('X') {
                    tbl.set("x", sw / 2.0)?; // center horizontally (alignment handles the rest)
                }
                if axis.contains('y') || axis.contains('Y') {
                    let size: f32 = tbl.get("size").unwrap_or(16.0);
                    tbl.set("y", (sh - size) / 2.0)?;
                }
            }
            Ok(())
        })?,
    )?;

    // setObjectCamera(tag, camera)
    globals.set(
        "setObjectCamera",
        lua.create_function(|lua, (tag, cam): (String, Option<String>)| {
            let cam = cam.unwrap_or_else(|| "camGame".to_string());
            let cam_name = normalize_lua_camera_name(&cam);
            // Set on sprite data
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                tbl.set("camera", cam_name.as_str())?;
            }
            // Also set on text data
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                tbl.set("camera", cam_name.as_str())?;
            }
            Ok(())
        })?,
    )?;

    // luaSpriteExists(tag)
    globals.set(
        "luaSpriteExists",
        lua.create_function(|lua, tag: String| -> LuaResult<bool> {
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            Ok(sprite_data.contains_key(tag)?)
        })?,
    )?;

    // setBlendMode(tag, blend)
    globals.set(
        "setBlendMode",
        lua.create_function(|lua, (tag, blend): (String, Option<String>)| {
            let blend = blend.unwrap_or_else(|| "NORMAL".to_string());
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
                tbl.set("_prop_blendMode", blend.clone())?;
            }
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                tbl.set("_prop_blendMode", blend)?;
            }
            Ok(())
        })?,
    )?;

    // loadGraphic(tag, image, ?width, ?height)
    // In Psych Engine this reloads (and optionally crops) the graphic for a sprite.
    globals.set(
        "loadGraphic",
        lua.create_function(
            |lua,
             (tag, image, width, height): (
                String,
                Option<String>,
                Option<f64>,
                Option<f64>,
            )| {
                if let Ok(tbl) = lua
                    .globals()
                    .get::<LuaTable>("__sprite_data")?
                    .get::<LuaTable>(tag.as_str())
                {
                    if let Some(image) = image.filter(|image| !image.is_empty()) {
                        tbl.set("kind", "image")?;
                        tbl.set("image", image.as_str())?;
                        update_lua_sprite_dimensions(lua, &tbl, &image)?;
                        queue_lua_sprite_reload(lua, &tag, false)?;
                    }
                    if let (Some(w), Some(h)) = (width, height) {
                        tbl.set("crop_w", w)?;
                        tbl.set("crop_h", h)?;
                    }
                }
                Ok(())
            },
        )?,
    )?;

    // updateHitboxFromGroup(group, index) — deprecated Psych Engine function
    // Calls updateHitbox on a member of a FlxTypedGroup (e.g. unspawnNotes[i]).
    globals.set(
        "updateHitboxFromGroup",
        lua.create_function(|_lua, (_group, _index): (LuaValue, LuaValue)| {
            // No-op: our note hitbox recalculation happens automatically on scale changes
            Ok(())
        })?,
    )?;

    // getColorFromRGB(r, g, b) — returns an integer color value
    globals.set(
        "getColorFromRGB",
        lua.create_function(|_lua, (r, g, b): (i32, i32, i32)| -> LuaResult<i64> {
            Ok(((r.clamp(0, 255) as i64) << 16)
                | ((g.clamp(0, 255) as i64) << 8)
                | (b.clamp(0, 255) as i64))
        })?,
    )?;

    // setSongTime(time) — seek to a position in the song (ms)
    globals.set(
        "setSongTime",
        lua.create_function(|lua, time: f64| {
            let pending: LuaTable = lua.globals().get("__pending_audio")?;
            pending.set("seek_to", time)?;
            Ok(())
        })?,
    )?;

    // reloadHealthBarColors() — no-op for now, health bar colors are managed differently
    globals.set(
        "reloadHealthBarColors",
        lua.create_function(|_lua, ()| Ok(()))?,
    )?;

    // changeIcon(character) — change the health icon for opponent/player
    globals.set(
        "changeIcon",
        lua.create_function(|_lua, _character: String| Ok(()))?,
    )?;

    // makeRating(sprite stuff) — create a rating popup sprite
    globals.set(
        "makeRating",
        lua.create_function(|_lua, _args: LuaMultiValue| Ok(()))?,
    )?;

    Ok(())
}

fn register_property_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // setProperty(property, value)
    globals.set(
        "setProperty",
        lua.create_function(|lua, (prop, value): (String, LuaValue)| {
            let prop = normalize_camera_offset_prop(&prop)
                .map(str::to_string)
                .unwrap_or(prop);
            // Check if it's a sprite property (tag.field) — also handles nested like tag.origin.y
            if let Some(dot_pos) = prop.find('.') {
                let tag = &prop[..dot_pos];
                let field = &prop[dot_pos + 1..];
                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.to_string()) {
                    match field {
                        "alpha" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("alpha", n)?;
                            }
                        }
                        "visible" => {
                            if let LuaValue::Boolean(b) = &value {
                                tbl.set("visible", *b)?;
                            }
                        }
                        "flipX" | "flip_x" => {
                            if let LuaValue::Boolean(b) = &value {
                                tbl.set("flip_x", *b)?;
                            }
                        }
                        "flipY" | "flip_y" => {
                            if let LuaValue::Boolean(b) = &value {
                                tbl.set("flip_y", *b)?;
                            }
                        }
                        "antialiasing" => {
                            if let LuaValue::Boolean(b) = &value {
                                tbl.set("antialiasing", *b)?;
                            }
                        }
                        "x" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("x", n)?;
                            }
                        }
                        "y" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("y", n)?;
                            }
                        }
                        "angle" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("angle", n)?;
                            }
                        }
                        "scale.x" | "scaleX" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("scale_x", n)?;
                            }
                        }
                        "scale.y" | "scaleY" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("scale_y", n)?;
                            }
                        }
                        "scrollFactor.x" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("scroll_x", n)?;
                            }
                        }
                        "scrollFactor.y" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("scroll_y", n)?;
                            }
                        }
                        "origin.x" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("origin_x", n)?;
                            }
                        }
                        "origin.y" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("origin_y", n)?;
                            }
                        }
                        "offset.x" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("offset_x", n)?;
                            }
                        }
                        "offset.y" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("offset_y", n)?;
                            }
                        }
                        "colorTransform.redOffset" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("ct_red", n)?;
                            }
                        }
                        "colorTransform.greenOffset" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("ct_green", n)?;
                            }
                        }
                        "colorTransform.blueOffset" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("ct_blue", n)?;
                            }
                        }
                        "color" => {
                            tbl.set("color", lua_value_to_color_string(&value))?;
                        }
                        "animation.frameIndex" | "animation.curAnim.curFrame" => {
                            if let Some(n) = lua_val_to_i64(&value) {
                                tbl.set("anim_frame", n.max(0))?;
                                set_sprite_animation_value(
                                    lua,
                                    &tbl,
                                    "curFrame",
                                    LuaValue::Integer(n.max(0)),
                                )?;
                                set_sprite_animation_value(
                                    lua,
                                    &tbl,
                                    "finished",
                                    LuaValue::Boolean(false),
                                )?;
                            }
                        }
                        "animation.name" | "animation.curAnim.name" => {
                            tbl.set("anim_finished", false)?;
                            set_sprite_animation_value(lua, &tbl, "name", value.clone())?;
                        }
                        "animation.finished" | "animation.curAnim.finished" => {
                            if let LuaValue::Boolean(b) = &value {
                                set_sprite_animation_value(
                                    lua,
                                    &tbl,
                                    "finished",
                                    LuaValue::Boolean(*b),
                                )?;
                            }
                        }
                        "animation.framerate" | "animation.curAnim.frameRate" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("anim_fps", n)?;
                            }
                        }
                        _ => {
                            tbl.set(format!("_prop_{field}"), value.clone())?;
                        }
                    }
                    return Ok(());
                }
                // Also check text objects
                let text_data: LuaTable = lua.globals().get("__text_data")?;
                if let Ok(tbl) = text_data.get::<LuaTable>(tag.to_string()) {
                    match field {
                        "alpha" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("alpha", n)?;
                            }
                        }
                        "visible" => {
                            if let LuaValue::Boolean(b) = &value {
                                tbl.set("visible", *b)?;
                            }
                        }
                        "x" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("x", n)?;
                            }
                        }
                        "y" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("y", n)?;
                            }
                        }
                        "angle" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("angle", n)?;
                            }
                        }
                        "antialiasing" => {
                            if let LuaValue::Boolean(b) = &value {
                                tbl.set("antialiasing", *b)?;
                            }
                        }
                        _ => {
                            tbl.set(format!("_prop_{field}"), value.clone())?;
                        }
                    }
                    return Ok(());
                }
                // Reflection-created Character instances (e.g. objects.Character).
                let character_instances: LuaTable = lua.globals().get("__character_instances")?;
                if let Ok(tbl) = character_instances.get::<LuaTable>(tag.to_string()) {
                    match field {
                        "alpha" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("alpha", n)?;
                            }
                        }
                        "visible" => {
                            if let LuaValue::Boolean(b) = &value {
                                tbl.set("visible", *b)?;
                            }
                        }
                        "x" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("x", n)?;
                            }
                        }
                        "y" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("y", n)?;
                            }
                        }
                        "scale.x" | "scaleX" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("scale_x", n)?;
                            }
                        }
                        "scale.y" | "scaleY" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("scale_y", n)?;
                            }
                        }
                        "animation.name" | "animation.curAnim.name" => {
                            tbl.set("current_anim", value.clone())?;
                            tbl.set("anim_finished", false)?;
                            set_sprite_animation_value(lua, &tbl, "name", value.clone())?;
                        }
                        "animation.finished" | "animation.curAnim.finished" => {
                            if let LuaValue::Boolean(b) = &value {
                                tbl.set("anim_finished", *b)?;
                                set_sprite_animation_value(
                                    lua,
                                    &tbl,
                                    "finished",
                                    LuaValue::Boolean(*b),
                                )?;
                            }
                        }
                        "holdTimer" | "hold_timer" => {
                            if let Some(n) = lua_val_to_f32(&value) {
                                tbl.set("holdTimer", n)?;
                            }
                        }
                        _ => {
                            tbl.set(format!("_prop_{field}"), value.clone())?;
                        }
                    }

                    let pending: LuaTable = lua.globals().get("__pending_props")?;
                    let queued = lua.create_table()?;
                    queued.set("prop", prop)?;
                    queued.set("value", value)?;
                    pending.set(pending.len()? + 1, queued)?;
                    return Ok(());
                }
            }

            let g = lua.globals();

            // Update Lua global if it's a known game property
            match prop.as_str() {
                "defaultCamZoom"
                | "cameraSpeed"
                | "camZooming"
                | "camZoomingMult"
                | "camZoomingDecay"
                | "gameZoomingDecay"
                | "crochet"
                | "stepCrochet"
                | "isCameraOnForcedPos"
                | "health" => {
                    g.set(prop.as_str(), value.clone()).ok();
                    if prop == "health" {
                        g.set("__health", value.clone()).ok();
                    }
                }
                "opponentCameraOffset.x" => {
                    g.set("__opponent_camera_offset_x", value.clone()).ok();
                    if let Ok(custom) = g.get::<LuaTable>("__custom_vars") {
                        custom.set(prop.as_str(), value.clone()).ok();
                    }
                }
                "opponentCameraOffset.y" => {
                    g.set("__opponent_camera_offset_y", value.clone()).ok();
                    if let Ok(custom) = g.get::<LuaTable>("__custom_vars") {
                        custom.set(prop.as_str(), value.clone()).ok();
                    }
                }
                "boyfriendCameraOffset.x" => {
                    g.set("__bf_camera_offset_x", value.clone()).ok();
                    if let Ok(custom) = g.get::<LuaTable>("__custom_vars") {
                        custom.set(prop.as_str(), value.clone()).ok();
                    }
                }
                "boyfriendCameraOffset.y" => {
                    g.set("__bf_camera_offset_y", value.clone()).ok();
                    if let Ok(custom) = g.get::<LuaTable>("__custom_vars") {
                        custom.set(prop.as_str(), value.clone()).ok();
                    }
                }
                // Array properties: split table values into separate writes
                "opponentCameraOffset" | "boyfriendCameraOffset" => {
                    if let LuaValue::Table(tbl) = &value {
                        let x: f64 = tbl.get(1).unwrap_or(0.0);
                        let y: f64 = tbl.get(2).unwrap_or(0.0);
                        let pending: LuaTable = g.get("__pending_props")?;
                        let t1 = lua.create_table()?;
                        t1.set("prop", format!("{}.x", prop))?;
                        t1.set("value", x)?;
                        let len = pending.len()? as i64;
                        pending.set(len + 1, t1)?;
                        let t2 = lua.create_table()?;
                        t2.set("prop", format!("{}.y", prop))?;
                        t2.set("value", y)?;
                        pending.set(len + 2, t2)?;
                        return Ok(());
                    }
                    // Fall through for non-table values
                    if let Ok(custom) = g.get::<LuaTable>("__custom_vars") {
                        custom.set(prop.as_str(), value.clone()).ok();
                    }
                }
                _ => {
                    // Store in custom vars so getProperty can read it back
                    if let Ok(custom) = g.get::<LuaTable>("__custom_vars") {
                        custom.set(prop.as_str(), value.clone()).ok();
                    }
                }
            }

            // Queue as a game property write (engine processes known ones like defaultCamZoom)
            let pending: LuaTable = g.get("__pending_props")?;
            let tbl = lua.create_table()?;
            tbl.set("prop", prop)?;
            tbl.set("value", value)?;
            let len = pending.len()? as i64;
            pending.set(len + 1, tbl)?;
            Ok(())
        })?,
    )?;

    // getProperty(property) -> value
    globals.set(
        "getProperty",
        lua.create_function(|lua, prop: String| -> LuaResult<LuaValue> {
            if let Some(dot_pos) = prop.find('.') {
                let tag = &prop[..dot_pos];
                let field = &prop[dot_pos + 1..];
                let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.to_string()) {
                    return match field {
                        "x" => Ok(tbl.get::<LuaValue>("x").unwrap_or(LuaValue::Number(0.0))),
                        "y" => Ok(tbl.get::<LuaValue>("y").unwrap_or(LuaValue::Number(0.0))),
                        "alpha" => Ok(tbl
                            .get::<LuaValue>("alpha")
                            .unwrap_or(LuaValue::Number(1.0))),
                        "visible" => Ok(tbl
                            .get::<LuaValue>("visible")
                            .unwrap_or(LuaValue::Boolean(true))),
                        "angle" => Ok(tbl
                            .get::<LuaValue>("angle")
                            .unwrap_or(LuaValue::Number(0.0))),
                        "flipX" => Ok(tbl
                            .get::<LuaValue>("flip_x")
                            .unwrap_or(LuaValue::Boolean(false))),
                        "flipY" => Ok(tbl
                            .get::<LuaValue>("flip_y")
                            .unwrap_or(LuaValue::Boolean(false))),
                        "scale.x" | "scaleX" => Ok(tbl
                            .get::<LuaValue>("scale_x")
                            .unwrap_or(LuaValue::Number(1.0))),
                        "scale.y" | "scaleY" => Ok(tbl
                            .get::<LuaValue>("scale_y")
                            .unwrap_or(LuaValue::Number(1.0))),
                        "scrollFactor.x" => Ok(tbl
                            .get::<LuaValue>("scroll_x")
                            .unwrap_or(LuaValue::Number(1.0))),
                        "scrollFactor.y" => Ok(tbl
                            .get::<LuaValue>("scroll_y")
                            .unwrap_or(LuaValue::Number(1.0))),
                        "antialiasing" => Ok(tbl
                            .get::<LuaValue>("antialiasing")
                            .unwrap_or(LuaValue::Boolean(true))),
                        "origin.x" => Ok(tbl.get::<LuaValue>("origin_x").unwrap_or(LuaNil)),
                        "origin.y" => Ok(tbl.get::<LuaValue>("origin_y").unwrap_or(LuaNil)),
                        "offset.x" => Ok(tbl
                            .get::<LuaValue>("offset_x")
                            .unwrap_or(LuaValue::Number(0.0))),
                        "offset.y" => Ok(tbl
                            .get::<LuaValue>("offset_y")
                            .unwrap_or(LuaValue::Number(0.0))),
                        "colorTransform.redOffset" => Ok(tbl
                            .get::<LuaValue>("ct_red")
                            .unwrap_or(LuaValue::Number(0.0))),
                        "colorTransform.greenOffset" => Ok(tbl
                            .get::<LuaValue>("ct_green")
                            .unwrap_or(LuaValue::Number(0.0))),
                        "colorTransform.blueOffset" => Ok(tbl
                            .get::<LuaValue>("ct_blue")
                            .unwrap_or(LuaValue::Number(0.0))),
                        "color" => Ok(tbl
                            .get::<LuaValue>("color")
                            .unwrap_or(LuaValue::String(lua.create_string("FFFFFF")?))),
                        // HaxeFlixel: width = frameWidth * abs(scale.x)
                        "width" => {
                            let tex_w: f64 = tbl.get("tex_w").unwrap_or(0.0);
                            let scale_x: f64 = tbl.get("scale_x").unwrap_or(1.0);
                            Ok(LuaValue::Number(tex_w * scale_x.abs()))
                        }
                        "height" => {
                            let tex_h: f64 = tbl.get("tex_h").unwrap_or(0.0);
                            let scale_y: f64 = tbl.get("scale_y").unwrap_or(1.0);
                            Ok(LuaValue::Number(tex_h * scale_y.abs()))
                        }
                        "animation.frameIndex" | "animation.curAnim.curFrame" => Ok(tbl
                            .get::<LuaValue>("anim_frame")
                            .unwrap_or(LuaValue::Integer(0))),
                        "animation.name" | "animation.curAnim.name" => Ok(sprite_animation_value(
                            lua,
                            &tbl,
                            "name",
                            LuaValue::String(lua.create_string("")?),
                        )?),
                        "animation.finished" | "animation.curAnim.finished" => {
                            Ok(sprite_animation_value(
                                lua,
                                &tbl,
                                "finished",
                                LuaValue::Boolean(false),
                            )?)
                        }
                        "animation.framerate" | "animation.curAnim.frameRate" => Ok(tbl
                            .get::<LuaValue>("anim_fps")
                            .unwrap_or(LuaValue::Number(24.0))),
                        _ => Ok(tbl
                            .get::<LuaValue>(format!("_prop_{field}"))
                            .unwrap_or(LuaNil)),
                    };
                }
                // Also check text objects
                let text_data: LuaTable = lua.globals().get("__text_data")?;
                if let Ok(tbl) = text_data.get::<LuaTable>(tag.to_string()) {
                    return match field {
                        "x" => Ok(tbl.get::<LuaValue>("x").unwrap_or(LuaValue::Number(0.0))),
                        "y" => Ok(tbl.get::<LuaValue>("y").unwrap_or(LuaValue::Number(0.0))),
                        "alpha" => Ok(tbl
                            .get::<LuaValue>("alpha")
                            .unwrap_or(LuaValue::Number(1.0))),
                        "visible" => Ok(tbl
                            .get::<LuaValue>("visible")
                            .unwrap_or(LuaValue::Boolean(true))),
                        "angle" => Ok(tbl
                            .get::<LuaValue>("angle")
                            .unwrap_or(LuaValue::Number(0.0))),
                        "width" => Ok(tbl
                            .get::<LuaValue>("width")
                            .unwrap_or(LuaValue::Number(0.0))),
                        "text" => Ok(tbl
                            .get::<LuaValue>("text")
                            .unwrap_or(LuaValue::String(lua.create_string("")?))),
                        "size" => Ok(tbl
                            .get::<LuaValue>("size")
                            .unwrap_or(LuaValue::Number(16.0))),
                        "font" => Ok(tbl
                            .get::<LuaValue>("font")
                            .unwrap_or(LuaValue::String(lua.create_string("")?))),
                        "borderSize" => Ok(tbl
                            .get::<LuaValue>("border")
                            .unwrap_or(LuaValue::Number(0.0))),
                        _ => Ok(tbl
                            .get::<LuaValue>(format!("_prop_{field}"))
                            .unwrap_or(LuaNil)),
                    };
                }
                let character_instances: LuaTable = lua.globals().get("__character_instances")?;
                if let Ok(tbl) = character_instances.get::<LuaTable>(tag.to_string()) {
                    let full_prop = format!("{tag}.{field}");
                    if let Ok(custom) = lua.globals().get::<LuaTable>("__custom_vars") {
                        if let Ok(value) = custom.get::<LuaValue>(full_prop.as_str()) {
                            if value != LuaNil {
                                return Ok(value);
                            }
                        }
                    }
                    return match field {
                        "x" => Ok(tbl.get::<LuaValue>("x").unwrap_or(LuaValue::Number(0.0))),
                        "y" => Ok(tbl.get::<LuaValue>("y").unwrap_or(LuaValue::Number(0.0))),
                        "alpha" => Ok(tbl
                            .get::<LuaValue>("alpha")
                            .unwrap_or(LuaValue::Number(1.0))),
                        "visible" => Ok(tbl
                            .get::<LuaValue>("visible")
                            .unwrap_or(LuaValue::Boolean(true))),
                        "scale.x" | "scaleX" => Ok(tbl
                            .get::<LuaValue>("scale_x")
                            .unwrap_or(LuaValue::Number(1.0))),
                        "scale.y" | "scaleY" => Ok(tbl
                            .get::<LuaValue>("scale_y")
                            .unwrap_or(LuaValue::Number(1.0))),
                        "animation.name" | "animation.curAnim.name" => Ok(sprite_animation_value(
                            lua,
                            &tbl,
                            "name",
                            LuaValue::String(lua.create_string("")?),
                        )?),
                        "animation.finished" | "animation.curAnim.finished" => {
                            Ok(sprite_animation_value(
                                lua,
                                &tbl,
                                "finished",
                                LuaValue::Boolean(false),
                            )?)
                        }
                        "holdTimer" | "hold_timer" => Ok(tbl
                            .get::<LuaValue>("holdTimer")
                            .unwrap_or(LuaValue::Number(0.0))),
                        _ => Ok(tbl
                            .get::<LuaValue>(format!("_prop_{field}"))
                            .unwrap_or(LuaNil)),
                    };
                }
            }
            // Character properties (animation name, position, alpha, etc.)
            {
                let g = lua.globals();
                match prop.as_str() {
                    "dad.animation.name"
                    | "dad.animation.curAnim.name"
                    | "dad.lastPlayedAnim"
                    | "opponent.animation.name"
                    | "opponent.animation.curAnim.name"
                    | "opponent.lastPlayedAnim" => {
                        return Ok(g
                            .get::<LuaValue>("__dad_anim_name")
                            .unwrap_or(LuaValue::String(lua.create_string("")?)))
                    }
                    "boyfriend.animation.name"
                    | "boyfriend.animation.curAnim.name"
                    | "boyfriend.lastPlayedAnim"
                    | "bf.animation.name"
                    | "bf.animation.curAnim.name"
                    | "bf.lastPlayedAnim" => {
                        return Ok(g
                            .get::<LuaValue>("__bf_anim_name")
                            .unwrap_or(LuaValue::String(lua.create_string("")?)))
                    }
                    "gf.animation.name"
                    | "gf.animation.curAnim.name"
                    | "gf.lastPlayedAnim"
                    | "girlfriend.animation.name"
                    | "girlfriend.animation.curAnim.name"
                    | "girlfriend.lastPlayedAnim" => {
                        return Ok(g
                            .get::<LuaValue>("__gf_anim_name")
                            .unwrap_or(LuaValue::String(lua.create_string("")?)))
                    }
                    "dad.animation.finished"
                    | "dad.animation.curAnim.finished"
                    | "opponent.animation.finished"
                    | "opponent.animation.curAnim.finished" => {
                        return Ok(g
                            .get::<LuaValue>("__dad_anim_finished")
                            .unwrap_or(LuaValue::Boolean(false)))
                    }
                    "boyfriend.animation.finished"
                    | "boyfriend.animation.curAnim.finished"
                    | "bf.animation.finished"
                    | "bf.animation.curAnim.finished" => {
                        return Ok(g
                            .get::<LuaValue>("__bf_anim_finished")
                            .unwrap_or(LuaValue::Boolean(false)))
                    }
                    "gf.animation.finished"
                    | "gf.animation.curAnim.finished"
                    | "girlfriend.animation.finished"
                    | "girlfriend.animation.curAnim.finished" => {
                        return Ok(g
                            .get::<LuaValue>("__gf_anim_finished")
                            .unwrap_or(LuaValue::Boolean(false)))
                    }
                    "dad.animation.curAnim.curFrame"
                    | "dad.animateAtlas.anim.curFrame"
                    | "opponent.animation.curAnim.curFrame" => {
                        return Ok(g
                            .get::<LuaValue>("__dad_anim_frame")
                            .unwrap_or(LuaValue::Integer(0)))
                    }
                    "boyfriend.animation.curAnim.curFrame"
                    | "boyfriend.animateAtlas.anim.curFrame"
                    | "bf.animation.curAnim.curFrame" => {
                        return Ok(g
                            .get::<LuaValue>("__bf_anim_frame")
                            .unwrap_or(LuaValue::Integer(0)))
                    }
                    "gf.animation.curAnim.curFrame"
                    | "gf.animateAtlas.anim.curFrame"
                    | "girlfriend.animation.curAnim.curFrame" => {
                        return Ok(g
                            .get::<LuaValue>("__gf_anim_frame")
                            .unwrap_or(LuaValue::Integer(0)))
                    }
                    // Character position reads — synced from game each frame
                    "dad.x" => {
                        return Ok(g
                            .get::<LuaValue>("__dad_x")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "dad.y" => {
                        return Ok(g
                            .get::<LuaValue>("__dad_y")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "dadGroup.x" => {
                        return Ok(g
                            .get::<LuaValue>("__dad_group_x")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "dadGroup.y" => {
                        return Ok(g
                            .get::<LuaValue>("__dad_group_y")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "boyfriend.x" | "bf.x" => {
                        return Ok(g.get::<LuaValue>("__bf_x").unwrap_or(LuaValue::Number(0.0)))
                    }
                    "boyfriend.y" | "bf.y" => {
                        return Ok(g.get::<LuaValue>("__bf_y").unwrap_or(LuaValue::Number(0.0)))
                    }
                    "boyfriendGroup.x" => {
                        return Ok(g
                            .get::<LuaValue>("__bf_group_x")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "boyfriendGroup.y" => {
                        return Ok(g
                            .get::<LuaValue>("__bf_group_y")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "gf.x" | "girlfriend.x" => {
                        return Ok(g.get::<LuaValue>("__gf_x").unwrap_or(LuaValue::Number(0.0)))
                    }
                    "gf.y" | "girlfriend.y" => {
                        return Ok(g.get::<LuaValue>("__gf_y").unwrap_or(LuaValue::Number(0.0)))
                    }
                    "gfGroup.x" => {
                        return Ok(g
                            .get::<LuaValue>("__gf_group_x")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "gfGroup.y" => {
                        return Ok(g
                            .get::<LuaValue>("__gf_group_y")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "camGame.scroll.x" => {
                        return Ok(g
                            .get::<LuaValue>("__cam_game_scroll_x")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "camGame.scroll.y" => {
                        return Ok(g
                            .get::<LuaValue>("__cam_game_scroll_y")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "opponentCameraOffset[0]" | "opponentCameraOffset.x" => {
                        return Ok(g
                            .get::<LuaValue>("__opponent_camera_offset_x")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "opponentCameraOffset[1]" | "opponentCameraOffset.y" => {
                        return Ok(g
                            .get::<LuaValue>("__opponent_camera_offset_y")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "boyfriendCameraOffset[0]" | "boyfriendCameraOffset.x" => {
                        return Ok(g
                            .get::<LuaValue>("__bf_camera_offset_x")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "boyfriendCameraOffset[1]" | "boyfriendCameraOffset.y" => {
                        return Ok(g
                            .get::<LuaValue>("__bf_camera_offset_y")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "dad.cameraPosition[0]"
                    | "dad.cameraPosition.x"
                    | "opponent.cameraPosition[0]"
                    | "opponent.cameraPosition.x" => {
                        return Ok(g
                            .get::<LuaValue>("__dad_camera_x")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "dad.cameraPosition[1]"
                    | "dad.cameraPosition.y"
                    | "opponent.cameraPosition[1]"
                    | "opponent.cameraPosition.y" => {
                        return Ok(g
                            .get::<LuaValue>("__dad_camera_y")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "boyfriend.cameraPosition[0]"
                    | "boyfriend.cameraPosition.x"
                    | "bf.cameraPosition[0]"
                    | "bf.cameraPosition.x" => {
                        return Ok(g
                            .get::<LuaValue>("__bf_camera_x")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "boyfriend.cameraPosition[1]"
                    | "boyfriend.cameraPosition.y"
                    | "bf.cameraPosition[1]"
                    | "bf.cameraPosition.y" => {
                        return Ok(g
                            .get::<LuaValue>("__bf_camera_y")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "gf.cameraPosition[0]"
                    | "gf.cameraPosition.x"
                    | "girlfriend.cameraPosition[0]"
                    | "girlfriend.cameraPosition.x" => {
                        return Ok(g
                            .get::<LuaValue>("__gf_camera_x")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    "gf.cameraPosition[1]"
                    | "gf.cameraPosition.y"
                    | "girlfriend.cameraPosition[1]"
                    | "girlfriend.cameraPosition.y" => {
                        return Ok(g
                            .get::<LuaValue>("__gf_camera_y")
                            .unwrap_or(LuaValue::Number(0.0)))
                    }
                    _ => {}
                }
            }

            if prop.starts_with("iconP1.animation.") || prop.starts_with("iconP2.animation.") {
                let g = lua.globals();
                if let Ok(custom) = g.get::<LuaTable>("__custom_vars") {
                    if let Ok(value) = custom.get::<LuaValue>(prop.as_str()) {
                        if value != LuaNil {
                            return Ok(value);
                        }
                    }
                }
                let health = g.get::<f64>("__health").unwrap_or(1.0);
                let frame = if prop.starts_with("iconP1.") {
                    if health < 0.4 {
                        1
                    } else {
                        0
                    }
                } else if health > 1.6 {
                    1
                } else {
                    0
                };
                return if prop.ends_with(".curFrame") {
                    Ok(LuaValue::Integer(frame))
                } else if prop.ends_with(".finished") || prop.ends_with(".animation.finished") {
                    Ok(LuaValue::Boolean(true))
                } else if prop.ends_with(".name") {
                    let name_key = if prop.starts_with("iconP1.") {
                        "boyfriendName"
                    } else {
                        "dadName"
                    };
                    Ok(g.get::<LuaValue>(name_key)
                        .unwrap_or(LuaValue::String(lua.create_string("")?)))
                } else {
                    Ok(LuaValue::Integer(frame))
                };
            }

            // Handle dotted paths for game object arrays (e.g. "unspawnNotes.length")
            if prop == "unspawnNotes.length" || prop == "notes.length" {
                let g = lua.globals();
                return Ok(LuaValue::Integer(
                    g.get::<i64>("__unspawnNotesLength").unwrap_or(0),
                ));
            }

            // Known game properties — read from globals (which Lua scripts may have set)
            let g = lua.globals();
            match prop.as_str() {
                "opponentCameraOffset" | "boyfriendCameraOffset" | "girlfriendCameraOffset" => {
                    let (x_key, y_key) = match prop.as_str() {
                        "opponentCameraOffset" => {
                            ("__opponent_camera_offset_x", "__opponent_camera_offset_y")
                        }
                        "boyfriendCameraOffset" => ("__bf_camera_offset_x", "__bf_camera_offset_y"),
                        _ => ("__gf_camera_x", "__gf_camera_y"),
                    };
                    let tbl = lua.create_table()?;
                    tbl.set(1, g.get::<f64>(x_key).unwrap_or(0.0))?;
                    tbl.set(2, g.get::<f64>(y_key).unwrap_or(0.0))?;
                    return Ok(LuaValue::Table(tbl));
                }
                "defaultCamZoom" => Ok(g
                    .get::<LuaValue>("defaultCamZoom")
                    .unwrap_or(LuaValue::Number(0.9))),
                "cameraSpeed" => Ok(g
                    .get::<LuaValue>("cameraSpeed")
                    .unwrap_or(LuaValue::Number(1.0))),
                "camZooming" | "camZoomingMult" | "camZoomingDecay" | "gameZoomingDecay" => Ok(g
                    .get::<LuaValue>(prop.as_str())
                    .unwrap_or(LuaValue::Number(1.0))),
                "healthGainMult" => Ok(LuaValue::Number(1.0)),
                "healthLossMult" => Ok(LuaValue::Number(1.0)),
                "health" => Ok(g
                    .get::<LuaValue>("__health")
                    .unwrap_or(LuaValue::Number(1.0))),
                "playbackRate" => Ok(LuaValue::Number(1.0)),
                "songLength" => Ok(LuaValue::Number(0.0)),
                "crochet" => Ok(g
                    .get::<LuaValue>("crochet")
                    .unwrap_or(LuaValue::Number(500.0))),
                "stepCrochet" => Ok(g
                    .get::<LuaValue>("stepCrochet")
                    .unwrap_or(LuaValue::Number(125.0))),
                _ => {
                    // Check custom variables table (set via setProperty for unknown properties)
                    let custom: LuaTable = g
                        .get::<LuaTable>("__custom_vars")
                        .unwrap_or(lua.create_table().unwrap());
                    let val = custom.get::<LuaValue>(prop.as_str()).unwrap_or(LuaNil);
                    if val != LuaNil {
                        return Ok(val);
                    }
                    let val = g.get::<LuaValue>(prop.as_str()).unwrap_or(LuaNil);
                    if val != LuaNil {
                        return Ok(val);
                    }
                    // Check game object property paths stored as globals
                    let val = g
                        .get::<LuaValue>(format!("__gprop_{prop}"))
                        .unwrap_or(LuaNil);
                    if val != LuaNil {
                        return Ok(val);
                    }
                    // Default: return 0 instead of nil to prevent arithmetic errors
                    // (Psych Engine returns 0/false for missing properties via Reflect)
                    log::debug!("getProperty: unknown property '{}', returning 0", prop);
                    Ok(LuaValue::Number(0.0))
                }
            }
        })?,
    )?;

    // getPropertyFromGroup(group, index, field)
    globals.set(
        "getPropertyFromGroup",
        lua.create_function(
            |lua, (group, idx, field): (String, i32, String)| -> LuaResult<LuaValue> {
                // Handle strum groups
                let strum_idx = match group.as_str() {
                    "opponentStrums" => Some(idx as usize),     // 0-3
                    "playerStrums" => Some((idx + 4) as usize), // 4-7
                    "strumLineNotes" => Some(idx as usize),     // 0-7 direct
                    _ => None,
                };
                if let Some(si) = strum_idx {
                    if si < 8 {
                        let props: LuaTable = lua.globals().get("__strum_props")?;
                        if let Ok(tbl) = props.get::<LuaTable>(si as i64 + 1) {
                            return match field.as_str() {
                                "x" => {
                                    Ok(tbl.get::<LuaValue>("x").unwrap_or(LuaValue::Number(0.0)))
                                }
                                "y" => {
                                    Ok(tbl.get::<LuaValue>("y").unwrap_or(LuaValue::Number(0.0)))
                                }
                                "alpha" => Ok(tbl
                                    .get::<LuaValue>("alpha")
                                    .unwrap_or(LuaValue::Number(1.0))),
                                "angle" => Ok(tbl
                                    .get::<LuaValue>("angle")
                                    .unwrap_or(LuaValue::Number(0.0))),
                                "scale.x" => Ok(tbl
                                    .get::<LuaValue>("scale_x")
                                    .unwrap_or(LuaValue::Number(0.7))),
                                "scale.y" => Ok(tbl
                                    .get::<LuaValue>("scale_y")
                                    .unwrap_or(LuaValue::Number(0.7))),
                                "downScroll" => Ok(tbl
                                    .get::<LuaValue>("downScroll")
                                    .unwrap_or(LuaValue::Boolean(false))),
                                _ => Ok(LuaNil),
                            };
                        }
                    }
                }
                // Handle note groups (unspawnNotes / notes)
                if group == "unspawnNotes" || group == "notes" {
                    let i = idx as usize;
                    // First check for overrides in __note_overrides
                    let overrides: LuaTable = lua.globals().get("__note_overrides")?;
                    if let Ok(note_tbl) = overrides.get::<LuaTable>(i as i64 + 1) {
                        if let Ok(val) = note_tbl.get::<LuaValue>(field.as_str()) {
                            if val != LuaNil {
                                return Ok(val);
                            }
                        }
                    }
                    // Fall back to __note_read_data for basic fields
                    let read_data: LuaTable = lua.globals().get("__note_read_data")?;
                    if let Ok(note_tbl) = read_data.get::<LuaTable>(i as i64 + 1) {
                        return match field.as_str() {
                            "strumTime" => Ok(note_tbl
                                .get::<LuaValue>("strumTime")
                                .unwrap_or(LuaValue::Number(0.0))),
                            "noteData" | "lane" => Ok(note_tbl
                                .get::<LuaValue>("lane")
                                .unwrap_or(LuaValue::Integer(0))),
                            "mustPress" => Ok(note_tbl
                                .get::<LuaValue>("mustPress")
                                .unwrap_or(LuaValue::Boolean(false))),
                            "isSustainNote" => Ok(note_tbl
                                .get::<LuaValue>("isSustainNote")
                                .unwrap_or(LuaValue::Boolean(false))),
                            "sustainLength" => Ok(note_tbl
                                .get::<LuaValue>("sustainLength")
                                .unwrap_or(LuaValue::Number(0.0))),
                            // animation.curAnim.name — derive from note data
                            "animation.curAnim.name" => {
                                let lane: i64 = note_tbl.get("lane").unwrap_or(0);
                                let is_sus: bool = note_tbl.get("isSustainNote").unwrap_or(false);
                                let color = match lane % 4 {
                                    0 => "purple",
                                    1 => "blue",
                                    2 => "green",
                                    3 => "red",
                                    _ => "purple",
                                };
                                let name = if is_sus {
                                    // Check if note has an explicit anim override, otherwise assume hold body
                                    if let Ok(ovr) = note_tbl.get::<String>("animName") {
                                        ovr
                                    } else {
                                        format!("{color}hold")
                                    }
                                } else {
                                    format!("{color}Scroll")
                                };
                                Ok(LuaValue::String(lua.create_string(&name)?))
                            }
                            // Visual defaults for fields not yet overridden
                            "visible" => Ok(LuaValue::Boolean(true)),
                            "alpha" => Ok(LuaValue::Number(1.0)),
                            "scale.x" | "scale.y" => Ok(LuaValue::Number(0.7)),
                            "angle" => Ok(LuaValue::Number(0.0)),
                            "flipY" => Ok(LuaValue::Boolean(false)),
                            "colorTransform.redOffset"
                            | "colorTransform.greenOffset"
                            | "colorTransform.blueOffset" => Ok(LuaValue::Number(0.0)),
                            _ => Ok(LuaNil),
                        };
                    }
                }
                Ok(LuaNil)
            },
        )?,
    )?;

    // setPropertyFromGroup(group, index, field, value)
    globals.set(
        "setPropertyFromGroup",
        lua.create_function(
            |lua, (group, idx, field, value): (String, i32, String, LuaValue)| {
                let strum_idx = match group.as_str() {
                    "opponentStrums" => Some(idx as usize),
                    "playerStrums" => Some((idx + 4) as usize),
                    "strumLineNotes" => Some(idx as usize),
                    _ => None,
                };
                if let Some(si) = strum_idx {
                    if si < 8 {
                        let props: LuaTable = lua.globals().get("__strum_props")?;
                        if let Ok(tbl) = props.get::<LuaTable>(si as i64 + 1) {
                            match field.as_str() {
                                "x" => tbl.set("x", &value)?,
                                "y" => tbl.set("y", &value)?,
                                "alpha" => tbl.set("alpha", &value)?,
                                "angle" => tbl.set("angle", &value)?,
                                "scale.x" => tbl.set("scale_x", &value)?,
                                "scale.y" => tbl.set("scale_y", &value)?,
                                "downScroll" => tbl.set("downScroll", &value)?,
                                _ => {}
                            }
                            tbl.set("custom", true)?;
                        }
                    }
                } else if group == "unspawnNotes" || group == "notes" {
                    // Handle note groups
                    let i = idx as usize;
                    let overrides: LuaTable = lua.globals().get("__note_overrides")?;
                    let note_tbl: LuaTable = match overrides.get::<LuaTable>(i as i64 + 1) {
                        Ok(t) => t,
                        Err(_) => {
                            let t = lua.create_table()?;
                            overrides.set(i as i64 + 1, t.clone())?;
                            t
                        }
                    };
                    note_tbl.set(field.as_str(), value)?;
                    // Mark this note index as dirty
                    let dirty: LuaTable = lua.globals().get("__dirty_notes")?;
                    dirty.set(i as i64 + 1, true)?;
                }
                Ok(())
            },
        )?,
    )?;

    // getPropertyFromClass — return values from known classes
    globals.set(
        "getPropertyFromClass",
        lua.create_function(
            |lua, (class, var): (String, String)| -> LuaResult<LuaValue> {
                let g = lua.globals();
                if class.contains("PlayState") {
                    match var.as_str() {
                        "bfVersion" => {
                            // Return the boyfriend character name (e.g., "bf-wind")
                            return Ok(g.get::<LuaValue>("boyfriendName").unwrap_or(LuaNil));
                        }
                        "pressedCheckpoint" => {
                            // Check global first (set by engine), then custom_vars (set by Lua)
                            if let Ok(val) = g.get::<LuaValue>("__pressedCheckpoint") {
                                if !matches!(val, LuaNil) {
                                    return Ok(val);
                                }
                            }
                            let custom: LuaTable = g.get("__custom_vars")?;
                            return Ok(custom
                                .get::<LuaValue>("__pressedCheckpoint")
                                .unwrap_or(LuaNil));
                        }
                        "endedCatastro" => {
                            let custom: LuaTable = g.get("__custom_vars")?;
                            return Ok(custom.get::<LuaValue>("endedCatastro").unwrap_or(LuaNil));
                        }
                        "SONG.needsVoices" => {
                            return Ok(LuaValue::Boolean(
                                g.get::<bool>("__songNeedsVoices").unwrap_or(false),
                            ));
                        }
                        _ => {}
                    }
                }
                if class.contains("FlxG") {
                    match var.as_str() {
                        "sound.music.time" | "sound.music.position" => {
                            return Ok(LuaValue::Number(
                                g.get::<f64>("__songPosition").unwrap_or(0.0),
                            ));
                        }
                        _ => {}
                    }
                }
                if class.contains("Conductor") {
                    match var.as_str() {
                        "offset" | "songOffset" => {
                            return Ok(LuaValue::Number(
                                g.get::<f64>("__conductorOffset").unwrap_or(0.0),
                            ));
                        }
                        "songPosition" => {
                            return Ok(LuaValue::Number(
                                g.get::<f64>("__songPosition").unwrap_or(0.0),
                            ));
                        }
                        _ => {}
                    }
                }
                Ok(LuaNil)
            },
        )?,
    )?;

    // setPropertyFromClass — store values for PlayState properties
    globals.set(
        "setPropertyFromClass",
        lua.create_function(|lua, (class, var, val): (String, String, LuaValue)| {
            if class.contains("PlayState") {
                let custom: LuaTable = lua.globals().get("__custom_vars")?;
                match var.as_str() {
                    "bfVersion" => {
                        if let LuaValue::String(s) = &val {
                            lua.globals()
                                .set("boyfriendName", s.to_string_lossy().to_string())?;
                        }
                    }
                    "endedCatastro" | "pressedCheckpoint" => {
                        let key = format!("__{}", var);
                        custom.set(key.as_str(), val)?;
                    }
                    _ => {
                        let key = format!("__class_{}_{}", class, var);
                        custom.set(key, val)?;
                    }
                }
            }
            Ok(())
        })?,
    )?;

    // set(property, value) — alias for setProperty (used by some mods)
    globals.set(
        "set",
        lua.create_function(|lua, (prop, value): (String, LuaValue)| {
            let set_prop: LuaFunction = lua.globals().get("setProperty")?;
            set_prop.call::<()>((prop, value))?;
            Ok(())
        })?,
    )?;

    // setVar / getVar — custom variables shared across scripts
    globals.set(
        "setVar",
        lua.create_function(|lua, (name, value): (String, LuaValue)| {
            let vars: LuaTable = lua.globals().get("__custom_vars")?;
            vars.set(name, value)?;
            Ok(())
        })?,
    )?;

    globals.set(
        "getVar",
        lua.create_function(|lua, name: String| -> LuaResult<LuaValue> {
            let vars: LuaTable = lua.globals().get("__custom_vars")?;
            Ok(vars.get::<LuaValue>(name).unwrap_or(LuaNil))
        })?,
    )?;
    globals.set(
        "removeVar",
        lua.create_function(|lua, name: String| -> LuaResult<bool> {
            let vars: LuaTable = lua.globals().get("__custom_vars")?;
            let existed = vars.get::<LuaValue>(name.as_str()).unwrap_or(LuaNil) != LuaNil;
            vars.set(name.as_str(), LuaNil)?;
            lua.globals().set(name, LuaNil).ok();
            Ok(existed)
        })?,
    )?;

    globals.set(
        "setGlobalFromScript",
        lua.create_function(
            |lua, (_script_name, name, value): (String, String, LuaValue)| {
                let vars: LuaTable = lua.globals().get("__custom_vars")?;
                vars.set(name, value)?;
                Ok(())
            },
        )?,
    )?;

    globals.set(
        "getGlobalFromScript",
        lua.create_function(
            |lua, (_script_name, name): (String, String)| -> LuaResult<LuaValue> {
                let vars: LuaTable = lua.globals().get("__custom_vars")?;
                Ok(vars.get::<LuaValue>(name).unwrap_or(LuaNil))
            },
        )?,
    )?;

    Ok(())
}

fn register_utility_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // close()
    globals.set(
        "close",
        lua.create_function(|lua, ()| {
            lua.globals().set("__script_closed", true)?;
            Ok(())
        })?,
    )?;

    // debugPrint(...)
    globals.set(
        "debugPrint",
        lua.create_function(|_lua, args: LuaMultiValue| {
            let parts: Vec<String> = args.iter().map(|v| lua_val_to_string(v)).collect();
            log::info!("[Lua] {}", parts.join(" "));
            Ok(())
        })?,
    )?;

    // luaTrace — alias
    globals.set(
        "luaTrace",
        lua.create_function(|_lua, args: LuaMultiValue| {
            let parts: Vec<String> = args.iter().map(|v| lua_val_to_string(v)).collect();
            log::info!("[Lua] {}", parts.join(" "));
            Ok(())
        })?,
    )?;

    // runHaxeCode — pattern-match common Haxe patterns and execute natively
    globals.set(
        "runHaxeCode",
        lua.create_function(|lua, code: String| -> LuaResult<LuaValue> {
            let code = code.trim();

            // Preserve simple captured sprite references used by Haxe helper functions.
            if code.contains("function fixClipRect") {
                if let Some(tag) =
                    extract_quoted_argument(code, "var divider = game.modchartSprites.get(")
                {
                    let custom: LuaTable = lua.globals().get("__custom_vars")?;
                    custom.set("__clip_rect_divider_tag", tag)?;
                }
            }

            // Pattern: game.moveCameraSection(N)
            if let Some(inner) = code.strip_prefix("game.moveCameraSection(") {
                if let Some(num_str) = inner.split(')').next() {
                    if let Ok(section) = num_str.trim().parse::<i32>() {
                        let g = lua.globals();
                        let pending: LuaTable = g.get("__pending_cam_sections")?;
                        let len = pending.len()? as i64;
                        pending.set(len + 1, section)?;
                    }
                }
                return Ok(LuaNil);
            }

            // Pattern: game.getLuaObject('tag').camera = getVar('cameraName')
            if code.contains("game.getLuaObject") && code.contains(".camera") {
                if let (Some(tag), Some(camera)) = (
                    extract_quoted_argument(code, "game.getLuaObject("),
                    extract_quoted_argument(code, "getVar("),
                ) {
                    let camera = normalize_lua_camera_name(camera);
                    let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                    if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
                        tbl.set("camera", camera.as_str())?;
                    }
                    let text_data: LuaTable = lua.globals().get("__text_data")?;
                    if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                        tbl.set("camera", camera.as_str())?;
                    }
                }
                return Ok(LuaNil);
            }

            // Pattern: FlxTween.num(getVar('name'), endVal, duration, {ease: FlxEase.X}, function(num) {setVar('name', num);})
            if code.contains("FlxTween.num") {
                if let Some((var_name, end_val, duration, ease)) = parse_flx_tween_num(code) {
                    let g = lua.globals();
                    // Read current value of the variable as start
                    let start_val = if let Ok(custom) = g.get::<LuaTable>("__custom_vars") {
                        custom.get::<f64>(var_name.as_str()).unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    // Push as a variable tween
                    let pending: LuaTable = g.get("__pending_tweens")?;
                    let tbl = lua.create_table()?;
                    tbl.set("tag", format!("__hx_var_{}", var_name))?;
                    tbl.set("target", format!("__var_{}", var_name))?;
                    tbl.set("property", "x")?; // arbitrary, we just need the interpolated value
                    tbl.set("value", end_val)?;
                    tbl.set("duration", duration)?;
                    tbl.set("ease", ease)?;
                    tbl.set("start", start_val)?;
                    let len = pending.len()? as i64;
                    pending.set(len + 1, tbl)?;
                    log::debug!(
                        "runHaxeCode: FlxTween.num({}, {} -> {}, {}s)",
                        var_name,
                        start_val,
                        end_val,
                        duration
                    );
                }
                return Ok(LuaNil);
            }

            // Pattern: camCharacters.visible = true/false
            if code.contains("camCharacters.visible") {
                // Parse the boolean value — affects character layer visibility
                // For now, log it; full implementation needs a property write
                if code.contains("true") {
                    log::debug!("runHaxeCode: camCharacters.visible = true");
                } else if code.contains("false") {
                    log::debug!("runHaxeCode: camCharacters.visible = false");
                }
                return Ok(LuaNil);
            }

            // Pattern: switch(game.bfVersion) with game.boyfriend/dad/gf.x/y assignments
            if code.contains("game.boyfriend.")
                || code.contains("game.dad.")
                || code.contains("game.gf.")
            {
                let g = lua.globals();
                let bf_name: String = g.get::<String>("boyfriendName").unwrap_or_default();
                let positions = parse_haxe_char_positions(code, &bf_name);
                if !positions.is_empty() {
                    let pending: LuaTable = g.get("__pending_char_positions")?;
                    for (char_name, field, delta) in &positions {
                        let tbl = lua.create_table()?;
                        tbl.set("character", *char_name)?;
                        tbl.set("field", *field)?;
                        tbl.set("value", *delta)?;
                        let len = pending.len()? as i64;
                        pending.set(len + 1, tbl)?;
                    }
                    log::info!(
                        "runHaxeCode: parsed {} char position adjustments for bf='{}'",
                        positions.len(),
                        bf_name
                    );
                }
                return Ok(LuaNil);
            }

            // Pattern: clothScythes manipulation (wrath_phase4 stage) — no-op
            if code.contains("clothScythes") {
                return Ok(LuaNil);
            }

            // Pattern: spotlight.y += N — adjust 'spotlight' sprite position
            if code.contains("spotlight.y") {
                if let Some(val_str) = code.split("spotlight.y").nth(1) {
                    let val_str = val_str
                        .trim()
                        .trim_start_matches('+')
                        .trim_start_matches('=')
                        .trim();
                    if let Ok(delta) = val_str
                        .split(';')
                        .next()
                        .unwrap_or(val_str)
                        .trim()
                        .parse::<f32>()
                    {
                        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                        if let Ok(tbl) = sprite_data.get::<LuaTable>("spotlight") {
                            let current_y: f32 = tbl.get("y").unwrap_or(0.0);
                            tbl.set("y", current_y + delta)?;
                        }
                    }
                }
                return Ok(LuaNil);
            }

            // Pattern: game.callOnLuas('setPosition', [N]) — call setPosition from runHaxeCode
            if code.contains("callOnLuas") {
                if let Some((function, args)) = parse_haxe_call_on_luas(lua, code)? {
                    queue_script_call(lua, None, &function, args)?;
                }
                return Ok(LuaNil);
            }

            // Ignore: function definitions, camCharacters.shake, camVideo.zoom, etc.
            if code.contains("function ") || code.contains(".shake(") || code.contains("camVideo.")
            {
                return Ok(LuaNil);
            }

            log::debug!(
                "runHaxeCode: unhandled pattern: {}",
                &code[..code.len().min(80)]
            );
            Ok(LuaNil)
        })?,
    )?;

    // runHaxeFunction(name, args) — call a previously defined Haxe function
    globals.set(
        "runHaxeFunction",
        lua.create_function(
            |lua, (name, args): (String, Option<LuaTable>)| -> LuaResult<LuaValue> {
                // Handle known function patterns
                match name.as_str() {
                    "charactersCamera" => {
                        // charactersCamera(visible) — toggle character layer visibility
                        let visible = args
                            .as_ref()
                            .and_then(|t| t.get::<LuaValue>(1).ok())
                            .map(|v| match v {
                                LuaValue::Boolean(b) => b,
                                LuaValue::Number(n) => n != 0.0,
                                _ => true,
                            })
                            .unwrap_or(true);
                        let pending: LuaTable = lua.globals().get("__pending_props")?;
                        let tbl = lua.create_table()?;
                        tbl.set("prop", "__camCharactersVisible")?;
                        tbl.set("value", visible)?;
                        let len = pending.len()? as i64;
                        pending.set(len + 1, tbl)?;
                    }
                    // setCameraOnThings(bool) — toggle 80sNightflaid VCR camera mode
                    // We can't replicate the shader, but we can log it for debugging
                    "setCameraOnThings" => {
                        let enabled = args
                            .as_ref()
                            .and_then(|t| t.get::<bool>(1).ok())
                            .unwrap_or(false);
                        log::info!("runHaxeFunction: setCameraOnThings({})", enabled);
                        // Store as a custom var so scripts can query it
                        let custom: LuaTable = lua.globals().get("__custom_vars")?;
                        custom.set("__cameraOnThings", enabled)?;
                    }
                    // preVideoTransition — transition out of 80s mode visuals
                    "preVideoTransition" => {
                        log::info!("runHaxeFunction: preVideoTransition");
                    }
                    // makeSpotlightAtlas(x, y) — create spotlight animated sprite for wrath_phase4
                    "makeSpotlightAtlas" => {
                        let x = args
                            .as_ref()
                            .and_then(|t| t.get::<f64>(1).ok())
                            .unwrap_or(0.0) as f32;
                        let y = args
                            .as_ref()
                            .and_then(|t| t.get::<f64>(2).ok())
                            .unwrap_or(0.0) as f32;
                        // Create as an animated lua sprite
                        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
                        let tbl = lua.create_table()?;
                        tbl.set("tag", "spotlight")?;
                        tbl.set("kind", "animated")?;
                        tbl.set("image", "Phase4/spotlight")?;
                        tbl.set("x", x - 150.0)?;
                        tbl.set("y", y)?;
                        tbl.set("scale_x", 1.2)?;
                        tbl.set("scale_y", 1.2)?;
                        tbl.set("scroll_x", 0.45)?;
                        tbl.set("scroll_y", 0.3)?;
                        tbl.set("alpha", 0.6)?;
                        tbl.set("visible", true)?;
                        tbl.set("flip_x", false)?;
                        tbl.set("antialiasing", true)?;
                        // Add animation
                        let anims = lua.create_table()?;
                        let anim_tbl = lua.create_table()?;
                        anim_tbl.set("prefix", "SpotLight Animation")?;
                        anim_tbl.set("fps", 24.0)?;
                        anim_tbl.set("looping", true)?;
                        anims.set("anim", anim_tbl)?;
                        tbl.set("__anims", anims)?;
                        sprite_data.set("spotlight", tbl)?;
                        // Queue add as sprite
                        let pending_adds: LuaTable = lua.globals().get("__pending_adds")?;
                        let add_tbl = lua.create_table()?;
                        add_tbl.set("tag", "spotlight")?;
                        add_tbl.set("in_front", false)?;
                        let len = pending_adds.len()? as i64;
                        pending_adds.set(len + 1, add_tbl)?;
                        log::info!("runHaxeFunction: makeSpotlightAtlas({}, {})", x, y);
                    }
                    // adjustPositions — compute BF position based on current step (wrath_phase4)
                    "adjustPositions" => {
                        let g = lua.globals();
                        let cur_step: f64 = g.get::<f64>("curStep").unwrap_or(0.0);
                        let ended_catastro: bool = g.get::<bool>("endedCatastro").unwrap_or(false);
                        let actual_step = if ended_catastro {
                            3328.0
                        } else {
                            cur_step.max(0.0)
                        };
                        // Replicate the FlxMath.remapToRange logic from the Haxe code
                        let bf_position = if actual_step <= 1792.0 {
                            remap_to_range(actual_step, 0.0, 1472.0, 8219.1605, 2000.355)
                        } else if actual_step <= 2304.0 {
                            remap_to_range(actual_step, 0.0, 1472.0, 8219.1605, 1847.355)
                        } else if actual_step <= 2560.0 {
                            remap_to_range(
                                actual_step,
                                2304.0,
                                2560.0,
                                -1758.35969606794,
                                -5703.23798043777,
                            )
                        } else {
                            remap_to_range(
                                actual_step,
                                2560.0,
                                3328.0,
                                -5703.23798043777,
                                -68148.9101742824,
                            )
                        };
                        // Call the Lua setPosition function directly if it exists
                        if let Ok(set_pos_fn) = g.get::<LuaFunction>("setPosition") {
                            if let Err(e) = set_pos_fn.call::<()>(bf_position) {
                                log::debug!(
                                    "adjustPositions: setPosition({}) failed: {}",
                                    bf_position,
                                    e
                                );
                            }
                        } else {
                            log::debug!("adjustPositions: setPosition function not found");
                        }
                    }
                    // dadCallback — resume dad idle when current animation completes
                    "dadCallback" => {
                        // Store flag so engine knows to re-enable dad dancing after anim finishes
                        let custom: LuaTable = lua.globals().get("__custom_vars")?;
                        custom.set("dadAnimCallback", true)?;
                    }
                    "fixClipRect" => {
                        apply_haxe_clip_rect(lua, &args, true)?;
                    }
                    "fixClipRect2" => {
                        apply_haxe_clip_rect(lua, &args, false)?;
                    }
                    _ => {
                        log::debug!("runHaxeFunction: unhandled function '{}'", name);
                    }
                }
                Ok(LuaNil)
            },
        )?,
    )?;

    // getSongPosition() — reads from Lua global kept in sync by the game
    globals.set(
        "getSongPosition",
        lua.create_function(|lua, ()| -> LuaResult<f64> {
            let g = lua.globals();
            Ok(g.get::<f64>("__songPosition").unwrap_or(0.0))
        })?,
    )?;

    // cameraSetTarget(target) — queues camera target switch
    globals.set(
        "cameraSetTarget",
        lua.create_function(|lua, target: String| {
            let g = lua.globals();
            let pending: LuaTable = g
                .get::<LuaTable>("__pending_cam_targets")
                .unwrap_or_else(|_| lua.create_table().unwrap());
            let len = pending.len().unwrap_or(0);
            pending.set(len + 1, target.as_str())?;
            g.set("__last_camera_target", target.as_str())?;
            g.set("__pending_cam_targets", pending)?;
            Ok(())
        })?,
    )?;

    // triggerEvent(name, v1, v2) — queues event for game processing
    globals.set(
        "triggerEvent",
        lua.create_function(
            |lua, (name, v1, v2): (String, Option<mlua::Value>, Option<mlua::Value>)| {
                let g = lua.globals();
                let pending: LuaTable = g
                    .get::<LuaTable>("__pending_events")
                    .unwrap_or_else(|_| lua.create_table().unwrap());
                let entry = lua.create_table()?;
                entry.set("name", name)?;
                // Convert values to strings (Psych Engine accepts both numbers and strings)
                let v1_str = match v1 {
                    Some(mlua::Value::String(s)) => s.to_string_lossy().to_string(),
                    Some(mlua::Value::Number(n)) => n.to_string(),
                    Some(mlua::Value::Integer(n)) => n.to_string(),
                    _ => String::new(),
                };
                let v2_str = match v2 {
                    Some(mlua::Value::String(s)) => s.to_string_lossy().to_string(),
                    Some(mlua::Value::Number(n)) => n.to_string(),
                    Some(mlua::Value::Integer(n)) => n.to_string(),
                    _ => String::new(),
                };
                entry.set("v1", v1_str)?;
                entry.set("v2", v2_str)?;
                let len = pending.len().unwrap_or(0);
                pending.set(len + 1, entry)?;
                g.set("__pending_events", pending)?;
                Ok(())
            },
        )?,
    )?;

    // moveCameraSection(section) — move camera based on chart section's mustHitSection
    globals.set(
        "moveCameraSection",
        lua.create_function(|lua, section: Option<i32>| {
            let section = section.unwrap_or(0);
            let g = lua.globals();
            let pending: LuaTable = g.get("__pending_cam_sections")?;
            let len = pending.len()? as i64;
            pending.set(len + 1, section)?;
            Ok(())
        })?,
    )?;

    // getColorFromHex(hex) -> integer
    globals.set(
        "getColorFromHex",
        lua.create_function(|_lua, hex: String| -> LuaResult<i64> {
            let hex = hex
                .trim_start_matches('#')
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            let val = u32::from_str_radix(hex, 16).unwrap_or(0xFFFFFF);
            let val = if hex.len() <= 6 {
                0xFF000000 | val
            } else {
                val
            };
            Ok(val as i64)
        })?,
    )?;

    globals.set(
        "FlxColor",
        lua.create_function(|_lua, hex: String| -> LuaResult<i64> {
            let hex = hex
                .trim_start_matches('#')
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            let val = u32::from_str_radix(hex, 16).unwrap_or(0xFFFFFF);
            let val = if hex.len() <= 6 {
                0xFF000000 | val
            } else {
                val
            };
            Ok(val as i64)
        })?,
    )?;

    // String utils
    globals.set(
        "stringStartsWith",
        lua.create_function(
            |_lua, (s, prefix): (LuaValue, LuaValue)| -> LuaResult<bool> {
                let text = match s {
                    LuaValue::String(st) => st.to_str()?.to_string(),
                    LuaValue::Number(n) => n.to_string(),
                    LuaValue::Integer(i) => i.to_string(),
                    _ => String::new(),
                };
                let pref = match prefix {
                    LuaValue::String(st) => st.to_str()?.to_string(),
                    LuaValue::Number(n) => n.to_string(),
                    LuaValue::Integer(i) => i.to_string(),
                    _ => String::new(),
                };
                Ok(text.starts_with(&pref))
            },
        )?,
    )?;

    globals.set(
        "stringEndsWith",
        lua.create_function(
            |_lua, (s, suffix): (LuaValue, LuaValue)| -> LuaResult<bool> {
                let text = match s {
                    LuaValue::String(st) => st.to_str()?.to_string(),
                    LuaValue::Number(n) => n.to_string(),
                    LuaValue::Integer(i) => i.to_string(),
                    _ => String::new(),
                };
                let suff = match suffix {
                    LuaValue::String(st) => st.to_str()?.to_string(),
                    LuaValue::Number(n) => n.to_string(),
                    LuaValue::Integer(i) => i.to_string(),
                    _ => String::new(),
                };
                Ok(text.ends_with(&suff))
            },
        )?,
    )?;

    globals.set(
        "stringSplit",
        lua.create_function(|lua, (s, sep): (LuaValue, LuaValue)| {
            let text = match s {
                LuaValue::String(st) => st.to_str()?.to_string(),
                LuaValue::Number(n) => n.to_string(),
                LuaValue::Integer(i) => i.to_string(),
                _ => String::new(),
            };
            let separator = match sep {
                LuaValue::String(st) => st.to_str()?.to_string(),
                _ => ",".to_string(),
            };
            let parts: Vec<String> = text.split(&separator).map(|p| p.to_string()).collect();
            let table = lua.create_table()?;
            for (i, part) in parts.iter().enumerate() {
                table.set(i + 1, part.as_str())?;
            }
            Ok(table)
        })?,
    )?;

    globals.set(
        "stringTrim",
        lua.create_function(|_lua, s: String| Ok(s.trim().to_string()))?,
    )?;

    // Random
    globals.set(
        "getRandomInt",
        lua.create_function(
            |_lua, (min, max, _exclude): (i32, i32, Option<String>)| -> LuaResult<i32> {
                let range = (max - min + 1).max(1);
                let val = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos()) as i32;
                Ok(min + (val.unsigned_abs() as i32 % range))
            },
        )?,
    )?;

    globals.set(
        "getRandomFloat",
        lua.create_function(
            |_lua, (min, max, _exclude): (f64, f64, Option<String>)| -> LuaResult<f64> {
                let t = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as f64)
                    / 1_000_000_000.0;
                Ok(min + t * (max - min))
            },
        )?,
    )?;

    globals.set(
        "getRandomBool",
        lua.create_function(|_lua, chance: Option<f64>| -> LuaResult<bool> {
            let chance = chance.unwrap_or(50.0);
            let t = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as f64)
                / 1_000_000_000.0
                * 100.0;
            Ok(t < chance)
        })?,
    )?;

    register_keyboard_query(lua, "keyJustPressed", "__input_just_pressed")?;
    register_keyboard_query(lua, "keyPressed", "__input_pressed")?;
    register_keyboard_query(lua, "keyReleased", "__input_just_released")?;

    // getMidpointX/Y — returns center of sprite or game character
    globals.set(
        "getMidpointX",
        lua.create_function(|lua, tag: String| -> LuaResult<f64> {
            let g = lua.globals();
            // Check game characters first (synced from PlayScreen)
            let char_key = format!("__midX_{}", tag);
            if let Ok(v) = g.get::<f64>(char_key) {
                return Ok(v);
            }
            // Fall back to Lua sprite
            let sprite_data: LuaTable = g.get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
                let x: f64 = tbl.get("x").unwrap_or(0.0);
                let tw: f64 = tbl.get("tex_w").unwrap_or(0.0);
                let sx: f64 = tbl.get("scale_x").unwrap_or(1.0);
                return Ok(x + tw * sx.abs() / 2.0);
            }
            Ok(0.0)
        })?,
    )?;
    globals.set(
        "getMidpointY",
        lua.create_function(|lua, tag: String| -> LuaResult<f64> {
            let g = lua.globals();
            let char_key = format!("__midY_{}", tag);
            if let Ok(v) = g.get::<f64>(char_key) {
                return Ok(v);
            }
            let sprite_data: LuaTable = g.get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
                let y: f64 = tbl.get("y").unwrap_or(0.0);
                let th: f64 = tbl.get("tex_h").unwrap_or(0.0);
                let sy: f64 = tbl.get("scale_y").unwrap_or(1.0);
                return Ok(y + th * sy.abs() / 2.0);
            }
            Ok(0.0)
        })?,
    )?;
    globals.set(
        "getGraphicMidpointX",
        lua.create_function(|lua, tag: String| -> LuaResult<f64> {
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
                let x: f64 = tbl.get("x").unwrap_or(0.0);
                let tw: f64 = tbl.get("tex_w").unwrap_or(0.0);
                let sx: f64 = tbl.get("scale_x").unwrap_or(1.0);
                return Ok(x + tw * sx.abs() / 2.0);
            }
            Ok(0.0)
        })?,
    )?;
    globals.set(
        "getGraphicMidpointY",
        lua.create_function(|lua, tag: String| -> LuaResult<f64> {
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
                let y: f64 = tbl.get("y").unwrap_or(0.0);
                let th: f64 = tbl.get("tex_h").unwrap_or(0.0);
                let sy: f64 = tbl.get("scale_y").unwrap_or(1.0);
                return Ok(y + th * sy.abs() / 2.0);
            }
            Ok(0.0)
        })?,
    )?;

    // Character access
    globals.set(
        "characterDance",
        lua.create_function(|lua, char_type: String| {
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let tbl = lua.create_table()?;
            tbl.set("prop", format!("__charDance.{}", char_type))?;
            tbl.set("value", true)?;
            pending.set(pending.len()? + 1, tbl)?;
            Ok(())
        })?,
    )?;
    globals.set(
        "getCharacterX",
        lua.create_function(|lua, typ: String| -> LuaResult<f64> {
            let key = match typ.to_lowercase().as_str() {
                "dad" | "opponent" | "1" => "__dad_x",
                "boyfriend" | "bf" | "0" => "__bf_x",
                "gf" | "girlfriend" | "2" => "__gf_x",
                _ => "__bf_x",
            };
            Ok(lua.globals().get::<f64>(key).unwrap_or(0.0))
        })?,
    )?;
    globals.set(
        "getCharacterY",
        lua.create_function(|lua, typ: String| -> LuaResult<f64> {
            let key = match typ.to_lowercase().as_str() {
                "dad" | "opponent" | "1" => "__dad_y",
                "boyfriend" | "bf" | "0" => "__bf_y",
                "gf" | "girlfriend" | "2" => "__gf_y",
                _ => "__bf_y",
            };
            Ok(lua.globals().get::<f64>(key).unwrap_or(0.0))
        })?,
    )?;
    globals.set(
        "setCharacterX",
        lua.create_function(|lua, (typ, val): (String, f64)| {
            let target = match typ.to_lowercase().as_str() {
                "dad" | "opponent" | "1" => "dad.x",
                "boyfriend" | "bf" | "0" => "boyfriend.x",
                "gf" | "girlfriend" | "2" => "gf.x",
                _ => "boyfriend.x",
            };
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let tbl = lua.create_table()?;
            tbl.set("prop", target)?;
            tbl.set("value", val)?;
            pending.set(pending.len()? + 1, tbl)?;
            Ok(())
        })?,
    )?;
    globals.set(
        "setCharacterY",
        lua.create_function(|lua, (typ, val): (String, f64)| {
            let target = match typ.to_lowercase().as_str() {
                "dad" | "opponent" | "1" => "dad.y",
                "boyfriend" | "bf" | "0" => "boyfriend.y",
                "gf" | "girlfriend" | "2" => "gf.y",
                _ => "boyfriend.y",
            };
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let tbl = lua.create_table()?;
            tbl.set("prop", target)?;
            tbl.set("value", val)?;
            pending.set(pending.len()? + 1, tbl)?;
            Ok(())
        })?,
    )?;
    globals.set(
        "setCharacterScale",
        lua.create_function(|lua, (typ, scale): (String, f64)| {
            let target = match typ.to_lowercase().as_str() {
                "dad" | "opponent" | "1" => "dad.scale",
                "boyfriend" | "bf" | "0" => "boyfriend.scale",
                "gf" | "girlfriend" | "2" => "gf.scale",
                _ => "boyfriend.scale",
            };
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let tbl = lua.create_table()?;
            tbl.set("prop", target)?;
            tbl.set("value", scale)?;
            pending.set(pending.len()? + 1, tbl)?;
            Ok(())
        })?,
    )?;

    // Health bar
    globals.set(
        "setHealthBarColors",
        lua.create_function(|lua, (left, right): (LuaValue, LuaValue)| {
            for (side, hex) in [
                ("left", lua_value_to_color_string(&left)),
                ("right", lua_value_to_color_string(&right)),
            ] {
                let (r, g, b, a) = parse_hex_rgba(&hex);
                let props: LuaTable = lua.globals().get("__pending_props")?;
                let entry = lua.create_table()?;
                entry.set("type", "healthbar_color")?;
                entry.set("side", side)?;
                entry.set("r", r)?;
                entry.set("g", g)?;
                entry.set("b", b)?;
                entry.set("a", a)?;
                entry.set("duration", 0.0f32)?;
                let len = props.len()? + 1;
                props.set(len, entry)?;
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "setTimeBarColors",
        lua.create_function(|_lua, (_left, _right): (LuaValue, LuaValue)| Ok(()))?,
    )?;

    // Health/score control — use __health global synced from game
    globals.set("__health", 1.0)?;
    globals.set(
        "getHealth",
        lua.create_function(|lua, ()| -> LuaResult<f64> {
            Ok(lua.globals().get::<f64>("__health").unwrap_or(1.0))
        })?,
    )?;
    globals.set(
        "setHealth",
        lua.create_function(|lua, v: f64| {
            let g = lua.globals();
            g.set("__health", v)?;
            let pending: LuaTable = g.get("__pending_props")?;
            let tbl = lua.create_table()?;
            tbl.set("prop", "health")?;
            tbl.set("value", v)?;
            let len = pending.len()? as i64;
            pending.set(len + 1, tbl)?;
            Ok(())
        })?,
    )?;
    globals.set(
        "addHealth",
        lua.create_function(|lua, v: f64| {
            let g = lua.globals();
            let cur = g.get::<f64>("__health").unwrap_or(1.0);
            g.set("__health", cur + v)?;
            let pending: LuaTable = g.get("__pending_props")?;
            let tbl = lua.create_table()?;
            tbl.set("prop", "health")?;
            tbl.set("value", cur + v)?;
            let len = pending.len()? as i64;
            pending.set(len + 1, tbl)?;
            Ok(())
        })?,
    )?;
    globals.set(
        "addScore",
        lua.create_function(|lua, v: Option<i32>| {
            let cur = lua.globals().get::<i32>("score").unwrap_or(0);
            queue_score_property(lua, "score", cur + v.unwrap_or(0))
        })?,
    )?;
    globals.set(
        "setScore",
        lua.create_function(|lua, v: Option<i32>| {
            queue_score_property(lua, "score", v.unwrap_or(0))
        })?,
    )?;
    globals.set(
        "addMisses",
        lua.create_function(|lua, v: Option<i32>| {
            let cur = lua.globals().get::<i32>("misses").unwrap_or(0);
            queue_score_property(lua, "misses", cur + v.unwrap_or(0))
        })?,
    )?;
    globals.set(
        "setMisses",
        lua.create_function(|lua, v: Option<i32>| {
            queue_score_property(lua, "misses", v.unwrap_or(0))
        })?,
    )?;
    globals.set(
        "addHits",
        lua.create_function(|lua, v: Option<i32>| {
            let cur = lua.globals().get::<i32>("hits").unwrap_or(0);
            queue_score_property(lua, "hits", cur + v.unwrap_or(0))
        })?,
    )?;
    globals.set(
        "setHits",
        lua.create_function(|lua, v: Option<i32>| {
            queue_score_property(lua, "hits", v.unwrap_or(0))
        })?,
    )?;
    globals.set(
        "setRatingPercent",
        lua.create_function(|lua, v: f64| queue_score_property(lua, "rating", v))?,
    )?;
    globals.set(
        "setRatingName",
        lua.create_function(|lua, v: String| queue_score_property(lua, "ratingName", v))?,
    )?;
    globals.set(
        "setRatingFC",
        lua.create_function(|lua, v: String| queue_score_property(lua, "ratingFC", v))?,
    )?;
    globals.set("updateScoreText", lua.create_function(|_lua, ()| Ok(()))?)?;

    // cameraShake(camera, intensity, duration)
    globals.set(
        "cameraShake",
        lua.create_function(|lua, (cam, intensity, duration): (String, f64, f64)| {
            let pending: LuaTable = lua.globals().get("__pending_cam_fx")?;
            let tbl = lua.create_table()?;
            tbl.set("kind", "shake")?;
            let cam_name = match cam.to_lowercase().as_str() {
                "camhud" | "hud" => "camHUD",
                "camgame" | "game" => "camGame",
                _ => &cam,
            };
            tbl.set("camera", cam_name)?;
            tbl.set("intensity", intensity)?;
            tbl.set("duration", duration)?;
            let len = pending.len()? as i64;
            pending.set(len + 1, tbl)?;
            Ok(())
        })?,
    )?;

    // cameraFlash(camera, color, duration, forced)
    globals.set(
        "cameraFlash",
        lua.create_function(
            |lua,
             (cam, color, duration, _forced): (
                String,
                Option<String>,
                Option<f64>,
                Option<bool>,
            )| {
                let pending: LuaTable = lua.globals().get("__pending_cam_fx")?;
                let tbl = lua.create_table()?;
                tbl.set("kind", "flash")?;
                let cam_name = match cam.to_lowercase().as_str() {
                    "camhud" | "hud" => "camHUD",
                    "camgame" | "game" => "camGame",
                    _ => &cam,
                };
                tbl.set("camera", cam_name)?;
                tbl.set("color", color.unwrap_or_else(|| "FFFFFF".to_string()))?;
                tbl.set("duration", duration.unwrap_or(0.5))?;
                tbl.set("alpha", 1.0)?;
                let len = pending.len()? as i64;
                pending.set(len + 1, tbl)?;
                Ok(())
            },
        )?,
    )?;

    globals.set(
        "cameraFade",
        lua.create_function(
            |lua,
             (cam, color, duration, fade_in): (
                String,
                Option<String>,
                Option<f64>,
                Option<bool>,
            )| {
                let pending: LuaTable = lua.globals().get("__pending_cam_fx")?;
                let tbl = lua.create_table()?;
                tbl.set("kind", "fade")?;
                let cam_name = match cam.to_lowercase().as_str() {
                    "camhud" | "hud" => "camHUD",
                    "camgame" | "game" => "camGame",
                    _ => &cam,
                };
                tbl.set("camera", cam_name)?;
                tbl.set("color", color.unwrap_or_else(|| "000000".to_string()))?;
                tbl.set("duration", duration.unwrap_or(0.5))?;
                tbl.set("fade_in", fade_in.unwrap_or(false))?;
                let len = pending.len()? as i64;
                pending.set(len + 1, tbl)?;
                Ok(())
            },
        )?,
    )?;

    // setSubtitle(text, font, color, size, duration, borderColor)
    globals.set(
        "setSubtitle",
        lua.create_function(|lua, args: LuaMultiValue| {
            let text: String = args
                .get(0)
                .and_then(|v| match v {
                    LuaValue::String(s) => Some(s.to_string_lossy().to_string()),
                    _ => None,
                })
                .unwrap_or_default();
            let font: String = args
                .get(1)
                .and_then(|v| match v {
                    LuaValue::String(s) => Some(s.to_string_lossy().to_string()),
                    _ => None,
                })
                .unwrap_or_default();
            let color: String = args
                .get(2)
                .and_then(|v| match v {
                    LuaValue::String(s) => Some(s.to_string_lossy().to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| "0xFFFFFFFF".to_string());
            let size: f64 = args
                .get(3)
                .and_then(|v| match v {
                    LuaValue::Number(n) => Some(*n),
                    LuaValue::Integer(n) => Some(*n as f64),
                    _ => None,
                })
                .unwrap_or(32.0);
            let duration: f64 = args
                .get(4)
                .and_then(|v| match v {
                    LuaValue::Number(n) => Some(*n),
                    LuaValue::Integer(n) => Some(*n as f64),
                    _ => None,
                })
                .unwrap_or(3.0);
            let border: String = args
                .get(5)
                .and_then(|v| match v {
                    LuaValue::String(s) => Some(s.to_string_lossy().to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| "0x00000000".to_string());

            let pending: LuaTable = lua.globals().get("__pending_subtitles")?;
            let tbl = lua.create_table()?;
            tbl.set("text", text)?;
            tbl.set("font", font)?;
            tbl.set("color", color)?;
            tbl.set("size", size)?;
            tbl.set("duration", duration)?;
            tbl.set("border", border)?;
            let len = pending.len()? as i64;
            pending.set(len + 1, tbl)?;
            Ok(())
        })?,
    )?;

    // customFlash(camera, color, duration, options)
    globals.set(
        "customFlash",
        lua.create_function(
            |lua,
             (cam, color, duration, options): (
                String,
                Option<String>,
                Option<f64>,
                Option<LuaValue>,
            )| {
                let alpha = if let Some(LuaValue::Table(t)) = &options {
                    t.get::<f64>("alpha").unwrap_or(0.75)
                } else {
                    0.75
                };
                let pending: LuaTable = lua.globals().get("__pending_cam_fx")?;
                let tbl = lua.create_table()?;
                tbl.set("kind", "flash")?;
                let cam_name = match cam.to_lowercase().as_str() {
                    "camhud" | "hud" => "camHUD",
                    "camgame" | "game" => "camGame",
                    _ => &cam,
                };
                tbl.set("camera", cam_name)?;
                tbl.set("color", color.unwrap_or_else(|| "FFFFFF".to_string()))?;
                tbl.set("duration", duration.unwrap_or(0.5))?;
                tbl.set("alpha", alpha)?;
                let len = pending.len()? as i64;
                pending.set(len + 1, tbl)?;
                Ok(())
            },
        )?,
    )?;

    // customFade(camera, color, duration, options)
    globals.set(
        "customFade",
        lua.create_function(|_lua, _args: LuaMultiValue| -> LuaResult<()> { Ok(()) })?,
    )?;

    Ok(())
}

fn register_tween_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // Pending tweens: list of { tag, target, property, value, duration, ease }
    globals.set("__pending_tweens", lua.create_table()?)?;
    globals.set("__pending_tween_cancels", lua.create_table()?)?;
    globals.set("__pending_timers", lua.create_table()?)?;
    globals.set("__pending_timer_cancels", lua.create_table()?)?;

    // doTweenX(tag, target, value, duration, ease)
    globals.set("doTweenX", lua.create_function(|lua, (tag, target, value, duration, ease): (String, String, f64, f64, Option<String>)| {
        push_tween(lua, &tag, &target, "x", value, duration, ease.as_deref())
    })?)?;

    globals.set("doTweenY", lua.create_function(|lua, (tag, target, value, duration, ease): (String, String, f64, f64, Option<String>)| {
        push_tween(lua, &tag, &target, "y", value, duration, ease.as_deref())
    })?)?;

    globals.set("doTweenAlpha", lua.create_function(|lua, (tag, target, value, duration, ease): (String, String, f64, f64, Option<String>)| {
        push_tween(lua, &tag, &target, "alpha", value, duration, ease.as_deref())
    })?)?;

    globals.set("doTweenAngle", lua.create_function(|lua, (tag, target, value, duration, ease): (String, String, f64, f64, Option<String>)| {
        push_tween(lua, &tag, &target, "angle", value, duration, ease.as_deref())
    })?)?;

    globals.set("doTweenZoom", lua.create_function(|lua, (tag, camera, value, duration, ease): (String, String, f64, f64, Option<String>)| {
        // Normalize camera name
        let cam = match camera.to_lowercase().as_str() {
            "camgame" | "game" | "game.camgame" => "camGame",
            "camhud" | "hud" | "game.camhud" => "camHUD",
            _ => "camGame",
        };
        push_tween(lua, &tag, cam, "zoom", value, duration, ease.as_deref())
    })?)?;

    // doTweenColor(tag, target, color, duration, ease)
    globals.set(
        "doTweenColor",
        lua.create_function(
            |lua,
             (tag, target, color, duration, ease): (
                String,
                String,
                String,
                f64,
                Option<String>,
            )| {
                // Parse target color from hex
                let hex = color
                    .trim_start_matches('#')
                    .trim_start_matches("0x")
                    .trim_start_matches("0X");
                let color_val = u32::from_str_radix(hex, 16).unwrap_or(0xFFFFFFFF);
                let r = ((color_val >> 16) & 0xFF) as f64;
                let g_val = ((color_val >> 8) & 0xFF) as f64;
                let b = (color_val & 0xFF) as f64;
                // Convert to colorTransform offsets: offset = target_channel - 255 (when base color is white)
                // This is approximate — full impl would read current color. Using offset approach for consistency.
                let r_off = r - 255.0;
                let g_off = g_val - 255.0;
                let b_off = b - 255.0;
                // Create 3 color transform tweens
                let resolved = resolve_strum_target(&target);
                push_tween(
                    lua,
                    &format!("{}_r", tag),
                    &resolved,
                    "red_offset",
                    r_off,
                    duration,
                    ease.as_deref(),
                )?;
                push_tween(
                    lua,
                    &format!("{}_g", tag),
                    &resolved,
                    "green_offset",
                    g_off,
                    duration,
                    ease.as_deref(),
                )?;
                push_tween(
                    lua,
                    &format!("{}_b", tag),
                    &resolved,
                    "blue_offset",
                    b_off,
                    duration,
                    ease.as_deref(),
                )?;
                Ok(())
            },
        )?,
    )?;

    // startTween(tag, target, values_table, duration, options)
    // Generic tween function — tweens multiple properties on a game object.
    // target can be: "strumLineNotes.members[N]", a Lua sprite tag, or a game object path.
    globals.set(
        "startTween",
        lua.create_function(
            |lua,
             (tag, target, values, duration, options): (
                String,
                String,
                LuaTable,
                f64,
                Option<mlua::Value>,
            )| {
                let g = lua.globals();
                // Parse ease from options (string or table with ease key)
                let ease = match &options {
                    Some(mlua::Value::String(s)) => s.to_string_lossy().to_string(),
                    Some(mlua::Value::Table(tbl)) => tbl
                        .get::<mlua::prelude::LuaString>("ease")
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|_| "linear".to_string()),
                    _ => "linear".to_string(),
                };
                // Parse onComplete callback name from options
                let on_complete = match &options {
                    Some(mlua::Value::Table(tbl)) => tbl
                        .get::<mlua::prelude::LuaString>("onComplete")
                        .map(|s| s.to_string_lossy().to_string())
                        .ok(),
                    _ => None,
                };

                // Handle .colorTransform suffix: "scythe.colorTransform" → target "scythe", color context
                let (resolved_target, is_color_transform) =
                    if let Some(prefix) = target.strip_suffix(".colorTransform") {
                        (resolve_strum_target(prefix), true)
                    } else {
                        (resolve_strum_target(&target), false)
                    };

                let pending: LuaTable = g.get("__pending_tweens")?;
                let mut prop_count = 0;
                for _ in values.pairs::<mlua::Value, mlua::Value>() {
                    prop_count += 1;
                }

                let mut is_first = true;
                // Create one tween per property in the values table
                for pair in values.pairs::<String, f64>() {
                    let Ok((prop_name, end_val)) = pair else {
                        continue;
                    };
                    let prop = if is_color_transform {
                        match prop_name.as_str() {
                            "redOffset" => "red_offset",
                            "greenOffset" => "green_offset",
                            "blueOffset" => "blue_offset",
                            _ => {
                                log::debug!(
                                    "startTween: ignoring unknown colorTransform property '{}'",
                                    prop_name
                                );
                                continue;
                            }
                        }
                    } else {
                        match prop_name.as_str() {
                            "x" => "x",
                            "y" => "y",
                            "alpha" => "alpha",
                            "angle" => "angle",
                            "scale.x" | "scaleX" => "scale_x",
                            "scale.y" | "scaleY" => "scale_y",
                            "offset.x" => "offset_x",
                            "offset.y" => "offset_y",
                            _ => {
                                log::debug!(
                                    "startTween: ignoring unknown property '{}' on '{}'",
                                    prop_name,
                                    target
                                );
                                continue;
                            }
                        }
                    };
                    let tween_tag = if prop_count > 1 {
                        format!("{}_{}", tag, prop_name)
                    } else {
                        tag.clone()
                    };
                    let tbl = lua.create_table()?;
                    tbl.set("tag", tween_tag)?;
                    tbl.set("originalTag", tag.as_str())?;
                    tbl.set("target", resolved_target.as_str())?;
                    tbl.set("property", prop)?;
                    tbl.set("value", end_val)?;
                    tbl.set("duration", duration)?;
                    tbl.set("ease", ease.as_str())?;
                    if is_first {
                        if let Some(ref cb) = on_complete {
                            tbl.set("onComplete", cb.as_str())?;
                        }
                        is_first = false;
                    }
                    let len = pending.len()? as i64;
                    pending.set(len + 1, tbl)?;
                }
                Ok(())
            },
        )?,
    )?;

    // noteTweenX/Y/Alpha/Angle/Direction — tween strum note properties
    for name in [
        "noteTweenX",
        "noteTweenY",
        "noteTweenAlpha",
        "noteTweenAngle",
        "noteTweenDirection",
    ] {
        let prop = match name {
            "noteTweenX" => "x",
            "noteTweenY" => "y",
            "noteTweenAlpha" => "alpha",
            "noteTweenAngle" => "angle",
            _ => "x", // direction maps to x for now
        };
        let prop_owned = prop.to_string();
        globals.set(
            name,
            lua.create_function(
                move |lua,
                      (tag, note, value, duration, ease): (
                    String,
                    i32,
                    f64,
                    f64,
                    Option<String>,
                )| {
                    // note index: 0-3 = opponent, 4-7 = player
                    let strum_tag = if note < 4 {
                        format!("__strum_opponent_{}", note)
                    } else {
                        format!("__strum_player_{}", note - 4)
                    };
                    push_tween(
                        lua,
                        &tag,
                        &strum_tag,
                        &prop_owned,
                        value,
                        duration,
                        ease.as_deref(),
                    )
                },
            )?,
        )?;
    }

    globals.set(
        "cancelTween",
        lua.create_function(|lua, tag: String| {
            let cancels: LuaTable = lua.globals().get("__pending_tween_cancels")?;
            let len = cancels.len()? as i64;
            cancels.set(len + 1, tag)?;
            Ok(())
        })?,
    )?;

    // runTimer(tag, duration, loops)
    globals.set(
        "runTimer",
        lua.create_function(|lua, (tag, duration, loops): (String, f64, Option<i32>)| {
            let timers: LuaTable = lua.globals().get("__pending_timers")?;
            let tbl = lua.create_table()?;
            tbl.set("tag", tag)?;
            tbl.set("duration", duration)?;
            tbl.set("loops", loops.unwrap_or(1))?;
            let len = timers.len()? as i64;
            timers.set(len + 1, tbl)?;
            Ok(())
        })?,
    )?;

    globals.set(
        "cancelTimer",
        lua.create_function(|lua, tag: String| {
            let cancels: LuaTable = lua.globals().get("__pending_timer_cancels")?;
            let len = cancels.len()? as i64;
            cancels.set(len + 1, tag)?;
            Ok(())
        })?,
    )?;

    Ok(())
}

fn queue_score_property<V>(lua: &Lua, prop: &str, value: V) -> LuaResult<()>
where
    V: IntoLua,
{
    let lua_value = value.into_lua(lua)?;
    lua.globals().set(prop, lua_value.clone())?;
    let pending: LuaTable = lua.globals().get("__pending_props")?;
    let tbl = lua.create_table()?;
    tbl.set("prop", prop)?;
    tbl.set("value", lua_value)?;
    pending.set(pending.len()? + 1, tbl)?;
    Ok(())
}

fn push_tween(
    lua: &Lua,
    tag: &str,
    target: &str,
    property: &str,
    value: f64,
    duration: f64,
    ease: Option<&str>,
) -> LuaResult<()> {
    let tweens: LuaTable = lua.globals().get("__pending_tweens")?;
    let tbl = lua.create_table()?;
    tbl.set("tag", tag.to_string())?;
    tbl.set("originalTag", tag.to_string())?;
    tbl.set("target", target.to_string())?;
    tbl.set("property", property.to_string())?;
    tbl.set("value", value)?;
    tbl.set("duration", duration)?;
    tbl.set("ease", ease.unwrap_or("linear").to_string())?;
    let len = tweens.len()? as i64;
    tweens.set(len + 1, tbl)?;
    Ok(())
}

fn register_sound_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    globals.set("__music_volume", 1.0)?;
    globals.set("__music_time", 0.0)?;
    globals.set("__sound_pitch", 1.0)?;

    globals.set(
        "playSound",
        lua.create_function(
            |lua,
             (path, volume, tag, looping): (String, Option<f64>, Option<String>, Option<bool>)| {
                let pending: LuaTable = lua.globals().get("__pending_audio")?;
                let tbl = lua.create_table()?;
                tbl.set("kind", "play_sound")?;
                tbl.set("path", path)?;
                tbl.set("volume", volume.unwrap_or(1.0))?;
                if let Some(tag) = tag.filter(|s| !s.is_empty()) {
                    tbl.set("tag", tag.clone())?;
                    set_sound_table_value(lua, "__sound_volumes", &tag, volume.unwrap_or(1.0))?;
                    set_sound_table_value(lua, "__sound_times", &tag, 0.0)?;
                    set_sound_table_bool(lua, "__sound_exists", &tag, true)?;
                }
                tbl.set("looping", looping.unwrap_or(false))?;
                pending.set(pending.len()? + 1, tbl)?;
                Ok(())
            },
        )?,
    )?;
    globals.set(
        "playMusic",
        lua.create_function(
            |lua, (path, volume, looping): (String, Option<f64>, Option<bool>)| {
                let pending: LuaTable = lua.globals().get("__pending_audio")?;
                let tbl = lua.create_table()?;
                tbl.set("kind", "play_music")?;
                tbl.set("path", path)?;
                tbl.set("volume", volume.unwrap_or(1.0))?;
                tbl.set("looping", looping.unwrap_or(true))?;
                pending.set(pending.len()? + 1, tbl)?;
                lua.globals().set("__music_volume", volume.unwrap_or(1.0))?;
                Ok(())
            },
        )?,
    )?;
    globals.set(
        "stopSound",
        lua.create_function(|lua, tag: Option<String>| {
            queue_sound_tag_request(lua, "stop_sound", tag.as_deref(), &[])?;
            if let Some(tag) = tag.filter(|s| !s.is_empty()) {
                set_sound_table_bool(lua, "__sound_exists", &tag, false)?;
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "pauseSound",
        lua.create_function(|lua, tag: Option<String>| {
            queue_sound_tag_request(lua, "pause_sound", tag.as_deref(), &[])?;
            Ok(())
        })?,
    )?;
    globals.set(
        "pauseSounds",
        lua.create_function(|lua, tag: Option<String>| {
            queue_sound_tag_request(lua, "pause_sound", tag.as_deref(), &[])?;
            Ok(())
        })?,
    )?;
    globals.set(
        "resumeSound",
        lua.create_function(|lua, tag: Option<String>| {
            queue_sound_tag_request(lua, "resume_sound", tag.as_deref(), &[])?;
            Ok(())
        })?,
    )?;
    globals.set(
        "resumeSounds",
        lua.create_function(|lua, tag: Option<String>| {
            queue_sound_tag_request(lua, "resume_sound", tag.as_deref(), &[])?;
            Ok(())
        })?,
    )?;
    globals.set(
        "soundFadeIn",
        lua.create_function(|lua, args: LuaMultiValue| {
            let tag = multi_string(&args, 0);
            let duration = multi_number(&args, 1).unwrap_or(1.0);
            let from = multi_number(&args, 2).unwrap_or(0.0);
            let to = multi_number(&args, 3).unwrap_or(1.0);
            queue_sound_tag_request(
                lua,
                "sound_fade",
                tag.as_deref(),
                &[
                    ("duration", LuaValue::Number(duration)),
                    ("from", LuaValue::Number(from)),
                    ("to", LuaValue::Number(to)),
                    ("stop_when_done", LuaValue::Boolean(false)),
                ],
            )?;
            if let Some(tag) = tag.filter(|s| !s.is_empty()) {
                set_sound_table_value(lua, "__sound_volumes", &tag, to)?;
            } else {
                lua.globals().set("__music_volume", to)?;
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "soundFadeOut",
        lua.create_function(|lua, args: LuaMultiValue| {
            let tag = multi_string(&args, 0);
            let duration = multi_number(&args, 1).unwrap_or(1.0);
            queue_sound_tag_request(
                lua,
                "sound_fade",
                tag.as_deref(),
                &[
                    ("duration", LuaValue::Number(duration)),
                    ("to", LuaValue::Number(0.0)),
                    ("stop_when_done", LuaValue::Boolean(true)),
                ],
            )?;
            if let Some(tag) = tag.filter(|s| !s.is_empty()) {
                set_sound_table_value(lua, "__sound_volumes", &tag, 0.0)?;
            } else {
                lua.globals().set("__music_volume", 0.0)?;
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "soundFadeCancel",
        lua.create_function(|_lua, _tag: Option<String>| Ok(()))?,
    )?;
    globals.set(
        "getSoundVolume",
        lua.create_function(|lua, tag: Option<String>| -> LuaResult<f64> {
            if let Some(tag) = tag.filter(|s| !s.is_empty()) {
                let tbl: LuaTable = lua.globals().get("__sound_volumes")?;
                return Ok(tbl.get::<f64>(tag).unwrap_or(1.0));
            }
            Ok(lua.globals().get::<f64>("__music_volume").unwrap_or(1.0))
        })?,
    )?;
    globals.set(
        "setSoundVolume",
        lua.create_function(|lua, (tag, vol): (Option<String>, f64)| {
            queue_sound_tag_request(
                lua,
                "set_sound_volume",
                tag.as_deref(),
                &[("volume", LuaValue::Number(vol))],
            )?;
            if let Some(tag) = tag.filter(|s| !s.is_empty()) {
                set_sound_table_value(lua, "__sound_volumes", &tag, vol)?;
            } else {
                lua.globals().set("__music_volume", vol)?;
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "getSoundTime",
        lua.create_function(|lua, tag: Option<String>| -> LuaResult<f64> {
            if let Some(tag) = tag.filter(|s| !s.is_empty()) {
                let tbl: LuaTable = lua.globals().get("__sound_times")?;
                return Ok(tbl.get::<f64>(tag).unwrap_or(0.0));
            }
            Ok(lua.globals().get::<f64>("__music_time").unwrap_or(0.0))
        })?,
    )?;
    globals.set(
        "setSoundTime",
        lua.create_function(|lua, (tag, time): (Option<String>, f64)| {
            queue_sound_tag_request(
                lua,
                "set_sound_time",
                tag.as_deref(),
                &[("time", LuaValue::Number(time))],
            )?;
            if let Some(tag) = tag.filter(|s| !s.is_empty()) {
                set_sound_table_value(lua, "__sound_times", &tag, time)?;
            } else {
                lua.globals().set("__music_time", time)?;
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "luaSoundExists",
        lua.create_function(|lua, tag: String| -> LuaResult<bool> {
            if tag.is_empty() {
                return Ok(lua.globals().get::<f64>("__music_volume").is_ok());
            }
            let tbl: LuaTable = lua.globals().get("__sound_exists")?;
            Ok(tbl.get::<bool>(tag).unwrap_or(false))
        })?,
    )?;
    globals.set(
        "getSoundPitch",
        lua.create_function(|lua, tag: String| -> LuaResult<f64> {
            if tag.is_empty() {
                return Ok(lua.globals().get::<f64>("__sound_pitch").unwrap_or(1.0));
            }
            let tbl: LuaTable = lua.globals().get("__sound_pitches")?;
            Ok(tbl.get::<f64>(tag).unwrap_or(1.0))
        })?,
    )?;
    globals.set(
        "setSoundPitch",
        lua.create_function(
            |lua, (tag, pitch, _do_pause): (String, f64, Option<bool>)| {
                queue_sound_tag_request(
                    lua,
                    "set_sound_pitch",
                    Some(&tag),
                    &[("pitch", LuaValue::Number(pitch))],
                )?;
                if tag.is_empty() {
                    lua.globals().set("__sound_pitch", pitch)?;
                } else {
                    set_sound_table_value(lua, "__sound_pitches", &tag, pitch)?;
                }
                Ok(())
            },
        )?,
    )?;

    Ok(())
}

fn queue_sound_tag_request(
    lua: &Lua,
    kind: &str,
    tag: Option<&str>,
    fields: &[(&str, LuaValue)],
) -> LuaResult<()> {
    let pending: LuaTable = lua.globals().get("__pending_audio")?;
    let tbl = lua.create_table()?;
    tbl.set("kind", kind)?;
    if let Some(tag) = tag.filter(|s| !s.is_empty()) {
        tbl.set("tag", tag)?;
    }
    for (key, value) in fields {
        tbl.set(*key, value.clone())?;
    }
    pending.set(pending.len()? + 1, tbl)?;
    Ok(())
}

fn set_sound_table_value(lua: &Lua, table: &str, tag: &str, value: f64) -> LuaResult<()> {
    let tbl: LuaTable = lua.globals().get(table)?;
    tbl.set(tag, value)
}

fn set_sound_table_bool(lua: &Lua, table: &str, tag: &str, value: bool) -> LuaResult<()> {
    let tbl: LuaTable = lua.globals().get(table)?;
    tbl.set(tag, value)
}

fn multi_string(args: &LuaMultiValue, index: usize) -> Option<String> {
    args.get(index).and_then(|value| match value {
        LuaValue::String(s) => s.to_str().ok().map(|s| s.to_string()),
        LuaValue::Integer(i) => Some(i.to_string()),
        LuaValue::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn multi_number(args: &LuaMultiValue, index: usize) -> Option<f64> {
    args.get(index).and_then(|value| match value {
        LuaValue::Integer(i) => Some(*i as f64),
        LuaValue::Number(n) => Some(*n),
        LuaValue::String(s) => s.to_str().ok().and_then(|s| s.parse().ok()),
        _ => None,
    })
}

fn register_window_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();
    globals.set(
        "getScreenWidth",
        lua.create_function(|lua, ()| -> LuaResult<i32> {
            Ok(lua.globals().get::<i32>("screenWidth").unwrap_or(1280))
        })?,
    )?;
    globals.set(
        "getScreenHeight",
        lua.create_function(|lua, ()| -> LuaResult<i32> {
            Ok(lua.globals().get::<i32>("screenHeight").unwrap_or(720))
        })?,
    )?;
    Ok(())
}

fn register_text_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // makeLuaText(tag, text, width, x, y)
    globals.set(
        "makeLuaText",
        lua.create_function(
            |lua,
             (tag, text, width, x, y): (
                String,
                Option<String>,
                Option<f64>,
                Option<f64>,
                Option<f64>,
            )| {
                let text = text.unwrap_or_default();
                let width = width.unwrap_or(0.0) as f32;
                let x = x.unwrap_or(0.0) as f32;
                let y = y.unwrap_or(0.0) as f32;
                let tbl = lua.create_table()?;
                tbl.set("tag", tag.clone())?;
                tbl.set("text", text)?;
                tbl.set("x", x)?;
                tbl.set("y", y)?;
                tbl.set("width", width)?;
                tbl.set("alpha", 1.0)?;
                tbl.set("visible", true)?;
                tbl.set("angle", 0.0)?;
                tbl.set("font", "")?;
                tbl.set("size", 16.0)?;
                tbl.set("color", "FFFFFF")?;
                tbl.set("border_size", 0.0)?;
                tbl.set("border_color", "000000")?;
                tbl.set("alignment", "left")?;
                tbl.set("camera", "camGame")?;
                tbl.set("antialiasing", true)?;
                let text_data: LuaTable = lua.globals().get("__text_data")?;
                text_data.set(tag, tbl)?;
                Ok(())
            },
        )?,
    )?;

    // addLuaText(tag, inFront)
    globals.set(
        "addLuaText",
        lua.create_function(|lua, (tag, in_front): (String, Option<bool>)| {
            let in_front = in_front.unwrap_or(true);
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag.clone()) {
                let pending: LuaTable = lua.globals().get("__pending_texts")?;
                let len = pending.len()? as i64;
                pending.set(len + 1, tbl)?;
            }
            let adds: LuaTable = lua.globals().get("__pending_text_adds")?;
            let add_tbl = lua.create_table()?;
            add_tbl.set("tag", tag)?;
            add_tbl.set("in_front", in_front)?;
            let len = adds.len()? as i64;
            adds.set(len + 1, add_tbl)?;
            Ok(())
        })?,
    )?;

    globals.set(
        "removeLuaText",
        lua.create_function(|lua, (tag, _destroy): (String, Option<bool>)| {
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag.clone()) {
                tbl.set("visible", false)?;
            }
            // Reuse sprite remove queue
            let pending: LuaTable = lua.globals().get("__pending_removes")?;
            let len = pending.len()? as i64;
            pending.set(len + 1, format!("__text_{}", tag))?;
            Ok(())
        })?,
    )?;

    globals.set(
        "setTextString",
        lua.create_function(|lua, (tag, text): (String, String)| {
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                tbl.set("text", text)?;
            }
            Ok(())
        })?,
    )?;

    globals.set(
        "setTextSize",
        lua.create_function(|lua, (tag, size): (String, f64)| {
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                tbl.set("size", size as f32)?;
            }
            Ok(())
        })?,
    )?;

    globals.set(
        "setTextColor",
        lua.create_function(|lua, (tag, color): (String, String)| {
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                tbl.set("color", color)?;
            }
            Ok(())
        })?,
    )?;

    globals.set(
        "setTextFont",
        lua.create_function(|lua, (tag, font): (String, String)| {
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                tbl.set("font", font)?;
            }
            Ok(())
        })?,
    )?;

    globals.set(
        "setTextBorder",
        lua.create_function(|lua, args: LuaMultiValue| {
            let tag: String = args
                .get(0)
                .and_then(|v| match v {
                    LuaValue::String(s) => Some(s.to_string_lossy().to_string()),
                    _ => None,
                })
                .unwrap_or_default();
            let size: f32 = args
                .get(1)
                .and_then(|v| match v {
                    LuaValue::Number(n) => Some(*n as f32),
                    LuaValue::Integer(n) => Some(*n as f32),
                    _ => None,
                })
                .unwrap_or(0.0);
            let color: String = args
                .get(2)
                .and_then(|v| match v {
                    LuaValue::String(s) => Some(s.to_string_lossy().to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| "000000".to_string());
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                tbl.set("border_size", size)?;
                tbl.set("border_color", color)?;
            }
            Ok(())
        })?,
    )?;

    globals.set(
        "setTextAlignment",
        lua.create_function(|lua, (tag, align): (String, String)| {
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                tbl.set("alignment", align)?;
            }
            Ok(())
        })?,
    )?;

    globals.set(
        "setTextWidth",
        lua.create_function(|lua, (tag, w): (String, f64)| {
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                tbl.set("width", w as f32)?;
            }
            Ok(())
        })?,
    )?;

    globals.set(
        "setTextAutoSize",
        lua.create_function(|_lua, (_tag, _v): (String, bool)| Ok(()))?,
    )?;

    globals.set(
        "getTextString",
        lua.create_function(|lua, tag: String| -> LuaResult<String> {
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
                return Ok(tbl.get::<String>("text").unwrap_or_default());
            }
            Ok(String::new())
        })?,
    )?;

    globals.set(
        "luaTextExists",
        lua.create_function(|lua, tag: String| -> LuaResult<bool> {
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            Ok(text_data.contains_key(tag)?)
        })?,
    )?;

    Ok(())
}

/// Register a custom note type from Lua.
/// registerNoteType(name, configTable)
/// configTable fields (all optional):
///   hitCausesMiss: bool (default false)
///   hitDamage: number (0.0–1.0, default 0.0)
///   ignoreMiss: bool (default false)
///   noteSkin: string (atlas path relative to images dir)
///   noteAnims: {string, string, string, string} (4 direction anim names)
///   strumAnims: {string, string, string, string}
///   confirmAnims: {string, string, string, string}
///   hitSfx: string (path relative to sounds dir, without extension)
///   healthDrainPct: number (0.0–1.0, fraction of max health to slide-drain)
///   drainDeathSafe: bool (if true, first drain stops just above death)
fn register_note_type_function(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // Pending note type registrations: table of {name, config...}
    globals.set("__pending_note_types", lua.create_table()?)?;

    globals.set(
        "registerNoteType",
        lua.create_function(|lua, (name, config): (String, LuaTable)| {
            let pending: LuaTable = lua.globals().get("__pending_note_types")?;

            let entry = lua.create_table()?;
            entry.set("name", name.clone())?;
            // Store all fields as-is; the app layer will parse them into NoteTypeConfig
            entry.set(
                "hitCausesMiss",
                config.get::<bool>("hitCausesMiss").unwrap_or(false),
            )?;
            entry.set("hitDamage", config.get::<f64>("hitDamage").unwrap_or(0.0))?;
            entry.set(
                "ignoreMiss",
                config.get::<bool>("ignoreMiss").unwrap_or(false),
            )?;
            if let Ok(v) = config.get::<String>("noteSkin") {
                entry.set("noteSkin", v)?;
            }
            if let Ok(v) = config.get::<String>("hitSfx") {
                entry.set("hitSfx", v)?;
            }
            if let Ok(v) = config.get::<f64>("healthDrainPct") {
                entry.set("healthDrainPct", v)?;
            }
            if let Ok(v) = config.get::<bool>("drainDeathSafe") {
                entry.set("drainDeathSafe", v)?;
            }

            // Animation arrays: store as sub-tables
            for &key in &["noteAnims", "strumAnims", "confirmAnims"] {
                if let Ok(tbl) = config.get::<LuaTable>(key) {
                    let arr = lua.create_table()?;
                    for i in 1..=4 {
                        if let Ok(v) = tbl.get::<String>(i) {
                            arr.set(i, v)?;
                        }
                    }
                    entry.set(key, arr)?;
                }
            }

            let len = pending.len()? as i64;
            pending.set(len + 1, entry)?;
            log::info!("Queued note type registration '{}' from Lua", name);
            Ok(())
        })?,
    )?;

    // addLuaScript(name)
    lua.globals().set(
        "addLuaScript",
        lua.create_function(|lua, name: String| {
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let entry = lua.create_table()?;
            entry.set("type", "add_script")?;
            entry.set("script_name", name)?;
            let len = pending.len()? + 1;
            pending.set(len, entry)?;
            Ok(())
        })?,
    )?;

    Ok(())
}

/// Register no-op stubs for all remaining Psych Engine functions that scripts may call.
fn register_noop_stubs(lua: &Lua) -> LuaResult<()> {
    // Psych save data compatibility. This is process-local, shared across Lua VMs,
    // and preserves values across scripts during a run.
    lua.globals().set(
        "initSaveData",
        lua.create_function(|_lua, save_name: String| {
            let mut saves = save_data()
                .lock()
                .map_err(|e| LuaError::external(e.to_string()))?;
            saves.entry(save_name).or_default();
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "getDataFromSave",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<LuaValue> {
            let mut args_iter = args.into_iter();
            let save_name = lua_value_to_string(args_iter.next()).unwrap_or_default();
            let key = lua_value_to_string(args_iter.next()).unwrap_or_default();
            let default = args_iter.next().unwrap_or(LuaValue::Nil);

            let saves = save_data()
                .lock()
                .map_err(|e| LuaError::external(e.to_string()))?;
            if let Some(value) = saves.get(&save_name).and_then(|save| save.get(&key)) {
                save_value_to_lua(lua, value)
            } else {
                Ok(default)
            }
        })?,
    )?;
    lua.globals().set(
        "setDataFromSave",
        lua.create_function(
            |_lua, (save_name, key, value): (String, String, LuaValue)| {
                let mut saves = save_data()
                    .lock()
                    .map_err(|e| LuaError::external(e.to_string()))?;
                saves
                    .entry(save_name)
                    .or_default()
                    .insert(key, lua_value_to_save(&value));
                Ok(())
            },
        )?,
    )?;
    lua.globals().set(
        "flushSaveData",
        lua.create_function(|_lua, save_name: Option<String>| {
            if let Some(save_name) = save_name {
                let mut saves = save_data()
                    .lock()
                    .map_err(|e| LuaError::external(e.to_string()))?;
                saves.entry(save_name).or_default();
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "eraseSaveData",
        lua.create_function(|_lua, save_name: String| {
            let mut saves = save_data()
                .lock()
                .map_err(|e| LuaError::external(e.to_string()))?;
            saves.remove(&save_name);
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "getModSetting",
        lua.create_function(
            |lua, (save_tag, mod_name): (String, Option<String>)| -> LuaResult<LuaValue> {
                let mod_name = mod_name
                    .filter(|name| !name.trim().is_empty())
                    .or_else(|| lua.globals().get::<String>("modFolder").ok())
                    .or_else(|| lua.globals().get::<String>("currentModDirectory").ok())
                    .unwrap_or_default();
                let Some(path) = find_mod_settings_file(lua, &mod_name) else {
                    return Ok(LuaNil);
                };
                let Ok(contents) = std::fs::read_to_string(path) else {
                    return Ok(LuaNil);
                };
                parse_mod_setting(lua, &contents, &save_tag)
            },
        )?,
    )?;

    lua.globals().set(
        "checkFileExists",
        lua.create_function(|lua, (relative, _absolute): (String, Option<bool>)| {
            Ok(find_rooted_file(lua, &relative).is_some())
        })?,
    )?;
    lua.globals().set(
        "getTextFromFile",
        lua.create_function(|lua, relative: String| -> LuaResult<String> {
            let Some(path) = find_rooted_file(lua, &relative) else {
                return Ok(String::new());
            };
            Ok(std::fs::read_to_string(path).unwrap_or_default())
        })?,
    )?;
    lua.globals().set(
        "saveFile",
        lua.create_function(
            |lua, (path, content, absolute): (String, String, Option<bool>)| -> LuaResult<bool> {
                let Some(path) = writable_lua_file(lua, &path, absolute.unwrap_or(false)) else {
                    return Ok(false);
                };
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                Ok(std::fs::write(path, content).is_ok())
            },
        )?,
    )?;
    lua.globals().set(
        "deleteFile",
        lua.create_function(
            |lua,
             (path, ignore_mod_folders, absolute): (String, Option<bool>, Option<bool>)|
             -> LuaResult<bool> {
                let resolved = if absolute.unwrap_or(false) {
                    Some(std::path::PathBuf::from(path))
                } else if ignore_mod_folders.unwrap_or(false) {
                    find_rooted_file(lua, &path)
                } else {
                    writable_lua_file(lua, &path, false)
                };
                let Some(path) = resolved else {
                    return Ok(false);
                };
                Ok(path.is_file() && std::fs::remove_file(path).is_ok())
            },
        )?,
    )?;
    lua.globals().set(
        "directoryFileList",
        lua.create_function(|lua, folder: String| -> LuaResult<LuaTable> {
            let out = lua.create_table()?;
            if let Some(path) = find_rooted_dir(lua, &folder) {
                if let Ok(entries) = std::fs::read_dir(path) {
                    let mut names: Vec<String> = entries
                        .flatten()
                        .map(|entry| entry.file_name().to_string_lossy().to_string())
                        .collect();
                    names.sort();
                    for (i, name) in names.iter().enumerate() {
                        out.set(i + 1, name.as_str())?;
                    }
                }
            }
            Ok(out)
        })?,
    )?;

    // Shader compatibility: store parameters so scripts can read back values
    // they write, even when the renderer does not implement the shader.
    lua.globals().set(
        "initLuaShader",
        lua.create_function(|_lua, (_name, _glsl_version): (String, Option<i32>)| Ok(true))?,
    )?;
    lua.globals().set(
        "setSpriteShader",
        lua.create_function(|lua, (tag, shader): (String, String)| {
            let g = lua.globals();
            if let Ok(sprite_data) = g.get::<LuaTable>("__sprite_data") {
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.as_str()) {
                    tbl.set("shader", shader.clone()).ok();
                }
            }
            let vars: LuaTable = g.get("__custom_vars")?;
            vars.set(format!("{tag}.shader"), shader)?;
            Ok(true)
        })?,
    )?;
    register_shader_scalar(lua, "Float")?;
    register_shader_scalar(lua, "Int")?;
    register_shader_bool(lua)?;
    register_shader_array(lua, "Float", LuaValue::Number(0.0))?;
    register_shader_array(lua, "Int", LuaValue::Integer(0))?;
    register_shader_array(lua, "Bool", LuaValue::Boolean(false))?;
    lua.globals().set(
        "removeSpriteShader",
        lua.create_function(|lua, tag: String| {
            if let Ok(sprite_data) = lua.globals().get::<LuaTable>("__sprite_data") {
                if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.as_str()) {
                    tbl.set("shader", LuaNil).ok();
                }
            }
            let vars: LuaTable = lua.globals().get("__custom_vars")?;
            vars.set(format!("{tag}.shader"), LuaNil)?;
            Ok(true)
        })?,
    )?;
    lua.globals().set(
        "setShaderSampler2D",
        lua.create_function(|lua, (target, field, path): (String, String, String)| {
            let shaders: LuaTable = lua.globals().get("__shader_data")?;
            shaders.set(shader_key(&target, &field), path)?;
            Ok(true)
        })?,
    )?;

    register_misc_default_functions(lua)?;
    register_control_default_functions(lua)?;
    register_reflection_default_functions(lua)?;
    register_deprecated_aliases(lua)?;

    lua.globals().set(
        "removeLuaScript",
        lua.create_function(|lua, name: String| {
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let entry = lua.create_table()?;
            entry.set("type", "remove_script")?;
            entry.set("script_name", name)?;
            let len = pending.len()? + 1;
            pending.set(len, entry)?;
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "removeHScript",
        lua.create_function(|lua, name: String| {
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let entry = lua.create_table()?;
            entry.set("type", "remove_script")?;
            entry.set("script_name", name)?;
            let len = pending.len()? + 1;
            pending.set(len, entry)?;
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "addHScript",
        lua.create_function(|lua, name: String| {
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let entry = lua.create_table()?;
            entry.set("type", "add_script")?;
            entry.set("script_name", name)?;
            let len = pending.len()? + 1;
            pending.set(len, entry)?;
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "callScript",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<LuaValue> {
            let mut args = args.into_iter();
            let target = lua_value_to_string(args.next()).unwrap_or_default();
            let function = lua_value_to_string(args.next()).unwrap_or_default();
            if target.is_empty() || function.is_empty() {
                return Ok(LuaNil);
            }
            let call_args = lua_call_args_to_vec(args.next())?;
            queue_script_call(lua, Some(&target), &function, call_args)?;
            Ok(LuaNil)
        })?,
    )?;
    lua.globals().set(
        "callOnLuas",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<LuaValue> {
            let mut args = args.into_iter();
            let function = lua_value_to_string(args.next()).unwrap_or_default();
            if function.is_empty() {
                return Ok(LuaNil);
            }
            let call_args = lua_call_args_to_vec(args.next())?;
            queue_script_call(lua, None, &function, call_args)?;
            Ok(LuaNil)
        })?,
    )?;
    lua.globals().set(
        "callOnScripts",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<LuaValue> {
            let mut args = args.into_iter();
            let function = lua_value_to_string(args.next()).unwrap_or_default();
            if function.is_empty() {
                return Ok(LuaNil);
            }
            let call_args = lua_call_args_to_vec(args.next())?;
            queue_script_call(lua, None, &function, call_args)?;
            Ok(LuaNil)
        })?,
    )?;
    lua.globals().set(
        "callOnHScript",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<LuaValue> {
            let mut args = args.into_iter();
            let function = lua_value_to_string(args.next()).unwrap_or_default();
            if function.is_empty() {
                return Ok(LuaNil);
            }
            let call_args = lua_call_args_to_vec(args.next())?;
            queue_script_call(lua, None, &function, call_args)?;
            Ok(LuaNil)
        })?,
    )?;
    for name in ["setOnLuas", "setOnScripts", "setOnHScript"] {
        lua.globals().set(
            name,
            lua.create_function(|lua, (key, value): (String, LuaValue)| {
                queue_script_global(lua, &key, value)?;
                Ok(())
            })?,
        )?;
    }
    lua.globals().set(
        "isRunning",
        lua.create_function(|lua, target: String| -> LuaResult<bool> {
            let running: LuaTable = lua.globals().get("__running_scripts")?;
            let len = running.len()?;
            for i in 1..=len {
                if let Ok(script) = running.get::<String>(i) {
                    if lua_script_target_matches(&script, &target) {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        })?,
    )?;
    lua.globals().set(
        "getRunningScripts",
        lua.create_function(|lua, ()| -> LuaResult<LuaTable> {
            lua.globals().get("__running_scripts")
        })?,
    )?;

    // startVideo(videoFile, canSkip=true, forMidSong=false, shouldLoop=false, playOnLoad=true)
    // Also accepts the older Rustic form startVideo(filename, callbackName).
    lua.globals().set(
        "startVideo",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<bool> {
            let filename = multi_string(&args, 0).unwrap_or_default();
            if filename.is_empty() {
                return Ok(false);
            }
            let callback = match args.get(1) {
                Some(LuaValue::String(s)) => Some(s.to_string_lossy().to_string()),
                _ => None,
            };
            let for_mid_song = match args.get(2) {
                Some(LuaValue::Boolean(v)) => *v,
                _ => false,
            };
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let entry = lua.create_table()?;
            entry.set("type", "video")?;
            entry.set("filename", filename)?;
            if let Some(ref cb) = callback {
                entry.set("callback", cb.as_str())?;
            }
            entry.set("blocks_gameplay", !for_mid_song)?;
            let len = pending.len()? + 1;
            pending.set(len, entry)?;
            Ok(true)
        })?,
    )?;
    lua.globals().set(
        "changePresence",
        lua.create_function(|_lua, _args: LuaMultiValue| -> LuaResult<LuaValue> { Ok(LuaNil) })?,
    )?;
    lua.globals().set(
        "runHaxeCodePost",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<LuaValue> {
            let func: LuaFunction = lua.globals().get("runHaxeCode")?;
            func.call(args)
        })?,
    )?;
    for name in ["createCallback", "createGlobalCallback"] {
        lua.globals().set(
            name,
            lua.create_function(|lua, (name, func): (String, LuaFunction)| {
                lua.globals().set(name, func)?;
                Ok(true)
            })?,
        )?;
    }
    lua.globals().set(
        "isPaused",
        lua.create_function(|lua, ()| -> LuaResult<bool> {
            Ok(lua.globals().get::<bool>("__is_paused").unwrap_or(false))
        })?,
    )?;
    lua.globals().set(
        "createRuntimeShader",
        lua.create_function(|lua, name: String| -> LuaResult<LuaValue> {
            let func: LuaFunction = lua.globals().get("initLuaShader")?;
            let _ = func.call::<bool>(name.clone());
            Ok(LuaValue::String(lua.create_string(&name)?))
        })?,
    )?;
    lua.globals().set(
        "addLuaSpriteSubstate",
        lua.create_function(|lua, (tag, in_front): (String, Option<bool>)| {
            let func: LuaFunction = lua.globals().get("addLuaSprite")?;
            func.call::<()>((tag, in_front.or(Some(true))))
        })?,
    )?;
    lua.globals().set(
        "removeLuaSpriteSubstate",
        lua.create_function(|lua, (tag, destroy): (String, Option<bool>)| {
            let func: LuaFunction = lua.globals().get("removeLuaSprite")?;
            func.call::<()>((tag, destroy))
        })?,
    )?;
    lua.globals().set(
        "addLuaTextSubstate",
        lua.create_function(|lua, tag: String| {
            let func: LuaFunction = lua.globals().get("addLuaText")?;
            func.call::<()>((tag, true))
        })?,
    )?;
    lua.globals().set(
        "removeLuaTextSubstate",
        lua.create_function(|lua, (tag, destroy): (String, Option<bool>)| {
            let func: LuaFunction = lua.globals().get("removeLuaText")?;
            func.call::<()>((tag, destroy))
        })?,
    )?;
    lua.globals().set(
        "insertToCustomSubstate",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<bool> {
            let Some(tag) = multi_string(&args, 0).or_else(|| multi_string(&args, 1)) else {
                return Ok(false);
            };
            if text_table(lua, &tag).is_some() {
                let func: LuaFunction = lua.globals().get("addLuaText")?;
                func.call::<()>((tag, true))?;
            } else if lua_sprite_or_text_exists(lua, &tag) {
                let func: LuaFunction = lua.globals().get("addLuaSprite")?;
                func.call::<()>((tag, true))?;
            } else {
                return Ok(false);
            }
            Ok(true)
        })?,
    )?;

    let noop =
        lua.create_function(|_lua, _args: LuaMultiValue| -> LuaResult<LuaValue> { Ok(LuaNil) })?;

    for name in [
        // Haxe integration
        "runHaxeCodePost",
        "addHaxeLibrary",
        // Script management
        "addLuaScript",
        "removeLuaScript",
        "addHScript",
        "removeHScript",
        "isRunning",
        "callScript",
        "getRunningScripts",
        "callOnLuas",
        "callOnScripts",
        "callOnHScript",
        "setOnLuas",
        "setOnScripts",
        "setOnHScript",
        "createCallback",
        "createGlobalCallback",
        // Song control
        "startCountdown",
        "endSong",
        "exitSong",
        "restartSong",
        "loadSong",
        "startDialogue",
        // Precaching
        "precacheImage",
        "precacheSound",
        "precacheMusic",
        // Camera
        "setCameraScroll",
        "setCameraFollowPoint",
        "addCameraScroll",
        "addCameraFollowPoint",
        "getCameraScrollX",
        "getCameraScrollY",
        "getCameraFollowX",
        "getCameraFollowY",
        "getMouseX",
        "getMouseY",
        // Character management
        "addCharacterToList",
        // Reflection
        "callMethod",
        "callMethodFromClass",
        "createInstance",
        "addInstance",
        "instanceArg",
        "addToGroup",
        "removeFromGroup",
        "objectsOverlap",
        // Position queries
        "getScreenPositionX",
        "getScreenPositionY",
        // Keyboard/gamepad (beyond the basic ones)
        "keyboardJustPressed",
        "keyboardPressed",
        "keyboardReleased",
        "mouseClicked",
        "mousePressed",
        "mouseReleased",
        "anyGamepadJustPressed",
        "anyGamepadPressed",
        "anyGamepadReleased",
        "gamepadJustPressed",
        "gamepadPressed",
        "gamepadReleased",
        "gamepadAnalogX",
        "gamepadAnalogY",
        // Save data
        "initSaveData",
        "flushSaveData",
        "setDataFromSave",
        "eraseSaveData",
        // Presence / pause
        "changePresence",
        "isPaused",
        // File I/O
        "checkFileExists",
        "getTextFromFile",
        "saveFile",
        "deleteFile",
        "directoryFileList",
        // Shader
        "initLuaShader",
        "setSpriteShader",
        "removeSpriteShader",
        "getShaderBool",
        "setShaderBool",
        "getShaderBoolArray",
        "setShaderBoolArray",
        "getShaderInt",
        "setShaderInt",
        "getShaderIntArray",
        "setShaderIntArray",
        "getShaderFloat",
        "setShaderFloat",
        "getShaderFloatArray",
        "setShaderFloatArray",
        "setShaderSampler2D",
        // Misc
        "getColorFromString",
        "getColorFromName",
        "setHudVisible",
        "addShake",
        // Substates
        "openCustomSubstate",
        "closeCustomSubstate",
        "addLuaSpriteSubstate",
        "removeLuaSpriteSubstate",
        "addLuaTextSubstate",
        "removeLuaTextSubstate",
        "insertToCustomSubstate",
        // Shader (runtime)
        "createRuntimeShader",
        // Text queries
        "getTextFont",
        "getTextSize",
        "getTextWidth",
        // Sound
        "getSoundPitch",
        "setSoundPitch",
        // Pixel
        "getPixelColor",
        // Atlas
        "loadAnimateAtlas",
        "loadFrames",
        "loadMultipleFrames",
        "makeFlxAnimateSprite",
        "addAnimationBySymbol",
        "addAnimationBySymbolIndices",
        // Deprecated aliases (Psych Engine DeprecatedFunctions.hx)
        "luaSpriteMakeGraphic",
        "luaSpriteAddAnimationByPrefix",
        "luaSpriteAddAnimationByIndices",
        "luaSpritePlayAnimation",
        "setLuaSpriteCamera",
        "setLuaSpriteScrollFactor",
        "scaleLuaSprite",
        "getPropertyLuaSprite",
        "setPropertyLuaSprite",
        "musicFadeIn",
        "musicFadeOut",
        // Timers
        "onTweenCompleted",
        "onTimerCompleted",
        "onSoundFinished",
    ] {
        // Only set if not already registered (avoid overwriting real implementations)
        if lua.globals().get::<LuaValue>(name)? == LuaNil {
            lua.globals().set(name, noop.clone())?;
        }
    }

    lua.globals().set(
        "setHudVisible",
        lua.create_function(|lua, visible: bool| {
            let pending: LuaTable = lua.globals().get("__pending_props")?;
            let entry = lua.create_table()?;
            entry.set("prop", "camHUD.visible")?;
            entry.set("value", visible)?;
            pending.set(pending.len()? + 1, entry)?;
            Ok(())
        })?,
    )?;

    // === Rustic extensions: stage overlay, post-processing, health bar ===

    // setStageColor(side, hexColor, duration)
    // side: "left", "right", or "both"
    let set_stage_color =
        lua.create_function(|lua, (side, hex, dur): (String, String, Option<f64>)| {
            let hex = hex.trim_start_matches('#').trim_start_matches("0x");
            let (r, g, b, a) = parse_hex_rgba(hex);
            let dur = dur.unwrap_or(0.3) as f32;
            let props: LuaTable = lua.globals().get("__pending_props")?;
            let entry = lua.create_table()?;
            entry.set("type", "stage_color")?;
            entry.set("side", side)?;
            entry.set("r", r)?;
            entry.set("g", g)?;
            entry.set("b", b)?;
            entry.set("a", a)?;
            entry.set("duration", dur)?;
            let len = props.len()? + 1;
            props.set(len, entry)?;
            Ok(())
        })?;
    lua.globals().set("setStageColor", set_stage_color)?;

    // swapStageColors(duration)
    let swap_stage_colors = lua.create_function(|lua, dur: Option<f64>| {
        let dur = dur.unwrap_or(0.15) as f32;
        let props: LuaTable = lua.globals().get("__pending_props")?;
        let entry = lua.create_table()?;
        entry.set("type", "stage_color_swap")?;
        entry.set("duration", dur)?;
        let len = props.len()? + 1;
        props.set(len, entry)?;
        Ok(())
    })?;
    lua.globals().set("swapStageColors", swap_stage_colors)?;

    // setStageLights(on)
    let set_stage_lights = lua.create_function(|lua, on: bool| {
        let props: LuaTable = lua.globals().get("__pending_props")?;
        let entry = lua.create_table()?;
        entry.set("type", "stage_lights")?;
        entry.set("on", on)?;
        let len = props.len()? + 1;
        props.set(len, entry)?;
        Ok(())
    })?;
    lua.globals().set("setStageLights", set_stage_lights)?;

    // setPostProcessing(enabled, tweenDuration)
    let set_postprocessing = lua.create_function(|lua, (enabled, dur): (bool, Option<f64>)| {
        let dur = dur.unwrap_or(1.0) as f32;
        let props: LuaTable = lua.globals().get("__pending_props")?;
        let entry = lua.create_table()?;
        entry.set("type", "postprocess")?;
        entry.set("enabled", enabled)?;
        entry.set("duration", dur)?;
        let len = props.len()? + 1;
        props.set(len, entry)?;
        Ok(())
    })?;
    lua.globals().set("setPostProcessing", set_postprocessing)?;

    // setPostProcessParam(param, value) — set individual postprocess shader uniform
    // Valid params: "scanline", "distortion", "chromatic", "vignette", "enabled", "time"
    let set_pp_param = lua.create_function(|lua, (param, value): (String, f64)| {
        let props: LuaTable = lua.globals().get("__pending_props")?;
        let entry = lua.create_table()?;
        entry.set("type", "postprocess_param")?;
        entry.set("param", param)?;
        entry.set("value", value as f32)?;
        let len = props.len()? + 1;
        props.set(len, entry)?;
        Ok(())
    })?;
    lua.globals().set("setPostProcessParam", set_pp_param)?;

    // setHealthBarColor(side, hexColor, duration)
    let set_hb_color =
        lua.create_function(|lua, (side, hex, dur): (String, String, Option<f64>)| {
            let hex = hex.trim_start_matches('#').trim_start_matches("0x");
            let (r, g, b, a) = parse_hex_rgba(hex);
            let dur = dur.unwrap_or(1.0) as f32;
            let props: LuaTable = lua.globals().get("__pending_props")?;
            let entry = lua.create_table()?;
            entry.set("type", "healthbar_color")?;
            entry.set("side", side)?;
            entry.set("r", r)?;
            entry.set("g", g)?;
            entry.set("b", b)?;
            entry.set("a", a)?;
            entry.set("duration", dur)?;
            let len = props.len()? + 1;
            props.set(len, entry)?;
            Ok(())
        })?;
    lua.globals().set("setHealthBarColor", set_hb_color)?;

    // setReflections(enabled)
    let set_reflections = lua.create_function(|lua, enabled: bool| {
        let props: LuaTable = lua.globals().get("__pending_props")?;
        let entry = lua.create_table()?;
        entry.set("type", "reflections")?;
        entry.set("enabled", enabled)?;
        let len = props.len()? + 1;
        props.set(len, entry)?;
        Ok(())
    })?;
    lua.globals().set("setReflections", set_reflections)?;

    Ok(())
}

// === Helpers ===

fn lua_value_to_save(value: &LuaValue) -> SaveValue {
    match value {
        LuaValue::Nil => SaveValue::Nil,
        LuaValue::Boolean(v) => SaveValue::Bool(*v),
        LuaValue::Integer(v) => SaveValue::Int(*v),
        LuaValue::Number(v) => SaveValue::Float(*v),
        LuaValue::String(v) => SaveValue::String(v.to_string_lossy().to_string()),
        LuaValue::Table(t) => {
            let len = t.len().unwrap_or(0);
            let mut values = Vec::new();
            for i in 1..=len {
                values.push(
                    t.get::<LuaValue>(i)
                        .map(|v| lua_value_to_save(&v))
                        .unwrap_or(SaveValue::Nil),
                );
            }
            SaveValue::Array(values)
        }
        _ => SaveValue::Nil,
    }
}

fn save_value_to_lua(lua: &Lua, value: &SaveValue) -> LuaResult<LuaValue> {
    match value {
        SaveValue::Nil => Ok(LuaValue::Nil),
        SaveValue::Bool(v) => Ok(LuaValue::Boolean(*v)),
        SaveValue::Int(v) => Ok(LuaValue::Integer(*v)),
        SaveValue::Float(v) => Ok(LuaValue::Number(*v)),
        SaveValue::String(v) => Ok(LuaValue::String(lua.create_string(v)?)),
        SaveValue::Array(values) => {
            let tbl = lua.create_table()?;
            for (i, item) in values.iter().enumerate() {
                tbl.set(i + 1, save_value_to_lua(lua, item)?)?;
            }
            Ok(LuaValue::Table(tbl))
        }
    }
}

fn lua_value_to_string(value: Option<LuaValue>) -> Option<String> {
    match value? {
        LuaValue::String(s) => Some(s.to_string_lossy().to_string()),
        LuaValue::Integer(i) => Some(i.to_string()),
        LuaValue::Number(n) => Some(format!("{n}")),
        LuaValue::Boolean(b) => Some(b.to_string()),
        LuaValue::Nil => None,
        other => Some(format!("{:?}", other)),
    }
}

fn find_rooted_file(lua: &Lua, relative: &str) -> Option<std::path::PathBuf> {
    let rel = relative.replace('\\', "/");
    if rel.starts_with('/') || rel.contains("../") || rel == ".." {
        return None;
    }
    let roots: LuaTable = lua.globals().get("__image_roots").ok()?;
    let len = roots.len().ok()?;
    for i in 1..=len {
        let root: String = roots.get(i).ok()?;
        let root = std::path::PathBuf::from(root);
        for candidate in [
            root.join(&rel),
            root.join(rel.trim_start_matches("assets/")),
            root.join("assets").join(&rel),
        ] {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn find_rooted_dir(lua: &Lua, relative: &str) -> Option<std::path::PathBuf> {
    let rel = relative.replace('\\', "/");
    if rel.starts_with('/') || rel.contains("../") || rel == ".." {
        let path = std::path::PathBuf::from(relative);
        return path.is_dir().then_some(path);
    }
    let roots: LuaTable = lua.globals().get("__image_roots").ok()?;
    let len = roots.len().ok()?;
    for i in 1..=len {
        let root: String = roots.get(i).ok()?;
        let root = std::path::PathBuf::from(root);
        for candidate in [
            root.join(&rel),
            root.join(rel.trim_start_matches("assets/")),
            root.join("assets").join(&rel),
        ] {
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

fn find_mod_settings_file(lua: &Lua, mod_name: &str) -> Option<std::path::PathBuf> {
    let rels = if mod_name.trim().is_empty() {
        vec!["data/settings.json".to_string()]
    } else {
        vec![
            "data/settings.json".to_string(),
            format!("{mod_name}/data/settings.json"),
            format!("mods/{mod_name}/data/settings.json"),
        ]
    };
    for rel in &rels {
        if let Some(path) = find_rooted_file(lua, rel) {
            return Some(path);
        }
    }

    let roots: LuaTable = lua.globals().get("__image_roots").ok()?;
    let len = roots.len().ok()?;
    for i in 1..=len {
        let root: String = roots.get(i).ok()?;
        let mut roots_to_try = vec![std::path::PathBuf::from(&root)];
        if roots_to_try[0].file_name().and_then(|name| name.to_str()) == Some("assets") {
            let mut mod_root = roots_to_try[0].clone();
            mod_root.pop();
            roots_to_try.push(mod_root);
        }
        for root in roots_to_try {
            let candidates = if mod_name.trim().is_empty() {
                vec![root.join("data/settings.json")]
            } else {
                vec![
                    root.join("data/settings.json"),
                    root.join(mod_name).join("data/settings.json"),
                    root.join("mods").join(mod_name).join("data/settings.json"),
                ]
            };
            for candidate in candidates {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn writable_lua_file(lua: &Lua, relative: &str, absolute: bool) -> Option<std::path::PathBuf> {
    let rel = relative.replace('\\', "/");
    if absolute {
        return Some(std::path::PathBuf::from(rel));
    }
    if rel.starts_with('/') || rel.contains("../") || rel == ".." {
        return None;
    }
    let roots: LuaTable = lua.globals().get("__image_roots").ok()?;
    let root: String = roots.get(1).ok()?;
    let mut root = std::path::PathBuf::from(root);
    if root.file_name().and_then(|n| n.to_str()) == Some("assets") {
        root.pop();
    }
    Some(root.join(rel))
}

fn lua_call_args_to_vec(value: Option<LuaValue>) -> LuaResult<Vec<LuaValue>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    match value {
        LuaValue::Nil => Ok(Vec::new()),
        LuaValue::Table(tbl) => {
            let len = tbl.len()?;
            let mut args = Vec::with_capacity(len as usize);
            for i in 1..=len {
                args.push(tbl.get::<LuaValue>(i)?);
            }
            Ok(args)
        }
        value => Ok(vec![value]),
    }
}

fn parse_mod_setting(lua: &Lua, contents: &str, save_tag: &str) -> LuaResult<LuaValue> {
    for block in json_object_blocks(contents) {
        let Some(save) = json_string_field(block, "save") else {
            continue;
        };
        if save != save_tag {
            continue;
        }

        let setting_type = json_string_field(block, "type").unwrap_or_default();
        if matches!(setting_type.as_str(), "key" | "keybind") {
            let table = lua.create_table()?;
            table.set(
                "keyboard",
                json_string_field(block, "keyboard").unwrap_or_else(|| "NONE".to_string()),
            )?;
            table.set(
                "gamepad",
                json_string_field(block, "gamepad").unwrap_or_else(|| "NONE".to_string()),
            )?;
            return Ok(LuaValue::Table(table));
        }

        if let Some(value) = json_field_value(block, "value") {
            return parse_json_scalar_or_array(lua, value);
        }
    }
    Ok(LuaNil)
}

fn json_object_blocks(contents: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut start = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in contents.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(idx);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(start) = start.take() {
                        blocks.push(&contents[start..=idx]);
                    }
                }
            }
            _ => {}
        }
    }
    blocks
}

fn json_string_field(block: &str, key: &str) -> Option<String> {
    let value = json_field_value(block, key)?.trim_start();
    if !value.starts_with('"') {
        return None;
    }
    parse_json_string(value).map(|(value, _)| value)
}

fn json_field_value<'a>(block: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let key_pos = block.find(&needle)?;
    let after_key = &block[key_pos + needle.len()..];
    let colon_pos = after_key.find(':')?;
    let value_start = key_pos + needle.len() + colon_pos + 1;
    let tail = &block[value_start..];
    Some(tail[..json_value_len(tail)].trim())
}

fn json_value_len(value: &str) -> usize {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in value.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '[' | '{' => depth += 1,
            ']' | '}' => {
                if depth == 0 {
                    return idx;
                }
                depth -= 1;
            }
            ',' if depth == 0 => return idx,
            _ => {}
        }
    }
    value.len()
}

fn parse_json_scalar_or_array(lua: &Lua, value: &str) -> LuaResult<LuaValue> {
    let value = value.trim();
    if value.starts_with('"') {
        return match parse_json_string(value) {
            Some((value, _)) => Ok(LuaValue::String(lua.create_string(&value)?)),
            None => Ok(LuaNil),
        };
    }
    if value.eq_ignore_ascii_case("true") {
        return Ok(LuaValue::Boolean(true));
    }
    if value.eq_ignore_ascii_case("false") {
        return Ok(LuaValue::Boolean(false));
    }
    if value.eq_ignore_ascii_case("null") {
        return Ok(LuaNil);
    }
    if value.starts_with('[') && value.ends_with(']') {
        let table = lua.create_table()?;
        for (idx, item) in split_json_array(&value[1..value.len() - 1])
            .iter()
            .enumerate()
        {
            table.set(idx + 1, parse_json_scalar_or_array(lua, item)?)?;
        }
        return Ok(LuaValue::Table(table));
    }
    if let Ok(int) = value.parse::<i64>() {
        return Ok(LuaValue::Integer(int));
    }
    if let Ok(float) = value.parse::<f64>() {
        return Ok(LuaValue::Number(float));
    }
    Ok(LuaNil)
}

fn parse_json_string(value: &str) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut escaped = false;
    let mut chars = value.char_indices();
    if chars.next()?.1 != '"' {
        return None;
    }
    for (idx, ch) in chars {
        if escaped {
            let resolved = match ch {
                '"' => '"',
                '\\' => '\\',
                '/' => '/',
                'b' => '\u{0008}',
                'f' => '\u{000c}',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                _ => ch,
            };
            out.push(resolved);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some((out, idx + ch.len_utf8()));
        } else {
            out.push(ch);
        }
    }
    None
}

fn split_json_array(value: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in value.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '[' | '{' => depth += 1,
            ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(value[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    if start < value.len() {
        out.push(value[start..].trim());
    }
    out
}

fn queue_script_call(
    lua: &Lua,
    target: Option<&str>,
    function: &str,
    args: Vec<LuaValue>,
) -> LuaResult<()> {
    let pending: LuaTable = lua.globals().get("__pending_props")?;
    let entry = lua.create_table()?;
    entry.set(
        "type",
        if target.is_some() {
            "call_script"
        } else {
            "call_luas"
        },
    )?;
    if let Some(target) = target {
        entry.set("target", target)?;
    }
    entry.set("function", function)?;
    let args_table = lua.create_table()?;
    for (i, arg) in args.into_iter().enumerate() {
        args_table.set(i + 1, arg)?;
    }
    entry.set("args", args_table)?;
    let len = pending.len()? + 1;
    pending.set(len, entry)?;
    Ok(())
}

fn queue_song_control(
    lua: &Lua,
    action: &str,
    fields: Option<Vec<(&str, LuaValue)>>,
) -> LuaResult<()> {
    let pending: LuaTable = lua.globals().get("__pending_props")?;
    let entry = lua.create_table()?;
    entry.set("type", "song_control")?;
    entry.set("action", action)?;
    if let Some(fields) = fields {
        for (key, value) in fields {
            entry.set(key, value)?;
        }
    }
    let len = pending.len()? + 1;
    pending.set(len, entry)?;
    Ok(())
}

fn queue_precache_request(
    lua: &Lua,
    kind: &str,
    name: &str,
    fields: &[(&str, LuaValue)],
) -> LuaResult<bool> {
    if name.is_empty() {
        return Ok(false);
    }
    let pending: LuaTable = lua.globals().get("__pending_props")?;
    let entry = lua.create_table()?;
    entry.set("type", "precache")?;
    entry.set("kind", kind)?;
    entry.set("name", name)?;
    for (key, value) in fields {
        entry.set(*key, value.clone())?;
    }
    let len = pending.len()? + 1;
    pending.set(len, entry)?;
    Ok(true)
}

fn queue_script_global(lua: &Lua, name: &str, value: LuaValue) -> LuaResult<()> {
    let pending: LuaTable = lua.globals().get("__pending_props")?;
    let entry = lua.create_table()?;
    entry.set("type", "set_global")?;
    entry.set("name", name)?;
    entry.set("value", value)?;
    let len = pending.len()? + 1;
    pending.set(len, entry)?;
    Ok(())
}

fn lua_script_target_matches(script: &str, target: &str) -> bool {
    fn normalize(value: &str) -> String {
        value
            .replace('\\', "/")
            .trim_start_matches("./")
            .trim_end_matches(".lua")
            .trim_end_matches(".hx")
            .to_ascii_lowercase()
    }

    let script = normalize(script);
    let target = normalize(target);
    !target.is_empty() && (script == target || script.ends_with(&format!("/{target}")))
}

fn parse_haxe_call_on_luas(lua: &Lua, code: &str) -> LuaResult<Option<(String, Vec<LuaValue>)>> {
    let Some(call_start) = code.find("callOnLuas(") else {
        return Ok(None);
    };
    let args_start = call_start + "callOnLuas(".len();
    let call = &code[args_start..];
    let Some(func_quote) = call.find(['\'', '"']) else {
        return Ok(None);
    };
    let quote = call.as_bytes()[func_quote] as char;
    let func_rest = &call[func_quote + 1..];
    let Some(func_end) = func_rest.find(quote) else {
        return Ok(None);
    };
    let function = func_rest[..func_end].to_string();

    let after_func = &func_rest[func_end + 1..];
    let Some(bracket_start) = after_func.find('[') else {
        return Ok(None);
    };
    let args_rest = &after_func[bracket_start + 1..];
    let Some(bracket_end) = args_rest.find(']') else {
        return Ok(None);
    };
    let args = parse_haxe_array_args(lua, &args_rest[..bracket_end])?;
    Ok(Some((function, args)))
}

fn parse_haxe_array_args(lua: &Lua, args: &str) -> LuaResult<Vec<LuaValue>> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in args.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => {
                quote = None;
                current.push(ch);
            }
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => {
                quote = Some(ch);
                current.push(ch);
            }
            (None, ',') => {
                values.push(parse_haxe_arg(lua, current.trim())?);
                current.clear();
            }
            (None, c) => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        values.push(parse_haxe_arg(lua, current.trim())?);
    }
    Ok(values)
}

fn parse_haxe_arg(lua: &Lua, arg: &str) -> LuaResult<LuaValue> {
    let arg = arg.trim();
    if arg.is_empty() || arg.eq_ignore_ascii_case("null") {
        return Ok(LuaValue::Nil);
    }
    if let Some(inner) = arg.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Ok(LuaValue::String(lua.create_string(inner)?));
    }
    if let Some(inner) = arg.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Ok(LuaValue::String(lua.create_string(inner)?));
    }
    if arg.eq_ignore_ascii_case("true") {
        return Ok(LuaValue::Boolean(true));
    }
    if arg.eq_ignore_ascii_case("false") {
        return Ok(LuaValue::Boolean(false));
    }
    if let Ok(value) = arg.parse::<i64>() {
        return Ok(LuaValue::Integer(value));
    }
    if let Ok(value) = arg.parse::<f64>() {
        return Ok(LuaValue::Number(value));
    }
    Ok(LuaValue::String(lua.create_string(arg)?))
}

fn register_shader_scalar(lua: &Lua, kind: &str) -> LuaResult<()> {
    let set_name = format!("setShader{kind}");
    let is_int = kind == "Int";
    lua.globals().set(
        set_name,
        lua.create_function(|lua, (target, field, value): (String, String, LuaValue)| {
            let shaders: LuaTable = lua.globals().get("__shader_data")?;
            shaders.set(shader_key(&target, &field), value)?;
            Ok(())
        })?,
    )?;

    let get_name = format!("getShader{kind}");
    lua.globals().set(
        get_name,
        lua.create_function(
            move |lua, (target, field): (String, String)| -> LuaResult<LuaValue> {
                let shaders: LuaTable = lua.globals().get("__shader_data")?;
                if let Ok(value) = shaders.get::<LuaValue>(shader_key(&target, &field)) {
                    if value != LuaValue::Nil {
                        return Ok(value);
                    }
                }
                if is_int {
                    Ok(LuaValue::Integer(0))
                } else {
                    Ok(LuaValue::Number(0.0))
                }
            },
        )?,
    )?;
    Ok(())
}

fn register_shader_bool(lua: &Lua) -> LuaResult<()> {
    lua.globals().set(
        "setShaderBool",
        lua.create_function(|lua, (target, field, value): (String, String, bool)| {
            let shaders: LuaTable = lua.globals().get("__shader_data")?;
            shaders.set(shader_key(&target, &field), value)?;
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "getShaderBool",
        lua.create_function(
            |lua, (target, field): (String, String)| -> LuaResult<bool> {
                let shaders: LuaTable = lua.globals().get("__shader_data")?;
                Ok(shaders
                    .get::<bool>(shader_key(&target, &field))
                    .unwrap_or(false))
            },
        )?,
    )?;
    Ok(())
}

fn shader_key(target: &str, field: &str) -> String {
    format!("{target}.{field}")
}

fn color_name_or_hex_to_int(value: &str) -> i64 {
    let upper = value
        .trim()
        .trim_start_matches("FlxColor.")
        .to_ascii_uppercase();
    let hex = match upper.as_str() {
        "BLACK" => "000000",
        "BLUE" => "0000FF",
        "BROWN" => "8B4513",
        "CYAN" => "00FFFF",
        "GRAY" | "GREY" => "808080",
        "GREEN" => "008000",
        "LIME" => "00FF00",
        "MAGENTA" | "PINK" => "FF00FF",
        "ORANGE" => "FFA500",
        "PURPLE" => "800080",
        "RED" => "FF0000",
        "TRANSPARENT" => "00000000",
        "WHITE" => "FFFFFF",
        "YELLOW" => "FFFF00",
        _ => value,
    };
    let hex = hex
        .trim()
        .trim_start_matches('#')
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    i64::from_str_radix(hex, 16).unwrap_or(0xFFFFFF)
}

fn register_keyboard_query(lua: &Lua, name: &str, table_name: &'static str) -> LuaResult<()> {
    lua.globals().set(
        name,
        lua.create_function(move |lua, key: String| -> LuaResult<bool> {
            let tbl: LuaTable = lua.globals().get(table_name)?;
            Ok(input_table_contains(&tbl, &key))
        })?,
    )
}

fn input_table_contains(tbl: &LuaTable, key: &str) -> bool {
    let key = key.trim();
    if key.is_empty() {
        return false;
    }
    let upper = key.to_ascii_uppercase();
    let variants = [
        key.to_string(),
        upper.clone(),
        format!("KEY{upper}"),
        format!("DIGIT{upper}"),
    ];
    variants
        .iter()
        .any(|variant| tbl.get::<bool>(variant.as_str()).unwrap_or(false))
}

fn update_lua_sprite_dimensions(lua: &Lua, tbl: &LuaTable, image: &str) -> LuaResult<()> {
    if let Some(path) = resolve_image_path(lua, image) {
        if let Some((w, h)) = read_png_dimensions(&path) {
            tbl.set("tex_w", w as f64)?;
            tbl.set("tex_h", h as f64)?;
        }
    }
    Ok(())
}

fn queue_lua_sprite_reload(lua: &Lua, tag: &str, in_front: bool) -> LuaResult<()> {
    let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
    let tbl: LuaTable = sprite_data.get(tag)?;

    let pending_sprites: LuaTable = lua.globals().get("__pending_sprites")?;
    let len = pending_sprites.len()? as i64;
    pending_sprites.set(len + 1, tbl)?;

    let pending_adds: LuaTable = lua.globals().get("__pending_adds")?;
    let add_tbl = lua.create_table()?;
    add_tbl.set("tag", tag)?;
    add_tbl.set("in_front", in_front)?;
    let len = pending_adds.len()? as i64;
    pending_adds.set(len + 1, add_tbl)?;
    Ok(())
}

fn text_table(lua: &Lua, tag: &str) -> Option<LuaTable> {
    lua.globals()
        .get::<LuaTable>("__text_data")
        .ok()?
        .get::<LuaTable>(tag)
        .ok()
}

fn object_rect(lua: &Lua, tag: &str) -> Option<(f64, f64, f64, f64)> {
    if let Ok(sprite_data) = lua.globals().get::<LuaTable>("__sprite_data") {
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let x: f64 = tbl.get("x").unwrap_or(0.0);
            let y: f64 = tbl.get("y").unwrap_or(0.0);
            let w: f64 = tbl
                .get("tex_w")
                .unwrap_or_else(|_| tbl.get("width").unwrap_or(0.0));
            let h: f64 = tbl
                .get("tex_h")
                .unwrap_or_else(|_| tbl.get("height").unwrap_or(0.0));
            let sx: f64 = tbl.get("scale_x").unwrap_or(1.0);
            let sy: f64 = tbl.get("scale_y").unwrap_or(1.0);
            return Some((x, y, w * sx.abs(), h * sy.abs()));
        }
    }

    if let Some(tbl) = text_table(lua, tag) {
        let x: f64 = tbl.get("x").unwrap_or(0.0);
        let y: f64 = tbl.get("y").unwrap_or(0.0);
        let width: f64 = tbl.get("width").unwrap_or(0.0);
        let size: f64 = tbl.get("size").unwrap_or(16.0);
        let text: String = tbl.get("text").unwrap_or_default();
        let w = if width > 0.0 {
            width
        } else {
            text.chars().count() as f64 * size * 0.6
        };
        return Some((x, y, w, size));
    }

    let globals = lua.globals();
    match tag {
        "dad" | "opponent" => Some((
            globals.get("__dad_x").unwrap_or(0.0),
            globals.get("__dad_y").unwrap_or(0.0),
            150.0,
            300.0,
        )),
        "boyfriend" | "bf" => Some((
            globals.get("__bf_x").unwrap_or(0.0),
            globals.get("__bf_y").unwrap_or(0.0),
            150.0,
            300.0,
        )),
        "gf" | "girlfriend" => Some((
            globals.get("__gf_x").unwrap_or(0.0),
            globals.get("__gf_y").unwrap_or(0.0),
            150.0,
            300.0,
        )),
        _ => None,
    }
}

fn rects_overlap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    ax < bx + bw && ax + aw > bx && ay < by + bh && ay + ah > by
}

fn register_shader_array(lua: &Lua, kind: &str, default: LuaValue) -> LuaResult<()> {
    let set_name = format!("setShader{kind}Array");
    lua.globals().set(
        set_name,
        lua.create_function(|lua, (target, field, values): (String, String, LuaValue)| {
            let shaders: LuaTable = lua.globals().get("__shader_data")?;
            shaders.set(shader_key(&target, &field), values)?;
            Ok(true)
        })?,
    )?;

    let get_name = format!("getShader{kind}Array");
    lua.globals().set(
        get_name,
        lua.create_function(move |lua, (target, field): (String, String)| {
            let shaders: LuaTable = lua.globals().get("__shader_data")?;
            if let Ok(LuaValue::Table(tbl)) = shaders.get::<LuaValue>(shader_key(&target, &field)) {
                return Ok(tbl);
            }
            let tbl = lua.create_table()?;
            tbl.set(1, default.clone())?;
            Ok(tbl)
        })?,
    )?;
    Ok(())
}

fn register_misc_default_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    globals.set(
        "getColorFromString",
        lua.create_function(|_lua, value: String| -> LuaResult<i64> {
            Ok(color_name_or_hex_to_int(&value))
        })?,
    )?;
    globals.set(
        "getColorFromName",
        lua.create_function(|_lua, value: String| -> LuaResult<i64> {
            Ok(color_name_or_hex_to_int(&value))
        })?,
    )?;
    globals.set(
        "getTextFont",
        lua.create_function(|lua, tag: String| -> LuaResult<String> {
            Ok(text_table(lua, &tag)
                .and_then(|tbl| tbl.get::<String>("font").ok())
                .unwrap_or_default())
        })?,
    )?;
    globals.set(
        "getTextSize",
        lua.create_function(|lua, tag: String| -> LuaResult<f64> {
            Ok(text_table(lua, &tag)
                .and_then(|tbl| tbl.get::<f64>("size").ok())
                .unwrap_or(16.0))
        })?,
    )?;
    globals.set(
        "getTextWidth",
        lua.create_function(|lua, tag: String| -> LuaResult<f64> {
            if let Some(tbl) = text_table(lua, &tag) {
                let width: f64 = tbl.get("width").unwrap_or(0.0);
                if width > 0.0 {
                    return Ok(width);
                }
                let text: String = tbl.get("text").unwrap_or_default();
                let size: f64 = tbl.get("size").unwrap_or(16.0);
                return Ok(text.chars().count() as f64 * size * 0.6);
            }
            Ok(0.0)
        })?,
    )?;
    globals.set(
        "setTextHeight",
        lua.create_function(|lua, (tag, height): (String, f64)| {
            if let Some(tbl) = text_table(lua, &tag) {
                tbl.set("height", height)?;
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "setTextItalic",
        lua.create_function(|lua, (tag, italic): (String, bool)| {
            if let Some(tbl) = text_table(lua, &tag) {
                tbl.set("italic", italic)?;
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "getScreenPositionX",
        lua.create_function(
            |lua, (tag, _camera): (String, Option<String>)| -> LuaResult<f64> {
                Ok(object_rect(lua, &tag).map(|rect| rect.0).unwrap_or(0.0))
            },
        )?,
    )?;
    globals.set(
        "getScreenPositionY",
        lua.create_function(
            |lua, (tag, _camera): (String, Option<String>)| -> LuaResult<f64> {
                Ok(object_rect(lua, &tag).map(|rect| rect.1).unwrap_or(0.0))
            },
        )?,
    )?;
    globals.set(
        "objectsOverlap",
        lua.create_function(|lua, (a, b): (String, String)| -> LuaResult<bool> {
            let Some(a) = object_rect(lua, &a) else {
                return Ok(false);
            };
            let Some(b) = object_rect(lua, &b) else {
                return Ok(false);
            };
            Ok(rects_overlap(a, b))
        })?,
    )?;
    globals.set(
        "getPixelColor",
        lua.create_function(
            |_lua, (_obj, _x, _y): (String, i32, i32)| -> LuaResult<i64> { Ok(0) },
        )?,
    )?;
    Ok(())
}

fn register_control_default_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    register_keyboard_query(lua, "keyboardJustPressed", "__input_just_pressed")?;
    register_keyboard_query(lua, "keyboardPressed", "__input_pressed")?;
    register_keyboard_query(lua, "keyboardReleased", "__input_just_released")?;

    for (name, key) in [
        ("mouseClicked", "__mouse_just_pressed"),
        ("mousePressed", "__mouse_pressed"),
        ("mouseReleased", "__mouse_just_released"),
    ] {
        globals.set(
            name,
            lua.create_function(move |lua, _args: LuaMultiValue| -> LuaResult<bool> {
                Ok(lua.globals().get::<bool>(key).unwrap_or(false))
            })?,
        )?;
    }
    for name in [
        "anyGamepadJustPressed",
        "anyGamepadPressed",
        "anyGamepadReleased",
        "gamepadJustPressed",
        "gamepadPressed",
        "gamepadReleased",
    ] {
        globals.set(
            name,
            lua.create_function(|_lua, _args: LuaMultiValue| -> LuaResult<bool> { Ok(false) })?,
        )?;
    }
    globals.set(
        "gamepadAnalogX",
        lua.create_function(|_lua, _args: LuaMultiValue| -> LuaResult<f64> { Ok(0.0) })?,
    )?;
    globals.set(
        "gamepadAnalogY",
        lua.create_function(|_lua, _args: LuaMultiValue| -> LuaResult<f64> { Ok(0.0) })?,
    )?;
    globals.set(
        "getMouseX",
        lua.create_function(|lua, _camera: Option<String>| -> LuaResult<i32> {
            Ok(lua.globals().get::<f64>("__mouse_x").unwrap_or(0.0) as i32)
        })?,
    )?;
    globals.set(
        "getMouseY",
        lua.create_function(|lua, _camera: Option<String>| -> LuaResult<i32> {
            Ok(lua.globals().get::<f64>("__mouse_y").unwrap_or(0.0) as i32)
        })?,
    )?;

    for (name, key) in [
        ("getCameraScrollX", "scroll_x"),
        ("getCameraScrollY", "scroll_y"),
        ("getCameraFollowX", "follow_x"),
        ("getCameraFollowY", "follow_y"),
    ] {
        globals.set(
            name,
            lua.create_function(move |lua, ()| -> LuaResult<f64> {
                let camera: LuaTable = lua.globals().get("__camera_values")?;
                Ok(camera.get::<f64>(key).unwrap_or(0.0))
            })?,
        )?;
    }
    globals.set(
        "setCameraScroll",
        lua.create_function(|lua, (x, y): (Option<f64>, Option<f64>)| {
            let camera: LuaTable = lua.globals().get("__camera_values")?;
            camera.set("scroll_x", x.unwrap_or(0.0))?;
            camera.set("scroll_y", y.unwrap_or(0.0))?;
            Ok(())
        })?,
    )?;
    globals.set(
        "addCameraScroll",
        lua.create_function(|lua, (x, y): (Option<f64>, Option<f64>)| {
            let camera: LuaTable = lua.globals().get("__camera_values")?;
            let nx = camera.get::<f64>("scroll_x").unwrap_or(0.0) + x.unwrap_or(0.0);
            let ny = camera.get::<f64>("scroll_y").unwrap_or(0.0) + y.unwrap_or(0.0);
            camera.set("scroll_x", nx)?;
            camera.set("scroll_y", ny)?;
            Ok(())
        })?,
    )?;
    globals.set(
        "setCameraFollowPoint",
        lua.create_function(|lua, (x, y): (Option<f64>, Option<f64>)| {
            let camera: LuaTable = lua.globals().get("__camera_values")?;
            camera.set("follow_x", x.unwrap_or(0.0))?;
            camera.set("follow_y", y.unwrap_or(0.0))?;
            Ok(())
        })?,
    )?;
    globals.set(
        "addCameraFollowPoint",
        lua.create_function(|lua, (x, y): (Option<f64>, Option<f64>)| {
            let camera: LuaTable = lua.globals().get("__camera_values")?;
            let nx = camera.get::<f64>("follow_x").unwrap_or(0.0) + x.unwrap_or(0.0);
            let ny = camera.get::<f64>("follow_y").unwrap_or(0.0) + y.unwrap_or(0.0);
            camera.set("follow_x", nx)?;
            camera.set("follow_y", ny)?;
            Ok(())
        })?,
    )?;

    for (name, action) in [
        ("startCountdown", "start_countdown"),
        ("endSong", "end_song"),
        ("exitSong", "exit_song"),
        ("restartSong", "restart_song"),
    ] {
        globals.set(
            name,
            lua.create_function(move |lua, _args: LuaMultiValue| -> LuaResult<bool> {
                queue_song_control(lua, action, None)?;
                Ok(true)
            })?,
        )?;
    }
    globals.set(
        "startDialogue",
        lua.create_function(
            |lua, (dialogue, music): (String, Option<String>)| -> LuaResult<bool> {
                let mut fields = Vec::new();
                fields.push(("dialogue", LuaValue::String(lua.create_string(&dialogue)?)));
                if let Some(music) = music {
                    fields.push(("music", LuaValue::String(lua.create_string(&music)?)));
                }
                queue_song_control(lua, "start_dialogue", Some(fields))?;
                Ok(true)
            },
        )?,
    )?;
    globals.set(
        "openCustomSubstate",
        lua.create_function(
            |lua, (name, pause_game): (String, Option<bool>)| -> LuaResult<bool> {
                queue_song_control(
                    lua,
                    "open_substate",
                    Some(vec![
                        ("name", LuaValue::String(lua.create_string(&name)?)),
                        ("pause_game", LuaValue::Boolean(pause_game.unwrap_or(true))),
                    ]),
                )?;
                Ok(true)
            },
        )?,
    )?;
    globals.set(
        "closeCustomSubstate",
        lua.create_function(|lua, _args: LuaMultiValue| -> LuaResult<bool> {
            queue_song_control(lua, "close_substate", None)?;
            Ok(true)
        })?,
    )?;
    globals.set(
        "insertToCustomSubstate",
        lua.create_function(|_lua, _args: LuaMultiValue| -> LuaResult<bool> { Ok(true) })?,
    )?;
    globals.set(
        "precacheImage",
        lua.create_function(|lua, (name, allow_gpu): (String, Option<bool>)| {
            queue_precache_request(
                lua,
                "image",
                &name,
                &[("allow_gpu", LuaValue::Boolean(allow_gpu.unwrap_or(true)))],
            )
        })?,
    )?;
    globals.set(
        "precacheSound",
        lua.create_function(|lua, name: String| queue_precache_request(lua, "sound", &name, &[]))?,
    )?;
    globals.set(
        "precacheMusic",
        lua.create_function(|lua, name: String| queue_precache_request(lua, "music", &name, &[]))?,
    )?;
    globals.set(
        "addCharacterToList",
        lua.create_function(|lua, (name, character_type): (String, Option<String>)| {
            queue_precache_request(
                lua,
                "character",
                &name,
                &[(
                    "character_type",
                    LuaValue::String(
                        lua.create_string(character_type.as_deref().unwrap_or("dad"))?,
                    ),
                )],
            )
        })?,
    )?;
    globals.set(
        "loadSong",
        lua.create_function(|lua, (song, difficulty): (String, Option<i32>)| {
            let mut fields = Vec::new();
            fields.push(("song", LuaValue::String(lua.create_string(&song)?)));
            if let Some(difficulty) = difficulty {
                fields.push(("difficulty", LuaValue::Integer(difficulty as i64)));
            }
            queue_song_control(lua, "load_song", Some(fields))?;
            Ok(true)
        })?,
    )?;
    Ok(())
}

fn register_reflection_default_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    globals.set(
        "createInstance",
        lua.create_function(
            |lua, (name, class_name, args): (String, String, Option<LuaTable>)| {
                let instances: LuaTable = lua.globals().get("__instances")?;
                let tbl = lua.create_table()?;
                tbl.set("class", class_name.as_str())?;
                if let Some(args) = args {
                    if class_name.ends_with("objects.Character") || class_name == "Character" {
                        let x: f64 = args.get(1).unwrap_or(0.0);
                        let y: f64 = args.get(2).unwrap_or(0.0);
                        let character: String = args.get(3).unwrap_or_default();
                        let is_player: bool = args.get(4).unwrap_or(false);

                        let characters: LuaTable = lua.globals().get("__character_instances")?;
                        let char_tbl = lua.create_table()?;
                        char_tbl.set("tag", name.as_str())?;
                        char_tbl.set("class", class_name.as_str())?;
                        char_tbl.set("character", character.as_str())?;
                        char_tbl.set("x", x)?;
                        char_tbl.set("y", y)?;
                        char_tbl.set("scale_x", 1.0)?;
                        char_tbl.set("scale_y", 1.0)?;
                        char_tbl.set("alpha", 1.0)?;
                        char_tbl.set("visible", true)?;
                        char_tbl.set("is_player", is_player)?;
                        char_tbl.set("current_anim", "")?;
                        char_tbl.set("anim_finished", false)?;
                        characters.set(name.as_str(), char_tbl)?;

                        let pending: LuaTable =
                            lua.globals().get("__pending_character_instances")?;
                        let create_tbl = lua.create_table()?;
                        create_tbl.set("tag", name.as_str())?;
                        create_tbl.set("character", character)?;
                        create_tbl.set("x", x)?;
                        create_tbl.set("y", y)?;
                        create_tbl.set("is_player", is_player)?;
                        pending.set(pending.len()? + 1, create_tbl)?;
                    }
                    tbl.set("args", args)?;
                }
                instances.set(name, tbl)?;
                Ok(true)
            },
        )?,
    )?;
    globals.set(
        "addInstance",
        lua.create_function(
            |lua, (name, in_front): (String, Option<bool>)| -> LuaResult<bool> {
                if text_table(lua, &name).is_some() {
                    let func: LuaFunction = lua.globals().get("addLuaText")?;
                    func.call::<()>((name, in_front))?;
                    return Ok(true);
                }
                if lua_sprite_or_text_exists(lua, &name) {
                    let func: LuaFunction = lua.globals().get("addLuaSprite")?;
                    func.call::<()>((name, in_front))?;
                    return Ok(true);
                }
                let character_instances: LuaTable = lua.globals().get("__character_instances")?;
                if character_instances.get::<LuaValue>(name.as_str())? != LuaNil {
                    let pending: LuaTable = lua.globals().get("__pending_character_adds")?;
                    let add_tbl = lua.create_table()?;
                    add_tbl.set("tag", name)?;
                    add_tbl.set("in_front", in_front.unwrap_or(true))?;
                    pending.set(pending.len()? + 1, add_tbl)?;
                    return Ok(true);
                }
                let instances: LuaTable = lua.globals().get("__instances")?;
                Ok(instances.get::<LuaValue>(name).unwrap_or(LuaNil) != LuaNil)
            },
        )?,
    )?;
    globals.set(
        "instanceArg",
        lua.create_function(|lua, (name, class_name): (String, Option<String>)| {
            let tbl = lua.create_table()?;
            tbl.set("instance", name)?;
            if let Some(class_name) = class_name {
                tbl.set("class", class_name)?;
            }
            Ok(tbl)
        })?,
    )?;
    globals.set(
        "callMethod",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<LuaValue> {
            dispatch_reflection_method(lua, args)
        })?,
    )?;
    globals.set(
        "callMethodFromClass",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<LuaValue> {
            dispatch_reflection_class_method(lua, args)
        })?,
    )?;
    globals.set(
        "addToGroup",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<bool> {
            let object = multi_string(&args, 1).or_else(|| multi_string(&args, 0));
            let Some(object) = object else {
                return Ok(false);
            };
            if lua_sprite_or_text_exists(lua, &object) {
                if text_table(lua, &object).is_some() {
                    let func: LuaFunction = lua.globals().get("addLuaText")?;
                    func.call::<()>((object, true))?;
                } else {
                    let func: LuaFunction = lua.globals().get("addLuaSprite")?;
                    func.call::<()>((object, true))?;
                }
                return Ok(true);
            }
            Ok(false)
        })?,
    )?;
    globals.set(
        "removeFromGroup",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<bool> {
            let object = multi_string(&args, 2).or_else(|| multi_string(&args, 1));
            let Some(object) = object else {
                return Ok(false);
            };
            if lua_sprite_or_text_exists(lua, &object) {
                let func: LuaFunction = lua.globals().get("removeLuaSprite")?;
                func.call::<()>((object, false))?;
                return Ok(true);
            }
            Ok(false)
        })?,
    )?;
    Ok(())
}

fn dispatch_reflection_method(lua: &Lua, args: LuaMultiValue) -> LuaResult<LuaValue> {
    let method = multi_string(&args, 0).unwrap_or_default();
    let Some((object, member)) = method.rsplit_once('.') else {
        return Ok(LuaNil);
    };
    let call_args = args.get(1).cloned().unwrap_or(LuaValue::Nil);

    match member {
        "snapToTarget" if object == "camGame" || object == "game.camGame" => {
            let g = lua.globals();
            let target = g
                .get::<String>("__last_camera_target")
                .unwrap_or_else(|_| "bf".to_string());
            let pending: LuaTable = g
                .get::<LuaTable>("__pending_cam_targets")
                .unwrap_or_else(|_| lua.create_table().unwrap());
            let len = pending.len().unwrap_or(0);
            pending.set(len + 1, format!("__snap:{target}"))?;
            g.set("__pending_cam_targets", pending)?;
            return Ok(LuaValue::Boolean(true));
        }
        "playAnim" | "playAnimation" | "play" if method.ends_with(".animation.play") => {
            if let Some(anim) = indexed_arg_string(&call_args, 1) {
                let forced = indexed_arg_bool(&call_args, 2).unwrap_or(false);
                let target = object.trim_end_matches(".animation").to_string();
                let func: LuaFunction = lua.globals().get("playAnim")?;
                func.call::<()>((target, anim, forced))?;
                return Ok(LuaValue::Boolean(true));
            }
        }
        "playAnim" | "playAnimation" => {
            if let Some(anim) = indexed_arg_string(&call_args, 1) {
                let forced = indexed_arg_bool(&call_args, 2).unwrap_or(false);
                let func: LuaFunction = lua.globals().get("playAnim")?;
                func.call::<()>((object.to_string(), anim, forced))?;
                return Ok(LuaValue::Boolean(true));
            }
        }
        "setPosition" => {
            if let (Some(x), Some(y)) = (
                indexed_arg_number(&call_args, 1),
                indexed_arg_number(&call_args, 2),
            ) {
                set_lua_property(lua, &format!("{object}.x"), LuaValue::Number(x))?;
                set_lua_property(lua, &format!("{object}.y"), LuaValue::Number(y))?;
                return Ok(LuaValue::Boolean(true));
            }
        }
        "set" if method.ends_with(".scale.set") => {
            if let (Some(x), Some(y)) = (
                indexed_arg_number(&call_args, 1),
                indexed_arg_number(&call_args, 2),
            ) {
                let target = object.trim_end_matches(".scale").to_string();
                let func: LuaFunction = lua.globals().get("scaleObject")?;
                func.call::<()>((target, x, y, true))?;
                return Ok(LuaValue::Boolean(true));
            }
        }
        "set" if method.ends_with(".scrollFactor.set") => {
            if let (Some(x), Some(y)) = (
                indexed_arg_number(&call_args, 1),
                indexed_arg_number(&call_args, 2),
            ) {
                let target = object.trim_end_matches(".scrollFactor").to_string();
                let func: LuaFunction = lua.globals().get("setScrollFactor")?;
                func.call::<()>((target, x, y))?;
                return Ok(LuaValue::Boolean(true));
            }
        }
        "screenCenter" => {
            let pos = indexed_arg_string(&call_args, 1).unwrap_or_else(|| "xy".to_string());
            let func: LuaFunction = lua.globals().get("screenCenter")?;
            func.call::<()>((object.to_string(), pos))?;
            return Ok(LuaValue::Boolean(true));
        }
        "updateHitbox" => {
            let func: LuaFunction = lua.globals().get("updateHitbox")?;
            func.call::<()>(object.to_string())?;
            return Ok(LuaValue::Boolean(true));
        }
        "kill" | "destroy" => {
            if lua_sprite_or_text_exists(lua, object) {
                let func: LuaFunction = lua.globals().get("removeLuaSprite")?;
                func.call::<()>((object.to_string(), true))?;
                return Ok(LuaValue::Boolean(true));
            }
        }
        _ => {}
    }

    Ok(LuaNil)
}

fn dispatch_reflection_class_method(lua: &Lua, args: LuaMultiValue) -> LuaResult<LuaValue> {
    let class_name = multi_string(&args, 0).unwrap_or_default();
    let method = multi_string(&args, 1).unwrap_or_default();
    let call_args = args.get(2).cloned().unwrap_or(LuaValue::Nil);
    let class_lc = class_name.to_ascii_lowercase();
    let method_lc = method.to_ascii_lowercase();

    if class_lc == "flixel.flxg" {
        match method_lc.as_str() {
            "cameras.remove" | "cameras.add" | "cameras.reset" | "cameras.setdefaultdrawtarget" => {
                return Ok(LuaValue::Boolean(true));
            }
            "sound.music.pause" => {
                queue_sound_tag_request(lua, "pause_music", None, &[])?;
                return Ok(LuaValue::Boolean(true));
            }
            "sound.music.resume" | "sound.music.play" => {
                queue_sound_tag_request(lua, "resume_music", None, &[])?;
                return Ok(LuaValue::Boolean(true));
            }
            "sound.music.stop" => {
                queue_sound_tag_request(lua, "stop_music", None, &[])?;
                return Ok(LuaValue::Boolean(true));
            }
            "sound.play" => {
                if let Some(path) = indexed_arg_string(&call_args, 1) {
                    let volume = indexed_arg_number(&call_args, 2).unwrap_or(1.0);
                    let func: LuaFunction = lua.globals().get("playSound")?;
                    func.call::<()>((path, Some(volume), None::<String>, Some(false)))?;
                    return Ok(LuaValue::Boolean(true));
                }
            }
            _ => {}
        }
    }

    if matches!(
        class_lc.as_str(),
        "coolutil" | "backend.coolutil" | "lime.app.application" | "openfl.lib"
    ) {
        return Ok(LuaValue::Boolean(true));
    }

    Ok(LuaNil)
}

fn indexed_arg_string(value: &LuaValue, index: i64) -> Option<String> {
    match value {
        LuaValue::Table(tbl) => tbl.get::<String>(index).ok(),
        LuaValue::String(s) if index == 1 => s.to_str().ok().map(|s| s.to_string()),
        _ => None,
    }
}

fn indexed_arg_number(value: &LuaValue, index: i64) -> Option<f64> {
    match value {
        LuaValue::Table(tbl) => tbl.get::<f64>(index).ok(),
        LuaValue::Number(n) if index == 1 => Some(*n),
        LuaValue::Integer(i) if index == 1 => Some(*i as f64),
        _ => None,
    }
}

fn indexed_arg_bool(value: &LuaValue, index: i64) -> Option<bool> {
    match value {
        LuaValue::Table(tbl) => tbl.get::<bool>(index).ok(),
        LuaValue::Boolean(b) if index == 1 => Some(*b),
        _ => None,
    }
}

fn set_lua_property(lua: &Lua, prop: &str, value: LuaValue) -> LuaResult<()> {
    let func: LuaFunction = lua.globals().get("setProperty")?;
    func.call::<()>((prop.to_string(), value))
}

fn lua_sprite_or_text_exists(lua: &Lua, tag: &str) -> bool {
    let sprite_exists = lua
        .globals()
        .get::<LuaTable>("__sprite_data")
        .ok()
        .and_then(|tbl| tbl.get::<LuaValue>(tag).ok())
        .is_some_and(|value| value != LuaNil);
    sprite_exists || text_table(lua, tag).is_some()
}

fn register_deprecated_aliases(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    globals.set(
        "luaSpriteMakeGraphic",
        lua.create_function(
            |lua, (tag, width, height, color): (String, i32, i32, String)| {
                let func: LuaFunction = lua.globals().get("makeGraphic")?;
                func.call::<()>((tag, width, height, color))
            },
        )?,
    )?;
    globals.set(
        "luaSpriteAddAnimationByPrefix",
        lua.create_function(
            |lua,
             (tag, name, prefix, fps, looping): (
                String,
                String,
                String,
                Option<f64>,
                Option<bool>,
            )| {
                let func: LuaFunction = lua.globals().get("addAnimationByPrefix")?;
                func.call::<()>((tag, name, prefix, fps, looping))
            },
        )?,
    )?;
    globals.set(
        "luaSpriteAddAnimationByIndices",
        lua.create_function(
            |lua, (tag, name, prefix, indices, fps): (String, String, String, LuaValue, Option<f64>)| {
                let func: LuaFunction = lua.globals().get("addAnimationByIndices")?;
                func.call::<()>((tag, name, prefix, indices, fps, Some(false)))
            },
        )?,
    )?;
    globals.set(
        "luaSpritePlayAnimation",
        lua.create_function(|lua, (tag, name, forced): (String, String, Option<bool>)| {
            let func: LuaFunction = lua.globals().get("playAnim")?;
            func.call::<()>((tag, name, forced))
        })?,
    )?;
    globals.set(
        "setLuaSpriteCamera",
        lua.create_function(|lua, (tag, camera): (String, Option<String>)| {
            let func: LuaFunction = lua.globals().get("setObjectCamera")?;
            func.call::<()>((tag, camera))
        })?,
    )?;
    globals.set(
        "setLuaSpriteScrollFactor",
        lua.create_function(|lua, (tag, x, y): (String, f64, f64)| {
            let func: LuaFunction = lua.globals().get("setScrollFactor")?;
            func.call::<()>((tag, x, y))
        })?,
    )?;
    globals.set(
        "scaleLuaSprite",
        lua.create_function(|lua, (tag, x, y): (String, f64, f64)| {
            let func: LuaFunction = lua.globals().get("scaleObject")?;
            func.call::<()>((tag, x, y, Some(true)))
        })?,
    )?;
    globals.set(
        "getPropertyLuaSprite",
        lua.create_function(
            |lua, (tag, prop): (String, String)| -> LuaResult<LuaValue> {
                let func: LuaFunction = lua.globals().get("getProperty")?;
                func.call::<LuaValue>(format!("{tag}.{prop}"))
            },
        )?,
    )?;
    globals.set(
        "setPropertyLuaSprite",
        lua.create_function(|lua, (tag, prop, value): (String, String, LuaValue)| {
            let func: LuaFunction = lua.globals().get("setProperty")?;
            func.call::<()>((format!("{tag}.{prop}"), value))
        })?,
    )?;
    globals.set(
        "musicFadeIn",
        lua.create_function(|lua, args: LuaMultiValue| {
            let duration = multi_number(&args, 0).unwrap_or(1.0);
            let from = multi_number(&args, 1).unwrap_or(0.0);
            let to = multi_number(&args, 2).unwrap_or(1.0);
            queue_sound_tag_request(
                lua,
                "sound_fade",
                None,
                &[
                    ("duration", LuaValue::Number(duration)),
                    ("from", LuaValue::Number(from)),
                    ("to", LuaValue::Number(to)),
                    ("stop_when_done", LuaValue::Boolean(false)),
                ],
            )?;
            lua.globals().set("__music_volume", to)?;
            Ok(())
        })?,
    )?;
    globals.set(
        "musicFadeOut",
        lua.create_function(|lua, args: LuaMultiValue| {
            let duration = multi_number(&args, 0).unwrap_or(1.0);
            let to = multi_number(&args, 1).unwrap_or(0.0);
            queue_sound_tag_request(
                lua,
                "sound_fade",
                None,
                &[
                    ("duration", LuaValue::Number(duration)),
                    ("to", LuaValue::Number(to)),
                    ("stop_when_done", LuaValue::Boolean(true)),
                ],
            )?;
            lua.globals().set("__music_volume", to)?;
            Ok(())
        })?,
    )?;
    globals.set(
        "makeFlxAnimateSprite",
        lua.create_function(
            |lua, (tag, x, y, folder): (String, Option<f64>, Option<f64>, Option<String>)| {
                let func: LuaFunction = lua.globals().get("makeAnimatedLuaSprite")?;
                func.call::<()>((tag, folder, x, y))
            },
        )?,
    )?;
    globals.set(
        "loadAnimateAtlas",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<bool> {
            let tag = multi_string(&args, 0).unwrap_or_default();
            let image = multi_string(&args, 1).unwrap_or_default();
            if tag.is_empty() || image.is_empty() {
                return Ok(false);
            }
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            let tbl: LuaTable = sprite_data.get(tag.as_str())?;
            tbl.set("kind", "animated")?;
            tbl.set("image", image.as_str())?;
            update_lua_sprite_dimensions(lua, &tbl, &image)?;
            queue_lua_sprite_reload(lua, &tag, false)?;
            Ok(true)
        })?,
    )?;
    globals.set(
        "loadFrames",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<bool> {
            let tag = multi_string(&args, 0).unwrap_or_default();
            let image = multi_string(&args, 1).unwrap_or_default();
            if tag.is_empty() || image.is_empty() {
                return Ok(false);
            }
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            let tbl: LuaTable = sprite_data.get(tag.as_str())?;
            tbl.set("kind", "animated")?;
            tbl.set("image", image.as_str())?;
            update_lua_sprite_dimensions(lua, &tbl, &image)?;
            queue_lua_sprite_reload(lua, &tag, false)?;
            Ok(true)
        })?,
    )?;
    globals.set(
        "loadMultipleFrames",
        lua.create_function(|lua, args: LuaMultiValue| -> LuaResult<bool> {
            let tag = multi_string(&args, 0).unwrap_or_default();
            if tag.is_empty() {
                return Ok(false);
            }
            let Some(images) = args.get(1).and_then(|value| match value {
                LuaValue::Table(tbl) => tbl.get::<String>(1).ok(),
                LuaValue::String(s) => s.to_str().ok().map(|s| s.to_string()),
                _ => None,
            }) else {
                return Ok(false);
            };
            let func: LuaFunction = lua.globals().get("loadFrames")?;
            func.call::<bool>((tag, images)).map(|_| true)
        })?,
    )?;
    globals.set(
        "addAnimationBySymbol",
        lua.create_function(
            |lua,
             (tag, name, symbol, fps, looping): (
                String,
                String,
                String,
                Option<f64>,
                Option<bool>,
            )| {
                let func: LuaFunction = lua.globals().get("addAnimationByPrefix")?;
                func.call::<()>((tag, name, symbol, fps, looping))
            },
        )?,
    )?;
    globals.set(
        "addAnimationBySymbolIndices",
        lua.create_function(
            |lua,
             (tag, name, symbol, indices, fps, looping): (
                String,
                String,
                String,
                LuaValue,
                Option<f64>,
                Option<bool>,
            )| {
                let func: LuaFunction = lua.globals().get("addAnimationByIndices")?;
                func.call::<()>((tag, name, symbol, indices, fps, looping))
            },
        )?,
    )?;
    Ok(())
}

/// Parse a hex color string to (r, g, b, a) as 0..1 floats.
fn parse_hex_rgba(hex: &str) -> (f32, f32, f32, f32) {
    let hex = if hex.len() > 6 {
        &hex[hex.len() - 6..]
    } else {
        hex
    };
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0);
        }
    }
    (0.8, 0.8, 0.8, 1.0)
}

fn lua_value_to_color_string(value: &LuaValue) -> String {
    match value {
        LuaValue::String(s) => s.to_string_lossy().to_string(),
        LuaValue::Integer(i) => format!("{:06X}", (*i as i64 & 0xFFFFFF) as u32),
        LuaValue::Number(n) => format!("{:06X}", (*n as i64 & 0xFFFFFF) as u32),
        _ => "FFFFFF".to_string(),
    }
}

fn lua_to_f32(val: &Option<LuaValue>) -> f32 {
    match val {
        Some(LuaValue::Number(n)) => *n as f32,
        Some(LuaValue::Integer(i)) => *i as f32,
        _ => 0.0,
    }
}

/// Linearly remap value from [in_start, in_end] to [out_start, out_end].
/// Equivalent to HaxeFlixel's FlxMath.remapToRange.
fn remap_to_range(value: f64, in_start: f64, in_end: f64, out_start: f64, out_end: f64) -> f64 {
    if (in_end - in_start).abs() < f64::EPSILON {
        return out_start;
    }
    out_start + (value - in_start) * (out_end - out_start) / (in_end - in_start)
}

fn lua_val_to_f32(val: &LuaValue) -> Option<f32> {
    match val {
        LuaValue::Number(n) => Some(*n as f32),
        LuaValue::Integer(n) => Some(*n as f32),
        LuaValue::String(s) => s.to_string_lossy().trim().parse::<f32>().ok(),
        _ => None,
    }
}

fn lua_val_to_i64(val: &LuaValue) -> Option<i64> {
    match val {
        LuaValue::Integer(n) => Some(*n),
        LuaValue::Number(n) => Some(*n as i64),
        LuaValue::String(s) => {
            let s = s.to_string_lossy();
            let trimmed = s.trim();
            trimmed
                .parse::<i64>()
                .ok()
                .or_else(|| trimmed.parse::<f64>().ok().map(|n| n as i64))
        }
        _ => None,
    }
}

fn sprite_animation_value(
    lua: &Lua,
    tbl: &LuaTable,
    key: &str,
    default: LuaValue,
) -> LuaResult<LuaValue> {
    if let Ok(value) = tbl.get::<LuaValue>(format!("animation.{key}")) {
        if !matches!(value, LuaValue::Nil) {
            return Ok(value);
        }
    }
    if let Ok(animation) = tbl.get::<LuaTable>("animation") {
        if let Ok(value) = animation.get::<LuaValue>(key) {
            if !matches!(value, LuaValue::Nil) {
                return Ok(value);
            }
        }
        if let Ok(cur_anim) = animation.get::<LuaTable>("curAnim") {
            if let Ok(value) = cur_anim.get::<LuaValue>(key) {
                if !matches!(value, LuaValue::Nil) {
                    return Ok(value);
                }
            }
        }
    }
    match key {
        "name" => Ok(tbl
            .get::<LuaValue>("current_anim")
            .unwrap_or(LuaValue::String(lua.create_string("")?))),
        "finished" => Ok(tbl.get::<LuaValue>("anim_finished").unwrap_or(default)),
        _ => Ok(default),
    }
}

fn set_sprite_animation_value(
    lua: &Lua,
    tbl: &LuaTable,
    key: &str,
    value: LuaValue,
) -> LuaResult<()> {
    let animation = match tbl.get::<LuaTable>("animation") {
        Ok(t) => t,
        Err(_) => {
            let t = lua.create_table()?;
            tbl.set("animation", t.clone())?;
            t
        }
    };
    let cur_anim = match animation.get::<LuaTable>("curAnim") {
        Ok(t) => t,
        Err(_) => {
            let t = lua.create_table()?;
            animation.set("curAnim", t.clone())?;
            t
        }
    };

    if key == "curFrame" {
        cur_anim.set(key, value)?;
    } else {
        animation.set(key, value.clone())?;
        cur_anim.set(key, value)?;
    }
    Ok(())
}

/// Parse a FlxTween.num(getVar('name'), endVal, duration, {ease: FlxEase.X}, ...) pattern.
/// Returns (var_name, end_value, duration, ease_name).
fn parse_flx_tween_num(code: &str) -> Option<(String, f64, f64, String)> {
    // Extract variable name from getVar('...')
    let gv = code.find("getVar('")?;
    let name_start = gv + "getVar('".len();
    let name_end_rel = code[name_start..].find('\'')?;
    let var_name = code[name_start..name_start + name_end_rel].to_string();

    // Find the closing paren of getVar('...')
    let after_name = name_start + name_end_rel;
    let close_paren = code[after_name..].find(')')?;
    let pos = after_name + close_paren + 1; // past the )

    // Remaining: ,endVal,duration ,{ease: FlxEase.X}, ...
    let remaining = &code[pos..];
    let remaining = remaining.trim_start_matches(|c: char| c == ',' || c.is_whitespace());

    // Parse end value
    let end_str: String = remaining
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let end_val: f64 = end_str.parse().ok()?;

    // Skip past end value, parse duration
    let remaining = &remaining[end_str.len()..];
    let remaining = remaining.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
    let dur_str: String = remaining
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let duration: f64 = dur_str.parse().ok()?;

    // Find ease function name from FlxEase.X
    let ease = if let Some(idx) = code.find("FlxEase.") {
        let start = idx + "FlxEase.".len();
        code[start..]
            .chars()
            .take_while(|c| c.is_alphanumeric())
            .collect::<String>()
    } else {
        "linear".to_string()
    };

    Some((var_name, end_val, duration, ease))
}

/// Parse runHaxeCode blocks that adjust character positions via switch(game.bfVersion).
/// Returns a list of (character, field, delta) tuples where character is "boyfriend"/"dad"/"gf",
/// field is "x" or "y", and delta is the cumulative offset to apply.
fn parse_haxe_char_positions(code: &str, bf_name: &str) -> Vec<(&'static str, &'static str, f64)> {
    let mut results = Vec::new();

    // Parse local variable declarations: var xx:Float = 220;
    let mut locals = std::collections::HashMap::new();
    for line in code.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("var ") {
            // var xx:Float = 220;  or  var xx = 220;
            if let Some(eq_pos) = rest.find('=') {
                let name_part = rest[..eq_pos].trim();
                // Strip type annotation: "xx:Float" -> "xx"
                let var_name = name_part.split(':').next().unwrap_or(name_part).trim();
                let val_part = rest[eq_pos + 1..].trim().trim_end_matches(';').trim();
                if let Ok(val) = val_part.parse::<f64>() {
                    locals.insert(var_name.to_string(), val);
                }
            }
        }
    }

    // Find the matching case block for switch(game.bfVersion)
    if let Some(switch_pos) = code.find("switch(game.bfVersion)") {
        // Find the opening brace
        let after_switch = &code[switch_pos..];
        if let Some(brace_pos) = after_switch.find('{') {
            let switch_body = &after_switch[brace_pos + 1..];

            // Find the case matching bf_name
            let case_pattern = format!("case '{}':", bf_name);
            if let Some(case_pos) = switch_body.find(&case_pattern) {
                let case_body_start = case_pos + case_pattern.len();
                // Case body extends until next "case '" or closing "}"
                let case_body_end = switch_body[case_body_start..]
                    .find("case '")
                    .or_else(|| {
                        // Find closing brace, accounting for nesting
                        let mut depth = 0i32;
                        for (i, ch) in switch_body[case_body_start..].char_indices() {
                            match ch {
                                '{' => depth += 1,
                                '}' => {
                                    if depth == 0 {
                                        return Some(i);
                                    }
                                    depth -= 1;
                                }
                                _ => {}
                            }
                        }
                        None
                    })
                    .unwrap_or(switch_body.len() - case_body_start);

                let case_body = &switch_body[case_body_start..case_body_start + case_body_end];
                parse_char_assignment_lines(case_body, &locals, &mut results);
            }
        }
    }

    // Also parse direct (non-switch) game.boyfriend/dad/gf assignments
    // These appear outside switch blocks in some stages
    for line in code.lines() {
        let line = line.trim();
        if line.starts_with("//") {
            continue;
        }
        // Only parse lines NOT inside the switch block (simple heuristic: no "case" indentation)
        // Actually, let's just parse all direct assignments that are outside switch context
        // The switch handler above already handles those; here we handle standalone lines
        for (prefix, char_name) in &[
            ("game.dad.x", "dad"),
            ("game.dad.y", "dad"),
            ("game.gf.x", "gf"),
            ("game.gf.y", "gf"),
        ] {
            if line.starts_with(prefix) && !code.contains("switch(game.bfVersion)") {
                // Only parse if there's no switch block (otherwise the switch handler covers it)
                let field = if prefix.ends_with(".x") { "x" } else { "y" };
                if let Some(val) = parse_assignment_value(line, prefix, &locals) {
                    results.push((*char_name, field, val));
                }
            }
        }
    }

    results
}

/// Parse character assignment lines like:
///   game.boyfriend.x -= (xx + 300);
///   game.boyfriend.y += 200;
///   game.dad.x = -500;
fn parse_char_assignment_lines(
    body: &str,
    locals: &std::collections::HashMap<String, f64>,
    results: &mut Vec<(&'static str, &'static str, f64)>,
) {
    // Track cumulative offsets per (character, field)
    let mut offsets: std::collections::HashMap<(&str, &str), f64> =
        std::collections::HashMap::new();

    for line in body.lines() {
        let line = line.trim();
        if line.starts_with("//") {
            continue;
        }

        for (prefix, char_name, field) in &[
            ("game.boyfriend.x", "boyfriend", "x"),
            ("game.boyfriend.y", "boyfriend", "y"),
            ("game.dad.x", "dad", "x"),
            ("game.dad.y", "dad", "y"),
            ("game.gf.x", "gf", "x"),
            ("game.gf.y", "gf", "y"),
        ] {
            if !line.starts_with(prefix) {
                continue;
            }
            let rest = line[prefix.len()..].trim();

            if let Some(expr) = rest.strip_prefix("+=") {
                let val = eval_simple_expr(expr.trim().trim_end_matches(';'), locals);
                *offsets.entry((*char_name, *field)).or_insert(0.0) += val;
            } else if let Some(expr) = rest.strip_prefix("-=") {
                let val = eval_simple_expr(expr.trim().trim_end_matches(';'), locals);
                *offsets.entry((*char_name, *field)).or_insert(0.0) -= val;
            } else if let Some(expr) = rest.strip_prefix("=") {
                // Direct assignment — store as absolute value with a special marker
                let val = eval_simple_expr(expr.trim().trim_end_matches(';'), locals);
                // For absolute assignments, we push immediately with a large negative to indicate "set"
                results.push((*char_name, *field, f64::NAN)); // marker: absolute follows
                results.push((*char_name, *field, val));
                // Reset cumulative offset since we've set absolute
                offsets.remove(&(*char_name, *field));
            }
        }
    }

    // Push cumulative offsets
    for ((char_name, field), delta) in offsets {
        if delta.abs() > 0.001 {
            results.push((char_name, field, delta));
        }
    }
}

/// Evaluate a simple arithmetic expression that may contain local variable references.
/// Handles: numbers, variables, parenthesized groups, +, -, *.
fn eval_simple_expr(expr: &str, locals: &std::collections::HashMap<String, f64>) -> f64 {
    let expr = expr.trim().trim_end_matches(';');

    // Strip outer parens
    let expr = if expr.starts_with('(') && expr.ends_with(')') {
        &expr[1..expr.len() - 1]
    } else {
        expr
    };

    // Try parsing as a simple number
    if let Ok(val) = expr.parse::<f64>() {
        return val;
    }

    // Try as a local variable
    if let Some(&val) = locals.get(expr.trim()) {
        return val;
    }

    // Split on + and - at the top level (outside parens)
    let mut result = 0.0;
    let mut current_sign = 1.0f64;
    let mut depth = 0i32;
    let mut start = 0;

    let bytes = expr.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'+' | b'-' if depth == 0 && i > 0 => {
                let token = expr[start..i].trim();
                if !token.is_empty() {
                    result += current_sign * eval_token(token, locals);
                }
                current_sign = if bytes[i] == b'+' { 1.0 } else { -1.0 };
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    // Last token
    let token = expr[start..].trim();
    if !token.is_empty() {
        result += current_sign * eval_token(token, locals);
    }

    result
}

fn eval_token(token: &str, locals: &std::collections::HashMap<String, f64>) -> f64 {
    let token = token.trim();
    // Handle parenthesized expression
    if token.starts_with('(') && token.ends_with(')') {
        return eval_simple_expr(&token[1..token.len() - 1], locals);
    }
    // Handle multiplication
    if let Some(pos) = token.find('*') {
        let left = eval_token(&token[..pos], locals);
        let right = eval_token(&token[pos + 1..], locals);
        return left * right;
    }
    // Simple number
    if let Ok(val) = token.parse::<f64>() {
        return val;
    }
    // Variable
    if let Some(&val) = locals.get(token) {
        return val;
    }
    0.0
}

/// Parse a single assignment line and return the value.
fn parse_assignment_value(
    line: &str,
    prefix: &str,
    locals: &std::collections::HashMap<String, f64>,
) -> Option<f64> {
    let rest = line[prefix.len()..].trim();
    if let Some(expr) = rest.strip_prefix("+=") {
        Some(eval_simple_expr(expr.trim().trim_end_matches(';'), locals))
    } else if let Some(expr) = rest.strip_prefix("-=") {
        Some(-eval_simple_expr(expr.trim().trim_end_matches(';'), locals))
    } else if let Some(expr) = rest.strip_prefix("=") {
        Some(eval_simple_expr(expr.trim().trim_end_matches(';'), locals))
    } else {
        None
    }
}

fn lua_val_to_string(val: &LuaValue) -> String {
    match val {
        LuaValue::String(s) => s.to_string_lossy().to_string(),
        LuaValue::Integer(i) => i.to_string(),
        LuaValue::Number(f) => format!("{f}"),
        LuaValue::Boolean(b) => b.to_string(),
        LuaValue::Nil => "nil".to_string(),
        _ => format!("{:?}", val),
    }
}
