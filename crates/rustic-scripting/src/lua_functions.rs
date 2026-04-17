use std::io::Read as _;

use mlua::prelude::*;

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

/// Read width and height from a PNG file's IHDR chunk (first 24 bytes).
fn read_png_dimensions(path: &std::path::Path) -> Option<(u32, u32)> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 24];
    f.read_exact(&mut header).ok()?;
    if &header[0..8] != b"\x89PNG\r\n\x1a\n" { return None; }
    let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
    let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
    Some((width, height))
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
    g.set("__pending_cam_fx", lua.create_table()?)?;
    g.set("__pending_subtitles", lua.create_table()?)?;
    g.set("__pending_char_positions", lua.create_table()?)?;
    g.set("__pending_audio", lua.create_table()?)?;

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
    globals.set("makeLuaSprite", lua.create_function(|lua, (tag, image, x, y): (String, LuaValue, Option<f64>, Option<f64>)| {
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
    })?)?;

    // makeAnimatedLuaSprite(tag, image, x, y, spriteType)
    globals.set("makeAnimatedLuaSprite", lua.create_function(|lua, (tag, image, x, y, _spr_type): (String, String, Option<f64>, Option<f64>, Option<String>)| {
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
    })?)?;

    // makeGraphic(tag, width, height, color)
    globals.set("makeGraphic", lua.create_function(|lua, (tag, width, height, color): (String, i32, i32, Option<String>)| {
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
    })?)?;

    // addLuaSprite(tag, inFront)
    globals.set("addLuaSprite", lua.create_function(|lua, (tag, in_front): (String, Option<bool>)| {
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
    })?)?;

    // removeLuaSprite(tag, destroy)
    globals.set("removeLuaSprite", lua.create_function(|lua, (tag, _destroy): (String, Option<bool>)| {
        // Mark sprite as invisible so it stops rendering
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
            tbl.set("visible", false)?;
        }
        // Queue for removal by the game engine
        let pending: LuaTable = lua.globals().get("__pending_removes")?;
        let len = pending.len()? as i64;
        pending.set(len + 1, tag)?;
        Ok(())
    })?)?;

    // scaleObject(tag, scaleX, scaleY)
    globals.set("scaleObject", lua.create_function(|lua, (tag, sx, sy): (String, LuaValue, LuaValue)| {
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
    })?)?;

    // setGraphicSize(tag, width, height) — matches HaxeFlixel's setGraphicSize:
    // scale = newSize / frameSize, with aspect-ratio preservation when one arg is 0.
    globals.set("setGraphicSize", lua.create_function(|lua, (tag, width, height): (String, Option<LuaValue>, Option<LuaValue>)| {
        let new_w = lua_to_f32(&width);
        let new_h = lua_to_f32(&height);
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let tex_w: f32 = tbl.get("tex_w").unwrap_or(1.0);
            let tex_h: f32 = tbl.get("tex_h").unwrap_or(1.0);
            if tex_w > 0.0 && tex_h > 0.0 {
                let mut sx = if new_w > 0.0 { new_w / tex_w } else { 0.0 };
                let mut sy = if new_h > 0.0 { new_h / tex_h } else { 0.0 };
                // Preserve aspect ratio when one dimension is 0
                if new_w <= 0.0 { sx = sy; }
                if new_h <= 0.0 { sy = sx; }
                tbl.set("scale_x", sx)?;
                tbl.set("scale_y", sy)?;
            }
        }
        Ok(())
    })?)?;

    // updateHitbox(tag)
    globals.set("updateHitbox", lua.create_function(|_lua, _tag: String| {
        Ok(())
    })?)?;

    // setScrollFactor(tag, x, y)
    globals.set("setScrollFactor", lua.create_function(|lua, (tag, x, y): (String, f64, f64)| {
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            tbl.set("scroll_x", x as f32)?;
            tbl.set("scroll_y", y as f32)?;
        }
        Ok(())
    })?)?;

    // setObjectOrder(tag, order)
    globals.set("setObjectOrder", lua.create_function(|lua, (tag, order): (String, i32)| {
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            tbl.set("order", order)?;
        }
        Ok(())
    })?)?;

    // getObjectOrder(tag)
    globals.set("getObjectOrder", lua.create_function(|lua, tag: String| -> LuaResult<i32> {
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            return Ok(tbl.get::<i32>("order").unwrap_or(0));
        }
        Ok(0)
    })?)?;

    // addAnimationByPrefix(tag, anim, prefix, fps, looping)
    globals.set("addAnimationByPrefix", lua.create_function(|lua, (tag, anim, prefix, fps, looping): (String, String, String, Option<f64>, Option<bool>)| {
        let fps = fps.unwrap_or(24.0);
        let looping = looping.unwrap_or(true);
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
            anim_tbl.set("looping", looping)?;
            anims.set(anim, anim_tbl)?;
        }
        Ok(())
    })?)?;

    // addAnimationByIndices(tag, anim, prefix, indices, fps, looping)
    globals.set("addAnimationByIndices", lua.create_function(|lua, (tag, anim, prefix, indices, fps, looping): (String, String, String, LuaValue, Option<f64>, Option<bool>)| {
        let fps = fps.unwrap_or(24.0);
        let looping = looping.unwrap_or(true);
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
    })?)?;

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
    globals.set("addAnimation", lua.create_function(|lua, (tag, anim, prefix, fps, looping): (String, String, LuaValue, Option<f64>, Option<bool>)| {
        let fps = fps.unwrap_or(24.0);
        let looping = looping.unwrap_or(true);
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
    })?)?;

    // addOffset(tag, anim, x, y)
    globals.set("addOffset", lua.create_function(|lua, (tag, anim, x, y): (String, String, Option<f64>, Option<f64>)| {
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
    })?)?;

    // playAnim(tag, anim, forced, reversed, frame)
    globals.set("playAnim", lua.create_function(|lua, (tag, anim, forced, _reversed, _frame): (String, String, Option<bool>, Option<bool>, Option<i32>)| {
        let forced = forced.unwrap_or(false);
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let play_tbl = lua.create_table()?;
            play_tbl.set("anim", anim)?;
            play_tbl.set("forced", forced)?;
            tbl.set("__pending_anim", play_tbl)?;
        }
        Ok(())
    })?)?;

    // objectPlayAnimation — alias for playAnim
    globals.set("objectPlayAnimation", lua.create_function(|lua, (tag, anim, forced): (String, String, Option<bool>)| {
        let forced = forced.unwrap_or(false);
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let play_tbl = lua.create_table()?;
            play_tbl.set("anim", anim)?;
            play_tbl.set("forced", forced)?;
            tbl.set("__pending_anim", play_tbl)?;
        }
        Ok(())
    })?)?;

    // characterPlayAnim(charType, anim, forced) — queue character anim via pending props
    globals.set("characterPlayAnim", lua.create_function(|lua, (char_type, anim, forced): (String, String, Option<bool>)| {
        let forced = forced.unwrap_or(false);
        // Queue as a property write for the app layer to handle
        let pending: LuaTable = lua.globals().get("__pending_props")?;
        let tbl = lua.create_table()?;
        let prop = format!("__charPlayAnim{}{}", if forced { "" } else { "Soft." }, char_type);
        tbl.set("prop", prop)?;
        tbl.set("value", anim)?;
        pending.set(pending.len()? + 1, tbl)?;
        Ok(())
    })?)?;

    // addProperty(name, defaultValue) — creates a property (stored as custom var)
    globals.set("addProperty", lua.create_function(|lua, (name, value): (String, LuaValue)| {
        let vars: LuaTable = lua.globals().get("__custom_vars")?;
        // Only set if not already defined
        let existing: LuaValue = vars.get(name.as_str())?;
        if matches!(existing, LuaNil) {
            vars.set(name, value)?;
        }
        Ok(())
    })?)?;

    // screenCenter(tag, axis)
    globals.set("screenCenter", lua.create_function(|lua, (tag, axis): (String, Option<String>)| {
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
    })?)?;

    // setObjectCamera(tag, camera)
    globals.set("setObjectCamera", lua.create_function(|lua, (tag, cam): (String, Option<String>)| {
        let cam = cam.unwrap_or_else(|| "camGame".to_string());
        let cam_name = match cam.to_lowercase().as_str() {
            "camhud" | "hud" => "camHUD",
            "camother" | "other" => "camOther",
            _ => "camGame",
        };
        // Set on sprite data
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.clone()) {
            tbl.set("camera", cam_name)?;
        }
        // Also set on text data
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
            tbl.set("camera", cam_name)?;
        }
        Ok(())
    })?)?;

    // luaSpriteExists(tag)
    globals.set("luaSpriteExists", lua.create_function(|lua, tag: String| -> LuaResult<bool> {
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        Ok(sprite_data.contains_key(tag)?)
    })?)?;

    // setBlendMode(tag, blend)
    globals.set("setBlendMode", lua.create_function(|lua, (tag, blend): (String, Option<String>)| {
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
    })?)?;

    // loadGraphic(tag, image, ?width, ?height)
    // In Psych Engine this reloads (and optionally crops) the graphic for a sprite.
    globals.set("loadGraphic", lua.create_function(|lua, (tag, image, width, height): (String, Option<String>, Option<f64>, Option<f64>)| {
        if let (Some(_w), Some(_h)) = (width, height) {
            // Store crop dimensions for the sprite — the render layer will apply them
            if let Ok(tbl) = lua.globals().get::<LuaTable>("__sprite_data")?.get::<LuaTable>(&tag as &str) {
                tbl.set("crop_w", _w)?;
                tbl.set("crop_h", _h)?;
            }
        }
        let _ = image; // image path — actual reload handled by render layer
        Ok(())
    })?)?;

    // updateHitboxFromGroup(group, index) — deprecated Psych Engine function
    // Calls updateHitbox on a member of a FlxTypedGroup (e.g. unspawnNotes[i]).
    globals.set("updateHitboxFromGroup", lua.create_function(|_lua, (_group, _index): (String, i32)| {
        // No-op: our note hitbox recalculation happens automatically on scale changes
        Ok(())
    })?)?;

    // getColorFromRGB(r, g, b) — returns an integer color value
    globals.set("getColorFromRGB", lua.create_function(|_lua, (r, g, b): (i32, i32, i32)| -> LuaResult<i64> {
        Ok(((r.clamp(0, 255) as i64) << 16) | ((g.clamp(0, 255) as i64) << 8) | (b.clamp(0, 255) as i64))
    })?)?;

    // setSongTime(time) — seek to a position in the song (ms)
    globals.set("setSongTime", lua.create_function(|lua, time: f64| {
        let pending: LuaTable = lua.globals().get("__pending_audio")?;
        pending.set("seek_to", time)?;
        Ok(())
    })?)?;

    // reloadHealthBarColors() — no-op for now, health bar colors are managed differently
    globals.set("reloadHealthBarColors", lua.create_function(|_lua, ()| { Ok(()) })?)?;

    // changeIcon(character) — change the health icon for opponent/player
    globals.set("changeIcon", lua.create_function(|_lua, _character: String| { Ok(()) })?)?;

    // makeRating(sprite stuff) — create a rating popup sprite
    globals.set("makeRating", lua.create_function(|_lua, _args: LuaMultiValue| { Ok(()) })?)?;

    Ok(())
}

fn register_property_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // setProperty(property, value)
    globals.set("setProperty", lua.create_function(|lua, (prop, value): (String, LuaValue)| {
        // Check if it's a sprite property (tag.field) — also handles nested like tag.origin.y
        if let Some(dot_pos) = prop.find('.') {
            let tag = &prop[..dot_pos];
            let field = &prop[dot_pos + 1..];
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.to_string()) {
                match field {
                    "alpha" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("alpha", n)?; } }
                    "visible" => { if let LuaValue::Boolean(b) = &value { tbl.set("visible", *b)?; } }
                    "flipX" | "flip_x" => { if let LuaValue::Boolean(b) = &value { tbl.set("flip_x", *b)?; } }
                    "flipY" | "flip_y" => { if let LuaValue::Boolean(b) = &value { tbl.set("flip_y", *b)?; } }
                    "antialiasing" => { if let LuaValue::Boolean(b) = &value { tbl.set("antialiasing", *b)?; } }
                    "x" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("x", n)?; } }
                    "y" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("y", n)?; } }
                    "angle" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("angle", n)?; } }
                    "scale.x" | "scaleX" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("scale_x", n)?; } }
                    "scale.y" | "scaleY" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("scale_y", n)?; } }
                    "scrollFactor.x" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("scroll_x", n)?; } }
                    "scrollFactor.y" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("scroll_y", n)?; } }
                    "origin.x" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("origin_x", n)?; } }
                    "origin.y" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("origin_y", n)?; } }
                    "offset.x" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("offset_x", n)?; } }
                    "offset.y" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("offset_y", n)?; } }
                    "colorTransform.redOffset" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("ct_red", n)?; } }
                    "colorTransform.greenOffset" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("ct_green", n)?; } }
                    "colorTransform.blueOffset" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("ct_blue", n)?; } }
                    "animation.frameIndex" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("anim_frame", n as i32)?; } }
                    "animation.framerate" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("anim_fps", n)?; } }
                    _ => { tbl.set(format!("_prop_{field}"), value.clone())?; }
                }
                return Ok(());
            }
            // Also check text objects
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag.to_string()) {
                match field {
                    "alpha" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("alpha", n)?; } }
                    "visible" => { if let LuaValue::Boolean(b) = &value { tbl.set("visible", *b)?; } }
                    "x" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("x", n)?; } }
                    "y" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("y", n)?; } }
                    "angle" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("angle", n)?; } }
                    "antialiasing" => { if let LuaValue::Boolean(b) = &value { tbl.set("antialiasing", *b)?; } }
                    _ => { tbl.set(format!("_prop_{field}"), value.clone())?; }
                }
                return Ok(());
            }
        }

        let g = lua.globals();

        // Update Lua global if it's a known game property
        match prop.as_str() {
            "defaultCamZoom" | "cameraSpeed" | "camZooming" | "camZoomingMult"
            | "camZoomingDecay" | "gameZoomingDecay" | "crochet" | "stepCrochet"
            | "isCameraOnForcedPos" | "health" => {
                g.set(prop.as_str(), value.clone()).ok();
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
    })?)?;

    // getProperty(property) -> value
    globals.set("getProperty", lua.create_function(|lua, prop: String| -> LuaResult<LuaValue> {
        if let Some(dot_pos) = prop.find('.') {
            let tag = &prop[..dot_pos];
            let field = &prop[dot_pos + 1..];
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.to_string()) {
                return match field {
                    "x" => Ok(tbl.get::<LuaValue>("x").unwrap_or(LuaValue::Number(0.0))),
                    "y" => Ok(tbl.get::<LuaValue>("y").unwrap_or(LuaValue::Number(0.0))),
                    "alpha" => Ok(tbl.get::<LuaValue>("alpha").unwrap_or(LuaValue::Number(1.0))),
                    "visible" => Ok(tbl.get::<LuaValue>("visible").unwrap_or(LuaValue::Boolean(true))),
                    "angle" => Ok(tbl.get::<LuaValue>("angle").unwrap_or(LuaValue::Number(0.0))),
                    "flipX" => Ok(tbl.get::<LuaValue>("flip_x").unwrap_or(LuaValue::Boolean(false))),
                    "flipY" => Ok(tbl.get::<LuaValue>("flip_y").unwrap_or(LuaValue::Boolean(false))),
                    "scale.x" | "scaleX" => Ok(tbl.get::<LuaValue>("scale_x").unwrap_or(LuaValue::Number(1.0))),
                    "scale.y" | "scaleY" => Ok(tbl.get::<LuaValue>("scale_y").unwrap_or(LuaValue::Number(1.0))),
                    "scrollFactor.x" => Ok(tbl.get::<LuaValue>("scroll_x").unwrap_or(LuaValue::Number(1.0))),
                    "scrollFactor.y" => Ok(tbl.get::<LuaValue>("scroll_y").unwrap_or(LuaValue::Number(1.0))),
                    "antialiasing" => Ok(tbl.get::<LuaValue>("antialiasing").unwrap_or(LuaValue::Boolean(true))),
                    "origin.x" => Ok(tbl.get::<LuaValue>("origin_x").unwrap_or(LuaNil)),
                    "origin.y" => Ok(tbl.get::<LuaValue>("origin_y").unwrap_or(LuaNil)),
                    "offset.x" => Ok(tbl.get::<LuaValue>("offset_x").unwrap_or(LuaValue::Number(0.0))),
                    "offset.y" => Ok(tbl.get::<LuaValue>("offset_y").unwrap_or(LuaValue::Number(0.0))),
                    "colorTransform.redOffset" => Ok(tbl.get::<LuaValue>("ct_red").unwrap_or(LuaValue::Number(0.0))),
                    "colorTransform.greenOffset" => Ok(tbl.get::<LuaValue>("ct_green").unwrap_or(LuaValue::Number(0.0))),
                    "colorTransform.blueOffset" => Ok(tbl.get::<LuaValue>("ct_blue").unwrap_or(LuaValue::Number(0.0))),
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
                    "animation.frameIndex" => Ok(tbl.get::<LuaValue>("anim_frame").unwrap_or(LuaValue::Integer(0))),
                    "animation.framerate" => Ok(tbl.get::<LuaValue>("anim_fps").unwrap_or(LuaValue::Number(24.0))),
                    _ => Ok(tbl.get::<LuaValue>(format!("_prop_{field}")).unwrap_or(LuaNil)),
                };
            }
            // Also check text objects
            let text_data: LuaTable = lua.globals().get("__text_data")?;
            if let Ok(tbl) = text_data.get::<LuaTable>(tag.to_string()) {
                return match field {
                    "x" => Ok(tbl.get::<LuaValue>("x").unwrap_or(LuaValue::Number(0.0))),
                    "y" => Ok(tbl.get::<LuaValue>("y").unwrap_or(LuaValue::Number(0.0))),
                    "alpha" => Ok(tbl.get::<LuaValue>("alpha").unwrap_or(LuaValue::Number(1.0))),
                    "visible" => Ok(tbl.get::<LuaValue>("visible").unwrap_or(LuaValue::Boolean(true))),
                    "angle" => Ok(tbl.get::<LuaValue>("angle").unwrap_or(LuaValue::Number(0.0))),
                    "width" => Ok(tbl.get::<LuaValue>("width").unwrap_or(LuaValue::Number(0.0))),
                    "text" => Ok(tbl.get::<LuaValue>("text").unwrap_or(LuaValue::String(lua.create_string("")?))),
                    "size" => Ok(tbl.get::<LuaValue>("size").unwrap_or(LuaValue::Number(16.0))),
                    "font" => Ok(tbl.get::<LuaValue>("font").unwrap_or(LuaValue::String(lua.create_string("")?))),
                    "borderSize" => Ok(tbl.get::<LuaValue>("border").unwrap_or(LuaValue::Number(0.0))),
                    _ => Ok(tbl.get::<LuaValue>(format!("_prop_{field}")).unwrap_or(LuaNil)),
                };
            }
        }
        // Character properties (animation name, position, alpha, etc.)
        {
            let g = lua.globals();
            match prop.as_str() {
                "dad.animation.curAnim.name" | "opponent.animation.curAnim.name" =>
                    return Ok(g.get::<LuaValue>("__dad_anim_name").unwrap_or(LuaValue::String(lua.create_string("")?))),
                "boyfriend.animation.curAnim.name" | "bf.animation.curAnim.name" =>
                    return Ok(g.get::<LuaValue>("__bf_anim_name").unwrap_or(LuaValue::String(lua.create_string("")?))),
                "gf.animation.curAnim.name" | "girlfriend.animation.curAnim.name" =>
                    return Ok(g.get::<LuaValue>("__gf_anim_name").unwrap_or(LuaValue::String(lua.create_string("")?))),
                "dad.animateAtlas.anim.curFrame" | "boyfriend.animateAtlas.anim.curFrame"
                | "gf.animateAtlas.anim.curFrame" =>
                    return Ok(LuaValue::Integer(0)),
                // Character position reads — synced from game each frame
                "dad.x" | "dadGroup.x" =>
                    return Ok(g.get::<LuaValue>("__dad_x").unwrap_or(LuaValue::Number(0.0))),
                "dad.y" | "dadGroup.y" =>
                    return Ok(g.get::<LuaValue>("__dad_y").unwrap_or(LuaValue::Number(0.0))),
                "boyfriend.x" | "bf.x" | "boyfriendGroup.x" =>
                    return Ok(g.get::<LuaValue>("__bf_x").unwrap_or(LuaValue::Number(0.0))),
                "boyfriend.y" | "bf.y" | "boyfriendGroup.y" =>
                    return Ok(g.get::<LuaValue>("__bf_y").unwrap_or(LuaValue::Number(0.0))),
                "gf.x" | "girlfriend.x" | "gfGroup.x" =>
                    return Ok(g.get::<LuaValue>("__gf_x").unwrap_or(LuaValue::Number(0.0))),
                "gf.y" | "girlfriend.y" | "gfGroup.y" =>
                    return Ok(g.get::<LuaValue>("__gf_y").unwrap_or(LuaValue::Number(0.0))),
                _ => {}
            }
        }

        // Handle dotted paths for game object arrays (e.g. "unspawnNotes.length")
        if prop == "unspawnNotes.length" || prop == "notes.length" {
            let g = lua.globals();
            return Ok(LuaValue::Integer(g.get::<i64>("__unspawnNotesLength").unwrap_or(0)));
        }

        // Known game properties — read from globals (which Lua scripts may have set)
        let g = lua.globals();
        match prop.as_str() {
            "defaultCamZoom" => Ok(g.get::<LuaValue>("defaultCamZoom").unwrap_or(LuaValue::Number(0.9))),
            "cameraSpeed" => Ok(g.get::<LuaValue>("cameraSpeed").unwrap_or(LuaValue::Number(1.0))),
            "camZooming" | "camZoomingMult" | "camZoomingDecay" | "gameZoomingDecay" => {
                Ok(g.get::<LuaValue>(prop.as_str()).unwrap_or(LuaValue::Number(1.0)))
            }
            "healthGainMult" => Ok(LuaValue::Number(1.0)),
            "healthLossMult" => Ok(LuaValue::Number(1.0)),
            "playbackRate" => Ok(LuaValue::Number(1.0)),
            "songLength" => Ok(LuaValue::Number(0.0)),
            "crochet" => Ok(g.get::<LuaValue>("crochet").unwrap_or(LuaValue::Number(500.0))),
            "stepCrochet" => Ok(g.get::<LuaValue>("stepCrochet").unwrap_or(LuaValue::Number(125.0))),
            _ => {
                // Check custom variables table (set via setProperty for unknown properties)
                let custom: LuaTable = g.get::<LuaTable>("__custom_vars").unwrap_or(lua.create_table().unwrap());
                let val = custom.get::<LuaValue>(prop.as_str()).unwrap_or(LuaNil);
                if val != LuaNil {
                    return Ok(val);
                }
                // Check game object property paths stored as globals
                let val = g.get::<LuaValue>(format!("__gprop_{prop}")).unwrap_or(LuaNil);
                if val != LuaNil {
                    return Ok(val);
                }
                // Default: return 0 instead of nil to prevent arithmetic errors
                // (Psych Engine returns 0/false for missing properties via Reflect)
                log::debug!("getProperty: unknown property '{}', returning 0", prop);
                Ok(LuaValue::Number(0.0))
            }
        }
    })?)?;

    // getPropertyFromGroup(group, index, field)
    globals.set("getPropertyFromGroup", lua.create_function(|lua, (group, idx, field): (String, i32, String)| -> LuaResult<LuaValue> {
        // Handle strum groups
        let strum_idx = match group.as_str() {
            "opponentStrums" => Some(idx as usize),          // 0-3
            "playerStrums" => Some((idx + 4) as usize),      // 4-7
            "strumLineNotes" => Some(idx as usize),           // 0-7 direct
            _ => None,
        };
        if let Some(si) = strum_idx {
            if si < 8 {
                let props: LuaTable = lua.globals().get("__strum_props")?;
                if let Ok(tbl) = props.get::<LuaTable>(si as i64 + 1) {
                    return match field.as_str() {
                        "x" => Ok(tbl.get::<LuaValue>("x").unwrap_or(LuaValue::Number(0.0))),
                        "y" => Ok(tbl.get::<LuaValue>("y").unwrap_or(LuaValue::Number(0.0))),
                        "alpha" => Ok(tbl.get::<LuaValue>("alpha").unwrap_or(LuaValue::Number(1.0))),
                        "angle" => Ok(tbl.get::<LuaValue>("angle").unwrap_or(LuaValue::Number(0.0))),
                        "scale.x" => Ok(tbl.get::<LuaValue>("scale_x").unwrap_or(LuaValue::Number(0.7))),
                        "scale.y" => Ok(tbl.get::<LuaValue>("scale_y").unwrap_or(LuaValue::Number(0.7))),
                        "downScroll" => Ok(tbl.get::<LuaValue>("downScroll").unwrap_or(LuaValue::Boolean(false))),
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
                    "strumTime" => Ok(note_tbl.get::<LuaValue>("strumTime").unwrap_or(LuaValue::Number(0.0))),
                    "noteData" | "lane" => Ok(note_tbl.get::<LuaValue>("lane").unwrap_or(LuaValue::Integer(0))),
                    "mustPress" => Ok(note_tbl.get::<LuaValue>("mustPress").unwrap_or(LuaValue::Boolean(false))),
                    "isSustainNote" => Ok(note_tbl.get::<LuaValue>("isSustainNote").unwrap_or(LuaValue::Boolean(false))),
                    "sustainLength" => Ok(note_tbl.get::<LuaValue>("sustainLength").unwrap_or(LuaValue::Number(0.0))),
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
                    "colorTransform.redOffset" | "colorTransform.greenOffset" | "colorTransform.blueOffset" => Ok(LuaValue::Number(0.0)),
                    _ => Ok(LuaNil),
                };
            }
        }
        Ok(LuaNil)
    })?)?;

    // setPropertyFromGroup(group, index, field, value)
    globals.set("setPropertyFromGroup", lua.create_function(|lua, (group, idx, field, value): (String, i32, String, LuaValue)| {
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
    })?)?;

    // getPropertyFromClass — return values from known classes
    globals.set("getPropertyFromClass", lua.create_function(|lua, (class, var): (String, String)| -> LuaResult<LuaValue> {
        if class.contains("PlayState") {
            let g = lua.globals();
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
                    return Ok(custom.get::<LuaValue>("__pressedCheckpoint").unwrap_or(LuaNil));
                }
                "endedCatastro" => {
                    let custom: LuaTable = g.get("__custom_vars")?;
                    return Ok(custom.get::<LuaValue>("endedCatastro").unwrap_or(LuaNil));
                }
                _ => {}
            }
        }
        Ok(LuaNil)
    })?)?;

    // setPropertyFromClass — store values for PlayState properties
    globals.set("setPropertyFromClass", lua.create_function(|lua, (class, var, val): (String, String, LuaValue)| {
        if class.contains("PlayState") {
            let custom: LuaTable = lua.globals().get("__custom_vars")?;
            match var.as_str() {
                "bfVersion" => {
                    if let LuaValue::String(s) = &val {
                        lua.globals().set("boyfriendName", s.to_string_lossy().to_string())?;
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
    })?)?;

    // set(property, value) — alias for setProperty (used by some mods)
    globals.set("set", lua.create_function(|lua, (prop, value): (String, LuaValue)| {
        let set_prop: LuaFunction = lua.globals().get("setProperty")?;
        set_prop.call::<()>((prop, value))?;
        Ok(())
    })?)?;

    // setVar / getVar — custom variables shared across scripts
    globals.set("setVar", lua.create_function(|lua, (name, value): (String, LuaValue)| {
        let vars: LuaTable = lua.globals().get("__custom_vars")?;
        vars.set(name, value)?;
        Ok(())
    })?)?;

    globals.set("getVar", lua.create_function(|lua, name: String| -> LuaResult<LuaValue> {
        let vars: LuaTable = lua.globals().get("__custom_vars")?;
        Ok(vars.get::<LuaValue>(name).unwrap_or(LuaNil))
    })?)?;

    globals.set("setGlobalFromScript", lua.create_function(|lua, (_script_name, name, value): (String, String, LuaValue)| {
        let vars: LuaTable = lua.globals().get("__custom_vars")?;
        vars.set(name, value)?;
        Ok(())
    })?)?;

    globals.set("getGlobalFromScript", lua.create_function(|lua, (_script_name, name): (String, String)| -> LuaResult<LuaValue> {
        let vars: LuaTable = lua.globals().get("__custom_vars")?;
        Ok(vars.get::<LuaValue>(name).unwrap_or(LuaNil))
    })?)?;

    Ok(())
}

fn register_utility_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // close()
    globals.set("close", lua.create_function(|lua, ()| {
        lua.globals().set("__script_closed", true)?;
        Ok(())
    })?)?;

    // debugPrint(...)
    globals.set("debugPrint", lua.create_function(|_lua, args: LuaMultiValue| {
        let parts: Vec<String> = args.iter().map(|v| lua_val_to_string(v)).collect();
        log::info!("[Lua] {}", parts.join(" "));
        Ok(())
    })?)?;

    // luaTrace — alias
    globals.set("luaTrace", lua.create_function(|_lua, args: LuaMultiValue| {
        let parts: Vec<String> = args.iter().map(|v| lua_val_to_string(v)).collect();
        log::info!("[Lua] {}", parts.join(" "));
        Ok(())
    })?)?;

    // runHaxeCode — pattern-match common Haxe patterns and execute natively
    globals.set("runHaxeCode", lua.create_function(|lua, code: String| -> LuaResult<LuaValue> {
        let code = code.trim();

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
                log::debug!("runHaxeCode: FlxTween.num({}, {} -> {}, {}s)", var_name, start_val, end_val, duration);
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
        if code.contains("game.boyfriend.") || code.contains("game.dad.") || code.contains("game.gf.") {
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
                log::info!("runHaxeCode: parsed {} char position adjustments for bf='{}'", positions.len(), bf_name);
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
                let val_str = val_str.trim().trim_start_matches('+').trim_start_matches('=').trim();
                if let Ok(delta) = val_str.split(';').next().unwrap_or(val_str).trim().parse::<f32>() {
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
        if code.contains("callOnLuas") && code.contains("setPosition") {
            // Extract the position value from [N]
            if let Some(start) = code.find('[') {
                if let Some(end) = code.find(']') {
                    if let Ok(pos) = code[start+1..end].trim().parse::<f64>() {
                        // Queue as a property write that the engine will process
                        let pending: LuaTable = lua.globals().get("__pending_props")?;
                        let tbl = lua.create_table()?;
                        tbl.set("prop", "__setPosition")?;
                        tbl.set("value", pos)?;
                        let len = pending.len()? as i64;
                        pending.set(len + 1, tbl)?;
                    }
                }
            }
            return Ok(LuaNil);
        }

        // Ignore: function definitions, camCharacters.shake, camVideo.zoom, etc.
        if code.contains("function ") || code.contains(".shake(") || code.contains("camVideo.") {
            return Ok(LuaNil);
        }

        log::debug!("runHaxeCode: unhandled pattern: {}", &code[..code.len().min(80)]);
        Ok(LuaNil)
    })?)?;

    // runHaxeFunction(name, args) — call a previously defined Haxe function
    globals.set("runHaxeFunction", lua.create_function(|lua, (name, args): (String, Option<LuaTable>)| -> LuaResult<LuaValue> {
        // Handle known function patterns
        match name.as_str() {
            "charactersCamera" => {
                // charactersCamera(visible) — toggle character layer visibility
                let visible = args.as_ref()
                    .and_then(|t| t.get::<LuaValue>(1).ok())
                    .map(|v| match v { LuaValue::Boolean(b) => b, LuaValue::Number(n) => n != 0.0, _ => true })
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
                let enabled = args.as_ref()
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
                let x = args.as_ref()
                    .and_then(|t| t.get::<f64>(1).ok())
                    .unwrap_or(0.0) as f32;
                let y = args.as_ref()
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
                let actual_step = if ended_catastro { 3328.0 } else { cur_step.max(0.0) };
                // Replicate the FlxMath.remapToRange logic from the Haxe code
                let bf_position = if actual_step <= 1792.0 {
                    remap_to_range(actual_step, 0.0, 1472.0, 8219.1605, 2000.355)
                } else if actual_step <= 2304.0 {
                    remap_to_range(actual_step, 0.0, 1472.0, 8219.1605, 1847.355)
                } else if actual_step <= 2560.0 {
                    remap_to_range(actual_step, 2304.0, 2560.0, -1758.35969606794, -5703.23798043777)
                } else {
                    remap_to_range(actual_step, 2560.0, 3328.0, -5703.23798043777, -68148.9101742824)
                };
                // Call the Lua setPosition function directly if it exists
                if let Ok(set_pos_fn) = g.get::<LuaFunction>("setPosition") {
                    if let Err(e) = set_pos_fn.call::<()>(bf_position) {
                        log::debug!("adjustPositions: setPosition({}) failed: {}", bf_position, e);
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
            _ => {
                log::debug!("runHaxeFunction: unhandled function '{}'", name);
            }
        }
        Ok(LuaNil)
    })?)?;

    // getSongPosition() — reads from Lua global kept in sync by the game
    globals.set("getSongPosition", lua.create_function(|lua, ()| -> LuaResult<f64> {
        let g = lua.globals();
        Ok(g.get::<f64>("__songPosition").unwrap_or(0.0))
    })?)?;

    // cameraSetTarget(target) — queues camera target switch
    globals.set("cameraSetTarget", lua.create_function(|lua, target: String| {
        let g = lua.globals();
        let pending: LuaTable = g.get::<LuaTable>("__pending_cam_targets")
            .unwrap_or_else(|_| lua.create_table().unwrap());
        let len = pending.len().unwrap_or(0);
        pending.set(len + 1, target)?;
        g.set("__pending_cam_targets", pending)?;
        Ok(())
    })?)?;

    // triggerEvent(name, v1, v2) — queues event for game processing
    globals.set("triggerEvent", lua.create_function(|lua, (name, v1, v2): (String, Option<mlua::Value>, Option<mlua::Value>)| {
        let g = lua.globals();
        let pending: LuaTable = g.get::<LuaTable>("__pending_events")
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
    })?)?;

    // moveCameraSection(section) — move camera based on chart section's mustHitSection
    globals.set("moveCameraSection", lua.create_function(|lua, section: Option<i32>| {
        let section = section.unwrap_or(0);
        let g = lua.globals();
        let pending: LuaTable = g.get("__pending_cam_sections")?;
        let len = pending.len()? as i64;
        pending.set(len + 1, section)?;
        Ok(())
    })?)?;

    // getColorFromHex(hex) -> integer
    globals.set("getColorFromHex", lua.create_function(|_lua, hex: String| -> LuaResult<i64> {
        let hex = hex.trim_start_matches('#').trim_start_matches("0x").trim_start_matches("0X");
        let val = u32::from_str_radix(hex, 16).unwrap_or(0xFFFFFF);
        let val = if hex.len() <= 6 { 0xFF000000 | val } else { val };
        Ok(val as i64)
    })?)?;

    globals.set("FlxColor", lua.create_function(|_lua, hex: String| -> LuaResult<i64> {
        let hex = hex.trim_start_matches('#').trim_start_matches("0x").trim_start_matches("0X");
        let val = u32::from_str_radix(hex, 16).unwrap_or(0xFFFFFF);
        let val = if hex.len() <= 6 { 0xFF000000 | val } else { val };
        Ok(val as i64)
    })?)?;

    // String utils
    globals.set("stringStartsWith", lua.create_function(|_lua, (s, prefix): (LuaValue, LuaValue)| -> LuaResult<bool> {
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
    })?)?;

    globals.set("stringEndsWith", lua.create_function(|_lua, (s, suffix): (LuaValue, LuaValue)| -> LuaResult<bool> {
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
    })?)?;

    globals.set("stringSplit", lua.create_function(|lua, (s, sep): (LuaValue, LuaValue)| {
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
    })?)?;

    globals.set("stringTrim", lua.create_function(|_lua, s: String| {
        Ok(s.trim().to_string())
    })?)?;

    // Random
    globals.set("getRandomInt", lua.create_function(|_lua, (min, max, _exclude): (i32, i32, Option<String>)| -> LuaResult<i32> {
        let range = (max - min + 1).max(1);
        let val = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()) as i32;
        Ok(min + (val.unsigned_abs() as i32 % range))
    })?)?;

    globals.set("getRandomFloat", lua.create_function(|_lua, (min, max, _exclude): (f64, f64, Option<String>)| -> LuaResult<f64> {
        let t = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as f64) / 1_000_000_000.0;
        Ok(min + t * (max - min))
    })?)?;

    globals.set("getRandomBool", lua.create_function(|_lua, chance: Option<f64>| -> LuaResult<bool> {
        let chance = chance.unwrap_or(50.0);
        let t = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as f64) / 1_000_000_000.0 * 100.0;
        Ok(t < chance)
    })?)?;

    // Input (stubs)
    for name in ["keyJustPressed", "keyPressed", "keyReleased"] {
        globals.set(name, lua.create_function(|_lua, _key: String| -> LuaResult<bool> {
            Ok(false)
        })?)?;
    }

    // getMidpointX/Y — returns center of sprite or game character
    globals.set("getMidpointX", lua.create_function(|lua, tag: String| -> LuaResult<f64> {
        let g = lua.globals();
        // Check game characters first (synced from PlayScreen)
        let char_key = format!("__midX_{}", tag);
        if let Ok(v) = g.get::<f64>(char_key) { return Ok(v); }
        // Fall back to Lua sprite
        let sprite_data: LuaTable = g.get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let x: f64 = tbl.get("x").unwrap_or(0.0);
            let tw: f64 = tbl.get("tex_w").unwrap_or(0.0);
            let sx: f64 = tbl.get("scale_x").unwrap_or(1.0);
            return Ok(x + tw * sx.abs() / 2.0);
        }
        Ok(0.0)
    })?)?;
    globals.set("getMidpointY", lua.create_function(|lua, tag: String| -> LuaResult<f64> {
        let g = lua.globals();
        let char_key = format!("__midY_{}", tag);
        if let Ok(v) = g.get::<f64>(char_key) { return Ok(v); }
        let sprite_data: LuaTable = g.get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let y: f64 = tbl.get("y").unwrap_or(0.0);
            let th: f64 = tbl.get("tex_h").unwrap_or(0.0);
            let sy: f64 = tbl.get("scale_y").unwrap_or(1.0);
            return Ok(y + th * sy.abs() / 2.0);
        }
        Ok(0.0)
    })?)?;
    globals.set("getGraphicMidpointX", lua.create_function(|lua, tag: String| -> LuaResult<f64> {
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let x: f64 = tbl.get("x").unwrap_or(0.0);
            let tw: f64 = tbl.get("tex_w").unwrap_or(0.0);
            let sx: f64 = tbl.get("scale_x").unwrap_or(1.0);
            return Ok(x + tw * sx.abs() / 2.0);
        }
        Ok(0.0)
    })?)?;
    globals.set("getGraphicMidpointY", lua.create_function(|lua, tag: String| -> LuaResult<f64> {
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let y: f64 = tbl.get("y").unwrap_or(0.0);
            let th: f64 = tbl.get("tex_h").unwrap_or(0.0);
            let sy: f64 = tbl.get("scale_y").unwrap_or(1.0);
            return Ok(y + th * sy.abs() / 2.0);
        }
        Ok(0.0)
    })?)?;

    // Character access
    globals.set("characterDance", lua.create_function(|lua, char_type: String| {
        let pending: LuaTable = lua.globals().get("__pending_props")?;
        let tbl = lua.create_table()?;
        tbl.set("prop", format!("__charDance.{}", char_type))?;
        tbl.set("value", true)?;
        pending.set(pending.len()? + 1, tbl)?;
        Ok(())
    })?)?;
    globals.set("getCharacterX", lua.create_function(|lua, typ: String| -> LuaResult<f64> {
        let key = match typ.to_lowercase().as_str() {
            "dad" | "opponent" | "1" => "__dad_x",
            "boyfriend" | "bf" | "0" => "__bf_x",
            "gf" | "girlfriend" | "2" => "__gf_x",
            _ => "__bf_x",
        };
        Ok(lua.globals().get::<f64>(key).unwrap_or(0.0))
    })?)?;
    globals.set("getCharacterY", lua.create_function(|lua, typ: String| -> LuaResult<f64> {
        let key = match typ.to_lowercase().as_str() {
            "dad" | "opponent" | "1" => "__dad_y",
            "boyfriend" | "bf" | "0" => "__bf_y",
            "gf" | "girlfriend" | "2" => "__gf_y",
            _ => "__bf_y",
        };
        Ok(lua.globals().get::<f64>(key).unwrap_or(0.0))
    })?)?;
    globals.set("setCharacterX", lua.create_function(|lua, (typ, val): (String, f64)| {
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
    })?)?;
    globals.set("setCharacterY", lua.create_function(|lua, (typ, val): (String, f64)| {
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
    })?)?;

    // Health bar
    globals.set("setHealthBarColors", lua.create_function(|lua, (left, right): (LuaValue, LuaValue)| {
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
    })?)?;
    globals.set("setTimeBarColors", lua.create_function(|_lua, (_left, _right): (LuaValue, LuaValue)| {
        Ok(())
    })?)?;

    // Health/score control — use __health global synced from game
    globals.set("__health", 1.0)?;
    globals.set("getHealth", lua.create_function(|lua, ()| -> LuaResult<f64> {
        Ok(lua.globals().get::<f64>("__health").unwrap_or(1.0))
    })?)?;
    globals.set("setHealth", lua.create_function(|lua, v: f64| {
        let g = lua.globals();
        g.set("__health", v)?;
        let pending: LuaTable = g.get("__pending_props")?;
        let tbl = lua.create_table()?;
        tbl.set("prop", "health")?;
        tbl.set("value", v)?;
        let len = pending.len()? as i64;
        pending.set(len + 1, tbl)?;
        Ok(())
    })?)?;
    globals.set("addHealth", lua.create_function(|lua, v: f64| {
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
    })?)?;
    globals.set("addScore", lua.create_function(|_lua, _v: i32| { Ok(()) })?)?;
    globals.set("setScore", lua.create_function(|_lua, _v: i32| { Ok(()) })?)?;
    globals.set("addMisses", lua.create_function(|_lua, _v: i32| { Ok(()) })?)?;
    globals.set("setMisses", lua.create_function(|_lua, _v: i32| { Ok(()) })?)?;
    globals.set("addHits", lua.create_function(|_lua, _v: i32| { Ok(()) })?)?;
    globals.set("setHits", lua.create_function(|_lua, _v: i32| { Ok(()) })?)?;
    globals.set("setRatingPercent", lua.create_function(|_lua, _v: f64| { Ok(()) })?)?;
    globals.set("setRatingName", lua.create_function(|_lua, _v: String| { Ok(()) })?)?;
    globals.set("setRatingFC", lua.create_function(|_lua, _v: String| { Ok(()) })?)?;
    globals.set("updateScoreText", lua.create_function(|_lua, ()| { Ok(()) })?)?;

    // cameraShake(camera, intensity, duration)
    globals.set("cameraShake", lua.create_function(|lua, (cam, intensity, duration): (String, f64, f64)| {
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
    })?)?;

    // cameraFlash(camera, color, duration, forced)
    globals.set("cameraFlash", lua.create_function(|lua, (cam, color, duration, _forced): (String, Option<String>, Option<f64>, Option<bool>)| {
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
    })?)?;

    globals.set("cameraFade", lua.create_function(|lua, (cam, color, duration, fade_in): (String, Option<String>, Option<f64>, Option<bool>)| {
        let pending: LuaTable = lua.globals().get("__pending_cam_fx")?;
        let tbl = lua.create_table()?;
        tbl.set("kind", "flash")?;
        let cam_name = match cam.to_lowercase().as_str() {
            "camhud" | "hud" => "camHUD",
            "camgame" | "game" => "camGame",
            _ => &cam,
        };
        tbl.set("camera", cam_name)?;
        tbl.set("color", color.unwrap_or_else(|| "000000".to_string()))?;
        tbl.set("duration", duration.unwrap_or(0.5))?;
        tbl.set("alpha", if fade_in.unwrap_or(false) { 0.0 } else { 1.0 })?;
        let len = pending.len()? as i64;
        pending.set(len + 1, tbl)?;
        Ok(())
    })?)?;

    // setSubtitle(text, font, color, size, duration, borderColor)
    globals.set("setSubtitle", lua.create_function(|lua, args: LuaMultiValue| {
        let text: String = args.get(0).and_then(|v| match v { LuaValue::String(s) => Some(s.to_string_lossy().to_string()), _ => None }).unwrap_or_default();
        let font: String = args.get(1).and_then(|v| match v { LuaValue::String(s) => Some(s.to_string_lossy().to_string()), _ => None }).unwrap_or_default();
        let color: String = args.get(2).and_then(|v| match v { LuaValue::String(s) => Some(s.to_string_lossy().to_string()), _ => None }).unwrap_or_else(|| "0xFFFFFFFF".to_string());
        let size: f64 = args.get(3).and_then(|v| match v { LuaValue::Number(n) => Some(*n), LuaValue::Integer(n) => Some(*n as f64), _ => None }).unwrap_or(32.0);
        let duration: f64 = args.get(4).and_then(|v| match v { LuaValue::Number(n) => Some(*n), LuaValue::Integer(n) => Some(*n as f64), _ => None }).unwrap_or(3.0);
        let border: String = args.get(5).and_then(|v| match v { LuaValue::String(s) => Some(s.to_string_lossy().to_string()), _ => None }).unwrap_or_else(|| "0x00000000".to_string());

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
    })?)?;

    // customFlash(camera, color, duration, options)
    globals.set("customFlash", lua.create_function(|lua, (cam, color, duration, options): (String, Option<String>, Option<f64>, Option<LuaValue>)| {
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
    })?)?;

    // customFade(camera, color, duration, options)
    globals.set("customFade", lua.create_function(|_lua, _args: LuaMultiValue| -> LuaResult<()> {
        Ok(())
    })?)?;

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
    globals.set("doTweenColor", lua.create_function(|lua, (tag, target, color, duration, ease): (String, String, String, f64, Option<String>)| {
        // Parse target color from hex
        let hex = color.trim_start_matches('#').trim_start_matches("0x").trim_start_matches("0X");
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
        push_tween(lua, &format!("{}_r", tag), &resolved, "red_offset", r_off, duration, ease.as_deref())?;
        push_tween(lua, &format!("{}_g", tag), &resolved, "green_offset", g_off, duration, ease.as_deref())?;
        push_tween(lua, &format!("{}_b", tag), &resolved, "blue_offset", b_off, duration, ease.as_deref())?;
        Ok(())
    })?)?;

    // startTween(tag, target, values_table, duration, options)
    // Generic tween function — tweens multiple properties on a game object.
    // target can be: "strumLineNotes.members[N]", a Lua sprite tag, or a game object path.
    globals.set("startTween", lua.create_function(|lua, (tag, target, values, duration, options): (String, String, LuaTable, f64, Option<mlua::Value>)| {
        let g = lua.globals();
        // Parse ease from options (string or table with ease key)
        let ease = match &options {
            Some(mlua::Value::String(s)) => s.to_string_lossy().to_string(),
            Some(mlua::Value::Table(tbl)) => {
                tbl.get::<mlua::prelude::LuaString>("ease")
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|_| "linear".to_string())
            }
            _ => "linear".to_string(),
        };
        // Parse onComplete callback name from options
        let on_complete = match &options {
            Some(mlua::Value::Table(tbl)) => {
                tbl.get::<mlua::prelude::LuaString>("onComplete")
                    .map(|s| s.to_string_lossy().to_string())
                    .ok()
            }
            _ => None,
        };

        // Handle .colorTransform suffix: "scythe.colorTransform" → target "scythe", color context
        let (resolved_target, is_color_transform) = if let Some(prefix) = target.strip_suffix(".colorTransform") {
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
            let Ok((prop_name, end_val)) = pair else { continue };
            let prop = if is_color_transform {
                match prop_name.as_str() {
                    "redOffset" => "red_offset",
                    "greenOffset" => "green_offset",
                    "blueOffset" => "blue_offset",
                    _ => {
                        log::debug!("startTween: ignoring unknown colorTransform property '{}'", prop_name);
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
                        log::debug!("startTween: ignoring unknown property '{}' on '{}'", prop_name, target);
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
    })?)?;

    // noteTweenX/Y/Alpha/Angle/Direction — tween strum note properties
    for name in [
        "noteTweenX", "noteTweenY", "noteTweenAlpha",
        "noteTweenAngle", "noteTweenDirection",
    ] {
        let prop = match name {
            "noteTweenX" => "x",
            "noteTweenY" => "y",
            "noteTweenAlpha" => "alpha",
            "noteTweenAngle" => "angle",
            _ => "x", // direction maps to x for now
        };
        let prop_owned = prop.to_string();
        globals.set(name, lua.create_function(move |lua, (tag, note, value, duration, ease): (String, i32, f64, f64, Option<String>)| {
            // note index: 0-3 = opponent, 4-7 = player
            let strum_tag = if note < 4 {
                format!("__strum_opponent_{}", note)
            } else {
                format!("__strum_player_{}", note - 4)
            };
            push_tween(lua, &tag, &strum_tag, &prop_owned, value, duration, ease.as_deref())
        })?)?;
    }

    globals.set("cancelTween", lua.create_function(|lua, tag: String| {
        let cancels: LuaTable = lua.globals().get("__pending_tween_cancels")?;
        let len = cancels.len()? as i64;
        cancels.set(len + 1, tag)?;
        Ok(())
    })?)?;

    // runTimer(tag, duration, loops)
    globals.set("runTimer", lua.create_function(|lua, (tag, duration, loops): (String, f64, Option<i32>)| {
        let timers: LuaTable = lua.globals().get("__pending_timers")?;
        let tbl = lua.create_table()?;
        tbl.set("tag", tag)?;
        tbl.set("duration", duration)?;
        tbl.set("loops", loops.unwrap_or(1))?;
        let len = timers.len()? as i64;
        timers.set(len + 1, tbl)?;
        Ok(())
    })?)?;

    globals.set("cancelTimer", lua.create_function(|lua, tag: String| {
        let cancels: LuaTable = lua.globals().get("__pending_timer_cancels")?;
        let len = cancels.len()? as i64;
        cancels.set(len + 1, tag)?;
        Ok(())
    })?)?;

    Ok(())
}

fn push_tween(lua: &Lua, tag: &str, target: &str, property: &str, value: f64, duration: f64, ease: Option<&str>) -> LuaResult<()> {
    let tweens: LuaTable = lua.globals().get("__pending_tweens")?;
    let tbl = lua.create_table()?;
    tbl.set("tag", tag.to_string())?;
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

    globals.set("playSound", lua.create_function(|_lua, _args: LuaMultiValue| { Ok(()) })?)?;
    globals.set("playMusic", lua.create_function(|lua, (path, volume, looping): (String, Option<f64>, Option<bool>)| {
        let pending: LuaTable = lua.globals().get("__pending_audio")?;
        let tbl = lua.create_table()?;
        tbl.set("kind", "play_music")?;
        tbl.set("path", path)?;
        tbl.set("volume", volume.unwrap_or(1.0))?;
        tbl.set("looping", looping.unwrap_or(true))?;
        pending.set(pending.len()? + 1, tbl)?;
        lua.globals().set("__music_volume", volume.unwrap_or(1.0))?;
        Ok(())
    })?)?;
    globals.set("stopSound", lua.create_function(|lua, _tag: Option<String>| {
        let pending: LuaTable = lua.globals().get("__pending_audio")?;
        let tbl = lua.create_table()?;
        tbl.set("kind", "stop_music")?;
        pending.set(pending.len()? + 1, tbl)?;
        Ok(())
    })?)?;
    globals.set("pauseSound", lua.create_function(|lua, _tag: Option<String>| {
        let pending: LuaTable = lua.globals().get("__pending_audio")?;
        let tbl = lua.create_table()?;
        tbl.set("kind", "pause_music")?;
        pending.set(pending.len()? + 1, tbl)?;
        Ok(())
    })?)?;
    globals.set("pauseSounds", lua.create_function(|lua, _tag: Option<String>| {
        let pending: LuaTable = lua.globals().get("__pending_audio")?;
        let tbl = lua.create_table()?;
        tbl.set("kind", "pause_music")?;
        pending.set(pending.len()? + 1, tbl)?;
        Ok(())
    })?)?;
    globals.set("resumeSound", lua.create_function(|lua, _tag: Option<String>| {
        let pending: LuaTable = lua.globals().get("__pending_audio")?;
        let tbl = lua.create_table()?;
        tbl.set("kind", "resume_music")?;
        pending.set(pending.len()? + 1, tbl)?;
        Ok(())
    })?)?;
    globals.set("resumeSounds", lua.create_function(|lua, _tag: Option<String>| {
        let pending: LuaTable = lua.globals().get("__pending_audio")?;
        let tbl = lua.create_table()?;
        tbl.set("kind", "resume_music")?;
        pending.set(pending.len()? + 1, tbl)?;
        Ok(())
    })?)?;
    globals.set("soundFadeIn", lua.create_function(|_lua, _args: LuaMultiValue| { Ok(()) })?)?;
    globals.set("soundFadeOut", lua.create_function(|_lua, _args: LuaMultiValue| { Ok(()) })?)?;
    globals.set("soundFadeCancel", lua.create_function(|_lua, _tag: Option<String>| { Ok(()) })?)?;
    globals.set("getSoundVolume", lua.create_function(|lua, _tag: Option<String>| -> LuaResult<f64> {
        Ok(lua.globals().get::<f64>("__music_volume").unwrap_or(1.0))
    })?)?;
    globals.set("setSoundVolume", lua.create_function(|lua, (_tag, vol): (Option<String>, f64)| {
        let pending: LuaTable = lua.globals().get("__pending_audio")?;
        let tbl = lua.create_table()?;
        tbl.set("kind", "set_music_volume")?;
        tbl.set("volume", vol)?;
        pending.set(pending.len()? + 1, tbl)?;
        lua.globals().set("__music_volume", vol)?;
        Ok(())
    })?)?;
    globals.set("getSoundTime", lua.create_function(|lua, _tag: Option<String>| -> LuaResult<f64> {
        Ok(lua.globals().get::<f64>("__music_time").unwrap_or(0.0))
    })?)?;
    globals.set("setSoundTime", lua.create_function(|lua, (_tag, time): (Option<String>, f64)| {
        let pending: LuaTable = lua.globals().get("__pending_audio")?;
        let tbl = lua.create_table()?;
        tbl.set("kind", "set_music_time")?;
        tbl.set("time", time)?;
        pending.set(pending.len()? + 1, tbl)?;
        lua.globals().set("__music_time", time)?;
        Ok(())
    })?)?;
    globals.set("luaSoundExists", lua.create_function(|lua, _tag: String| -> LuaResult<bool> {
        Ok(lua.globals().get::<f64>("__music_volume").unwrap_or(0.0) >= 0.0)
    })?)?;

    Ok(())
}

fn register_window_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();
    globals.set("getScreenWidth", lua.create_function(|lua, ()| -> LuaResult<i32> {
        Ok(lua.globals().get::<i32>("screenWidth").unwrap_or(1280))
    })?)?;
    globals.set("getScreenHeight", lua.create_function(|lua, ()| -> LuaResult<i32> {
        Ok(lua.globals().get::<i32>("screenHeight").unwrap_or(720))
    })?)?;
    Ok(())
}

fn register_text_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // makeLuaText(tag, text, width, x, y)
    globals.set("makeLuaText", lua.create_function(|lua, (tag, text, width, x, y): (String, Option<String>, Option<f64>, Option<f64>, Option<f64>)| {
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
    })?)?;

    // addLuaText(tag, inFront)
    globals.set("addLuaText", lua.create_function(|lua, (tag, in_front): (String, Option<bool>)| {
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
    })?)?;

    globals.set("removeLuaText", lua.create_function(|lua, (tag, _destroy): (String, Option<bool>)| {
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        if let Ok(tbl) = text_data.get::<LuaTable>(tag.clone()) {
            tbl.set("visible", false)?;
        }
        // Reuse sprite remove queue
        let pending: LuaTable = lua.globals().get("__pending_removes")?;
        let len = pending.len()? as i64;
        pending.set(len + 1, format!("__text_{}", tag))?;
        Ok(())
    })?)?;

    globals.set("setTextString", lua.create_function(|lua, (tag, text): (String, String)| {
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
            tbl.set("text", text)?;
        }
        Ok(())
    })?)?;

    globals.set("setTextSize", lua.create_function(|lua, (tag, size): (String, f64)| {
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
            tbl.set("size", size as f32)?;
        }
        Ok(())
    })?)?;

    globals.set("setTextColor", lua.create_function(|lua, (tag, color): (String, String)| {
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
            tbl.set("color", color)?;
        }
        Ok(())
    })?)?;

    globals.set("setTextFont", lua.create_function(|lua, (tag, font): (String, String)| {
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
            tbl.set("font", font)?;
        }
        Ok(())
    })?)?;

    globals.set("setTextBorder", lua.create_function(|lua, args: LuaMultiValue| {
        let tag: String = args.get(0).and_then(|v| match v { LuaValue::String(s) => Some(s.to_string_lossy().to_string()), _ => None }).unwrap_or_default();
        let size: f32 = args.get(1).and_then(|v| match v { LuaValue::Number(n) => Some(*n as f32), LuaValue::Integer(n) => Some(*n as f32), _ => None }).unwrap_or(0.0);
        let color: String = args.get(2).and_then(|v| match v { LuaValue::String(s) => Some(s.to_string_lossy().to_string()), _ => None }).unwrap_or_else(|| "000000".to_string());
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
            tbl.set("border_size", size)?;
            tbl.set("border_color", color)?;
        }
        Ok(())
    })?)?;

    globals.set("setTextAlignment", lua.create_function(|lua, (tag, align): (String, String)| {
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
            tbl.set("alignment", align)?;
        }
        Ok(())
    })?)?;

    globals.set("setTextWidth", lua.create_function(|lua, (tag, w): (String, f64)| {
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
            tbl.set("width", w as f32)?;
        }
        Ok(())
    })?)?;

    globals.set("setTextAutoSize", lua.create_function(|_lua, (_tag, _v): (String, bool)| { Ok(()) })?)?;

    globals.set("getTextString", lua.create_function(|lua, tag: String| -> LuaResult<String> {
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        if let Ok(tbl) = text_data.get::<LuaTable>(tag) {
            return Ok(tbl.get::<String>("text").unwrap_or_default());
        }
        Ok(String::new())
    })?)?;

    globals.set("luaTextExists", lua.create_function(|lua, tag: String| -> LuaResult<bool> {
        let text_data: LuaTable = lua.globals().get("__text_data")?;
        Ok(text_data.contains_key(tag)?)
    })?)?;

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

    globals.set("registerNoteType", lua.create_function(|lua, (name, config): (String, LuaTable)| {
        let pending: LuaTable = lua.globals().get("__pending_note_types")?;

        let entry = lua.create_table()?;
        entry.set("name", name.clone())?;
        // Store all fields as-is; the app layer will parse them into NoteTypeConfig
        entry.set("hitCausesMiss", config.get::<bool>("hitCausesMiss").unwrap_or(false))?;
        entry.set("hitDamage", config.get::<f64>("hitDamage").unwrap_or(0.0))?;
        entry.set("ignoreMiss", config.get::<bool>("ignoreMiss").unwrap_or(false))?;
        if let Ok(v) = config.get::<String>("noteSkin") { entry.set("noteSkin", v)?; }
        if let Ok(v) = config.get::<String>("hitSfx") { entry.set("hitSfx", v)?; }
        if let Ok(v) = config.get::<f64>("healthDrainPct") { entry.set("healthDrainPct", v)?; }
        if let Ok(v) = config.get::<bool>("drainDeathSafe") { entry.set("drainDeathSafe", v)?; }

        // Animation arrays: store as sub-tables
        for &key in &["noteAnims", "strumAnims", "confirmAnims"] {
            if let Ok(tbl) = config.get::<LuaTable>(key) {
                let arr = lua.create_table()?;
                for i in 1..=4 {
                    if let Ok(v) = tbl.get::<String>(i) { arr.set(i, v)?; }
                }
                entry.set(key, arr)?;
            }
        }

        let len = pending.len()? as i64;
        pending.set(len + 1, entry)?;
        log::info!("Queued note type registration '{}' from Lua", name);
        Ok(())
    })?)?;

    // addLuaScript(name)
    lua.globals().set("addLuaScript", lua.create_function(|lua, name: String| {
        let pending: LuaTable = lua.globals().get("__pending_props")?;
        let entry = lua.create_table()?;
        entry.set("type", "add_script")?;
        entry.set("script_name", name)?;
        let len = pending.len()? + 1;
        pending.set(len, entry)?;
        Ok(())
    })?)?;

    Ok(())
}

/// Register no-op stubs for all remaining Psych Engine functions that scripts may call.
fn register_noop_stubs(lua: &Lua) -> LuaResult<()> {
    // getDataFromSave(saveName, key, ?default) — returns the default if not found.
    // Critical for mod compatibility: many scripts pass a default value and branch on it.
    lua.globals().set("getDataFromSave", lua.create_function(|_lua, args: LuaMultiValue| -> LuaResult<LuaValue> {
        let mut args_iter = args.into_iter();
        let _save_name = args_iter.next(); // save name — ignored
        let _key = args_iter.next();        // key — ignored
        // Return the third argument (default value) if provided, otherwise false
        match args_iter.next() {
            Some(val) => Ok(val),
            None => Ok(LuaValue::Boolean(false)),
        }
    })?)?;
    // startVideo(filename, callback) — queue mid-song video playback
    // Pauses gameplay, plays video, then resumes on finish
    lua.globals().set("startVideo", lua.create_function(|lua, (filename, callback): (String, Option<String>)| {
        let pending: LuaTable = lua.globals().get("__pending_props")?;
        let entry = lua.create_table()?;
        entry.set("type", "video")?;
        entry.set("filename", filename)?;
        if let Some(ref cb) = callback {
            entry.set("callback", cb.as_str())?;
        }
        let len = pending.len()? + 1;
        pending.set(len, entry)?;
        Ok(())
    })?)?;
    let noop = lua.create_function(|_lua, _args: LuaMultiValue| -> LuaResult<LuaValue> {
        Ok(LuaNil)
    })?;

    for name in [
        // Haxe integration
        "runHaxeCodePost", "addHaxeLibrary",
        // Script management
        "addLuaScript", "removeLuaScript", "addHScript", "removeHScript",
        "isRunning", "callScript", "getRunningScripts",
        "callOnLuas", "callOnScripts", "callOnHScript",
        "setOnLuas", "setOnScripts", "setOnHScript",
        "createCallback", "createGlobalCallback",
        // Song control
        "startCountdown", "endSong", "exitSong", "restartSong",
        "loadSong", "startDialogue",
        // Precaching
        "precacheImage", "precacheSound", "precacheMusic",
        // Camera
        "cameraFade",
        "setCameraScroll", "setCameraFollowPoint",
        "addCameraScroll", "addCameraFollowPoint",
        "getCameraScrollX", "getCameraScrollY",
        "getCameraFollowX", "getCameraFollowY",
        "getMouseX", "getMouseY",
        // Character management
        "addCharacterToList",
        // Reflection
        "callMethod", "callMethodFromClass",
        "createInstance", "addInstance", "instanceArg",
        "addToGroup", "removeFromGroup",
        "objectsOverlap",
        // Position queries
        "getScreenPositionX", "getScreenPositionY",
        // Strum/note
        "getObjectOrder",
        // Keyboard/gamepad (beyond the basic ones)
        "keyboardJustPressed", "keyboardPressed", "keyboardReleased",
        "mouseClicked", "mousePressed", "mouseReleased",
        "anyGamepadJustPressed", "anyGamepadPressed", "anyGamepadReleased",
        "gamepadJustPressed", "gamepadPressed", "gamepadReleased",
        "gamepadAnalogX", "gamepadAnalogY",
        // Save data
        "initSaveData", "flushSaveData", "setDataFromSave", "eraseSaveData",
        // Presence / pause
        "changePresence", "isPaused",
        // File I/O
        "checkFileExists", "getTextFromFile", "saveFile", "deleteFile", "directoryFileList",
        // Shader
        "initLuaShader", "setSpriteShader", "removeSpriteShader",
        "getShaderBool", "setShaderBool", "getShaderBoolArray", "setShaderBoolArray",
        "getShaderInt", "setShaderInt", "getShaderIntArray", "setShaderIntArray",
        "getShaderFloat", "setShaderFloat", "getShaderFloatArray", "setShaderFloatArray",
        "setShaderSampler2D",
        // Misc
        "getColorFromString", "getColorFromName",
        "setHudVisible",
        "addShake",
        // Substates
        "openCustomSubstate", "closeCustomSubstate",
        "addLuaSpriteSubstate", "removeLuaSpriteSubstate",
        "addLuaTextSubstate", "removeLuaTextSubstate",
        "insertToCustomSubstate",
        // Shader (runtime)
        "createRuntimeShader",
        // Text queries
        "getTextFont", "getTextSize", "getTextWidth",
        // Sound
        "getSoundPitch", "setSoundPitch",
        // Pixel
        "getPixelColor",
        // Atlas
        "loadAnimateAtlas", "loadFrames", "loadMultipleFrames",
        "makeFlxAnimateSprite",
        "addAnimationBySymbol", "addAnimationBySymbolIndices",
        // Deprecated aliases (Psych Engine DeprecatedFunctions.hx)
        "luaSpriteMakeGraphic", "luaSpriteAddAnimationByPrefix",
        "luaSpriteAddAnimationByIndices", "luaSpritePlayAnimation",
        "setLuaSpriteCamera", "setLuaSpriteScrollFactor",
        "scaleLuaSprite", "getPropertyLuaSprite", "setPropertyLuaSprite",
        "musicFadeIn", "musicFadeOut",
        // Timers
        "onTweenCompleted", "onTimerCompleted", "onSoundFinished",
    ] {
        // Only set if not already registered (avoid overwriting real implementations)
        if lua.globals().get::<LuaValue>(name)? == LuaNil {
            lua.globals().set(name, noop.clone())?;
        }
    }

    // === Rustic extensions: stage overlay, post-processing, health bar ===

    // setStageColor(side, hexColor, duration)
    // side: "left", "right", or "both"
    let set_stage_color = lua.create_function(|lua, (side, hex, dur): (String, String, Option<f64>)| {
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
    let set_hb_color = lua.create_function(|lua, (side, hex, dur): (String, String, Option<f64>)| {
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

/// Parse a hex color string to (r, g, b, a) as 0..1 floats.
fn parse_hex_rgba(hex: &str) -> (f32, f32, f32, f32) {
    let hex = if hex.len() > 6 { &hex[hex.len()-6..] } else { hex };
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
        _ => None,
    }
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
    let end_str: String = remaining.chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let end_val: f64 = end_str.parse().ok()?;

    // Skip past end value, parse duration
    let remaining = &remaining[end_str.len()..];
    let remaining = remaining.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
    let dur_str: String = remaining.chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let duration: f64 = dur_str.parse().ok()?;

    // Find ease function name from FlxEase.X
    let ease = if let Some(idx) = code.find("FlxEase.") {
        let start = idx + "FlxEase.".len();
        code[start..].chars()
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
                                    if depth == 0 { return Some(i); }
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
        if line.starts_with("//") { continue; }
        // Only parse lines NOT inside the switch block (simple heuristic: no "case" indentation)
        // Actually, let's just parse all direct assignments that are outside switch context
        // The switch handler above already handles those; here we handle standalone lines
        for (prefix, char_name) in &[
            ("game.dad.x", "dad"), ("game.dad.y", "dad"),
            ("game.gf.x", "gf"), ("game.gf.y", "gf"),
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
    let mut offsets: std::collections::HashMap<(&str, &str), f64> = std::collections::HashMap::new();

    for line in body.lines() {
        let line = line.trim();
        if line.starts_with("//") { continue; }

        for (prefix, char_name, field) in &[
            ("game.boyfriend.x", "boyfriend", "x"),
            ("game.boyfriend.y", "boyfriend", "y"),
            ("game.dad.x", "dad", "x"),
            ("game.dad.y", "dad", "y"),
            ("game.gf.x", "gf", "x"),
            ("game.gf.y", "gf", "y"),
        ] {
            if !line.starts_with(prefix) { continue; }
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
