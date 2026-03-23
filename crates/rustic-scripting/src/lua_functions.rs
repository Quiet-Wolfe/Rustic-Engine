use std::io::Read as _;

use mlua::prelude::*;

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
    g.set("__sprite_data", lua.create_table()?)?;
    g.set("__script_closed", false)?;
    g.set("__custom_vars", lua.create_table()?)?;

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
    g.set("version", "0.7.3")?;
    g.set("luaDebugMode", false)?;
    g.set("luaDeprecatedWarnings", false)?;
    g.set("buildTarget", "linux")?;

    register_sprite_functions(lua)?;
    register_property_functions(lua)?;
    register_utility_functions(lua)?;
    register_tween_functions(lua)?;
    register_sound_functions(lua)?;
    register_text_functions(lua)?;
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
    globals.set("removeLuaSprite", lua.create_function(|_lua, (_tag, _destroy): (String, Option<bool>)| {
        Ok(())
    })?)?;

    // scaleObject(tag, scaleX, scaleY)
    globals.set("scaleObject", lua.create_function(|lua, (tag, sx, sy): (String, f64, f64)| {
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            tbl.set("scale_x", sx as f32)?;
            tbl.set("scale_y", sy as f32)?;
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
    globals.set("setObjectOrder", lua.create_function(|_lua, (_tag, _order): (String, i32)| {
        Ok(())
    })?)?;

    // getObjectOrder(tag)
    globals.set("getObjectOrder", lua.create_function(|_lua, _tag: String| -> LuaResult<i32> {
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
    globals.set("addAnimationByIndices", lua.create_function(|lua, (tag, anim, prefix, indices, fps, looping): (String, String, String, LuaTable, Option<f64>, Option<bool>)| {
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
            // Store indices as a Lua table
            anim_tbl.set("indices", indices)?;
            anims.set(anim, anim_tbl)?;
        }
        Ok(())
    })?)?;

    // addAnimation(tag, anim, frames, fps, looping) — same as addAnimationByPrefix for our purposes
    globals.set("addAnimation", lua.create_function(|lua, (tag, anim, prefix, fps, looping): (String, String, String, Option<f64>, Option<bool>)| {
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

    // screenCenter(tag, axis)
    globals.set("screenCenter", lua.create_function(|lua, (tag, axis): (String, Option<String>)| {
        let axis = axis.unwrap_or_else(|| "xy".to_string());
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let tex_w: f32 = tbl.get("tex_w").unwrap_or(0.0);
            let tex_h: f32 = tbl.get("tex_h").unwrap_or(0.0);
            let sx: f32 = tbl.get("scale_x").unwrap_or(1.0);
            let sy: f32 = tbl.get("scale_y").unwrap_or(1.0);
            let sw: f32 = lua.globals().get("screenWidth").unwrap_or(1280.0);
            let sh: f32 = lua.globals().get("screenHeight").unwrap_or(720.0);
            if axis.contains('x') || axis.contains('X') {
                tbl.set("x", (sw - tex_w * sx.abs()) / 2.0)?;
            }
            if axis.contains('y') || axis.contains('Y') {
                tbl.set("y", (sh - tex_h * sy.abs()) / 2.0)?;
            }
        }
        Ok(())
    })?)?;

    // setObjectCamera(tag, camera)
    globals.set("setObjectCamera", lua.create_function(|_lua, (_tag, _cam): (String, Option<String>)| {
        Ok(())
    })?)?;

    // luaSpriteExists(tag)
    globals.set("luaSpriteExists", lua.create_function(|lua, tag: String| -> LuaResult<bool> {
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        Ok(sprite_data.contains_key(tag)?)
    })?)?;

    // setBlendMode(tag, blend)
    globals.set("setBlendMode", lua.create_function(|_lua, (_tag, _blend): (String, Option<String>)| {
        Ok(())
    })?)?;

    // loadGraphic(tag, image)
    globals.set("loadGraphic", lua.create_function(|_lua, (_tag, _image): (String, Option<String>)| {
        Ok(())
    })?)?;

    Ok(())
}

fn register_property_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // setProperty(property, value)
    globals.set("setProperty", lua.create_function(|lua, (prop, value): (String, LuaValue)| {
        // Check if it's a sprite property (tag.field)
        if let Some(dot_pos) = prop.find('.') {
            let tag = &prop[..dot_pos];
            let field = &prop[dot_pos + 1..];
            let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
            if let Ok(tbl) = sprite_data.get::<LuaTable>(tag.to_string()) {
                match field {
                    "alpha" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("alpha", n)?; } }
                    "visible" => { if let LuaValue::Boolean(b) = &value { tbl.set("visible", *b)?; } }
                    "flipX" | "flip_x" => { if let LuaValue::Boolean(b) = &value { tbl.set("flip_x", *b)?; } }
                    "antialiasing" => { if let LuaValue::Boolean(b) = &value { tbl.set("antialiasing", *b)?; } }
                    "x" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("x", n)?; } }
                    "y" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("y", n)?; } }
                    "scale.x" | "scaleX" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("scale_x", n)?; } }
                    "scale.y" | "scaleY" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("scale_y", n)?; } }
                    "scrollFactor.x" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("scroll_x", n)?; } }
                    "scrollFactor.y" => { if let Some(n) = lua_val_to_f32(&value) { tbl.set("scroll_y", n)?; } }
                    _ => { tbl.set(format!("_prop_{field}"), value.clone())?; }
                }
                return Ok(());
            }
        }

        // Update Lua global if it's a known property (so getProperty reads back the latest value)
        match prop.as_str() {
            "defaultCamZoom" | "cameraSpeed" => { lua.globals().set(prop.as_str(), value.clone()).ok(); }
            _ => {}
        }

        // Queue as a game property write
        let pending: LuaTable = lua.globals().get("__pending_props")?;
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
                    "flipX" => Ok(tbl.get::<LuaValue>("flip_x").unwrap_or(LuaValue::Boolean(false))),
                    "flipY" => Ok(tbl.get::<LuaValue>("flip_y").unwrap_or(LuaValue::Boolean(false))),
                    "scale.x" | "scaleX" => Ok(tbl.get::<LuaValue>("scale_x").unwrap_or(LuaValue::Number(1.0))),
                    "scale.y" | "scaleY" => Ok(tbl.get::<LuaValue>("scale_y").unwrap_or(LuaValue::Number(1.0))),
                    "scrollFactor.x" => Ok(tbl.get::<LuaValue>("scroll_x").unwrap_or(LuaValue::Number(1.0))),
                    "scrollFactor.y" => Ok(tbl.get::<LuaValue>("scroll_y").unwrap_or(LuaValue::Number(1.0))),
                    "antialiasing" => Ok(tbl.get::<LuaValue>("antialiasing").unwrap_or(LuaValue::Boolean(true))),
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
                    _ => Ok(tbl.get::<LuaValue>(format!("_prop_{field}")).unwrap_or(LuaNil)),
                };
            }
        }
        // Known game properties — read from globals (which Lua scripts may have set)
        match prop.as_str() {
            "defaultCamZoom" => Ok(lua.globals().get::<LuaValue>("defaultCamZoom").unwrap_or(LuaValue::Number(0.9))),
            "cameraSpeed" => Ok(lua.globals().get::<LuaValue>("cameraSpeed").unwrap_or(LuaValue::Number(1.0))),
            "healthGainMult" => Ok(LuaValue::Number(1.0)),
            "healthLossMult" => Ok(LuaValue::Number(1.0)),
            "playbackRate" => Ok(LuaValue::Number(1.0)),
            _ => Ok(LuaNil),
        }
    })?)?;

    // getPropertyFromGroup(group, index, field)
    globals.set("getPropertyFromGroup", lua.create_function(|_lua, (_group, _idx, _field): (String, i32, String)| -> LuaResult<LuaValue> {
        Ok(LuaNil)
    })?)?;

    // setPropertyFromGroup(group, index, field, value)
    globals.set("setPropertyFromGroup", lua.create_function(|_lua, (_group, _idx, _field, _value): (String, i32, String, LuaValue)| {
        Ok(())
    })?)?;

    // getPropertyFromClass
    globals.set("getPropertyFromClass", lua.create_function(|_lua, (_class, _var): (String, String)| -> LuaResult<LuaValue> {
        Ok(LuaNil)
    })?)?;

    // setPropertyFromClass
    globals.set("setPropertyFromClass", lua.create_function(|_lua, (_class, _var, _val): (String, String, LuaValue)| {
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

    // runHaxeCode / runHaxeFunction — no-op
    globals.set("runHaxeCode", lua.create_function(|_lua, _code: String| -> LuaResult<LuaValue> {
        Ok(LuaNil)
    })?)?;

    // getSongPosition()
    globals.set("getSongPosition", lua.create_function(|_lua, ()| -> LuaResult<f64> {
        Ok(0.0) // TODO: wire to conductor
    })?)?;

    // cameraSetTarget(target)
    globals.set("cameraSetTarget", lua.create_function(|_lua, _target: String| {
        Ok(())
    })?)?;

    // triggerEvent(name, v1, v2)
    globals.set("triggerEvent", lua.create_function(|_lua, (_name, _v1, _v2): (String, Option<String>, Option<String>)| {
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
    globals.set("stringStartsWith", lua.create_function(|_lua, (s, prefix): (String, String)| -> LuaResult<bool> {
        Ok(s.starts_with(&prefix))
    })?)?;

    globals.set("stringEndsWith", lua.create_function(|_lua, (s, suffix): (String, String)| -> LuaResult<bool> {
        Ok(s.ends_with(&suffix))
    })?)?;

    globals.set("stringSplit", lua.create_function(|lua, (s, sep): (String, String)| {
        let parts: Vec<String> = s.split(&sep).map(|p| p.to_string()).collect();
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

    // getMidpointX/Y — returns center of sprite (x + width/2)
    globals.set("getMidpointX", lua.create_function(|lua, tag: String| -> LuaResult<f64> {
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
        if let Ok(tbl) = sprite_data.get::<LuaTable>(tag) {
            let x: f64 = tbl.get("x").unwrap_or(0.0);
            let tw: f64 = tbl.get("tex_w").unwrap_or(0.0);
            let sx: f64 = tbl.get("scale_x").unwrap_or(1.0);
            return Ok(x + tw * sx.abs() / 2.0);
        }
        Ok(0.0)
    })?)?;
    globals.set("getMidpointY", lua.create_function(|lua, tag: String| -> LuaResult<f64> {
        let sprite_data: LuaTable = lua.globals().get("__sprite_data")?;
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
    globals.set("characterDance", lua.create_function(|_lua, _char: String| { Ok(()) })?)?;
    globals.set("getCharacterX", lua.create_function(|_lua, _typ: String| -> LuaResult<f64> { Ok(0.0) })?)?;
    globals.set("getCharacterY", lua.create_function(|_lua, _typ: String| -> LuaResult<f64> { Ok(0.0) })?)?;
    globals.set("setCharacterX", lua.create_function(|_lua, (_typ, _val): (String, f64)| { Ok(()) })?)?;
    globals.set("setCharacterY", lua.create_function(|_lua, (_typ, _val): (String, f64)| { Ok(()) })?)?;

    // Health bar
    globals.set("setHealthBarColors", lua.create_function(|_lua, (_left, _right): (LuaValue, LuaValue)| {
        Ok(())
    })?)?;
    globals.set("setTimeBarColors", lua.create_function(|_lua, (_left, _right): (LuaValue, LuaValue)| {
        Ok(())
    })?)?;

    // Health/score control
    globals.set("getHealth", lua.create_function(|_lua, ()| -> LuaResult<f64> { Ok(1.0) })?)?;
    globals.set("setHealth", lua.create_function(|_lua, _v: f64| { Ok(()) })?)?;
    globals.set("addHealth", lua.create_function(|_lua, _v: f64| { Ok(()) })?)?;
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

    Ok(())
}

fn register_tween_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    for name in [
        "doTweenX", "doTweenY", "doTweenAlpha", "doTweenAngle",
        "doTweenZoom", "doTweenColor", "startTween",
    ] {
        globals.set(name, lua.create_function(move |_lua, _args: LuaMultiValue| {
            Ok(())
        })?)?;
    }

    for name in [
        "noteTweenX", "noteTweenY", "noteTweenAlpha",
        "noteTweenAngle", "noteTweenDirection",
    ] {
        globals.set(name, lua.create_function(move |_lua, _args: LuaMultiValue| {
            Ok(())
        })?)?;
    }

    globals.set("cancelTween", lua.create_function(|_lua, _tag: String| { Ok(()) })?)?;

    Ok(())
}

fn register_sound_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    globals.set("playSound", lua.create_function(|_lua, _args: LuaMultiValue| { Ok(()) })?)?;
    globals.set("playMusic", lua.create_function(|_lua, _args: LuaMultiValue| { Ok(()) })?)?;
    globals.set("stopSound", lua.create_function(|_lua, _tag: Option<String>| { Ok(()) })?)?;
    globals.set("pauseSound", lua.create_function(|_lua, _tag: Option<String>| { Ok(()) })?)?;
    globals.set("resumeSound", lua.create_function(|_lua, _tag: Option<String>| { Ok(()) })?)?;
    globals.set("soundFadeIn", lua.create_function(|_lua, _args: LuaMultiValue| { Ok(()) })?)?;
    globals.set("soundFadeOut", lua.create_function(|_lua, _args: LuaMultiValue| { Ok(()) })?)?;
    globals.set("soundFadeCancel", lua.create_function(|_lua, _tag: Option<String>| { Ok(()) })?)?;
    globals.set("getSoundVolume", lua.create_function(|_lua, _tag: Option<String>| -> LuaResult<f64> { Ok(1.0) })?)?;
    globals.set("setSoundVolume", lua.create_function(|_lua, (_tag, _vol): (Option<String>, f64)| { Ok(()) })?)?;
    globals.set("getSoundTime", lua.create_function(|_lua, _tag: Option<String>| -> LuaResult<f64> { Ok(0.0) })?)?;
    globals.set("setSoundTime", lua.create_function(|_lua, (_tag, _time): (Option<String>, f64)| { Ok(()) })?)?;
    globals.set("luaSoundExists", lua.create_function(|_lua, _tag: String| -> LuaResult<bool> { Ok(false) })?)?;

    Ok(())
}

fn register_text_functions(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // makeLuaText(tag, text, width, x, y)
    globals.set("makeLuaText", lua.create_function(|_lua, (_tag, _text, _width, _x, _y): (String, Option<String>, Option<f64>, Option<f64>, Option<f64>)| {
        Ok(())
    })?)?;

    globals.set("addLuaText", lua.create_function(|_lua, _tag: String| { Ok(()) })?)?;
    globals.set("removeLuaText", lua.create_function(|_lua, (_tag, _destroy): (String, Option<bool>)| { Ok(()) })?)?;
    globals.set("setTextString", lua.create_function(|_lua, (_tag, _text): (String, String)| { Ok(()) })?)?;
    globals.set("setTextSize", lua.create_function(|_lua, (_tag, _size): (String, f64)| { Ok(()) })?)?;
    globals.set("setTextColor", lua.create_function(|_lua, (_tag, _color): (String, String)| { Ok(()) })?)?;
    globals.set("setTextFont", lua.create_function(|_lua, (_tag, _font): (String, String)| { Ok(()) })?)?;
    globals.set("setTextBorder", lua.create_function(|_lua, _args: LuaMultiValue| { Ok(()) })?)?;
    globals.set("setTextAlignment", lua.create_function(|_lua, (_tag, _align): (String, String)| { Ok(()) })?)?;
    globals.set("setTextWidth", lua.create_function(|_lua, (_tag, _w): (String, f64)| { Ok(()) })?)?;
    globals.set("setTextAutoSize", lua.create_function(|_lua, (_tag, _v): (String, bool)| { Ok(()) })?)?;
    globals.set("getTextString", lua.create_function(|_lua, _tag: String| -> LuaResult<String> { Ok(String::new()) })?)?;
    globals.set("luaTextExists", lua.create_function(|_lua, _tag: String| -> LuaResult<bool> { Ok(false) })?)?;

    Ok(())
}

/// Register no-op stubs for all remaining Psych Engine functions that scripts may call.
fn register_noop_stubs(lua: &Lua) -> LuaResult<()> {
    let noop = lua.create_function(|_lua, _args: LuaMultiValue| -> LuaResult<LuaValue> {
        Ok(LuaNil)
    })?;

    for name in [
        // Haxe integration
        "runHaxeFunction", "runHaxeCodePost", "addHaxeLibrary",
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
        "cameraShake", "cameraFlash", "cameraFade",
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
        "initSaveData", "flushSaveData", "getDataFromSave", "setDataFromSave", "eraseSaveData",
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
        "setHudVisible", "runTimer", "cancelTimer",
        "startVideo",
        "setSubtitle", "addShake", "customFlash", "customFade",
        // Timers
        "onTweenCompleted", "onTimerCompleted", "onSoundFinished",
    ] {
        // Only set if not already registered (avoid overwriting real implementations)
        if lua.globals().get::<LuaValue>(name)? == LuaNil {
            lua.globals().set(name, noop.clone())?;
        }
    }

    Ok(())
}

// === Helpers ===

fn lua_to_f32(val: &Option<LuaValue>) -> f32 {
    match val {
        Some(LuaValue::Number(n)) => *n as f32,
        Some(LuaValue::Integer(i)) => *i as f32,
        _ => 0.0,
    }
}

fn lua_val_to_f32(val: &LuaValue) -> Option<f32> {
    match val {
        LuaValue::Number(n) => Some(*n as f32),
        LuaValue::Integer(n) => Some(*n as f32),
        _ => None,
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
