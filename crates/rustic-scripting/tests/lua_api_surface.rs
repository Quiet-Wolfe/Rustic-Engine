use std::io::Write;

use rustic_scripting::{LuaValue, ScriptManager};

fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("rustic-lua-api-tests");
    std::fs::create_dir_all(&dir).expect("mk tmp dir");
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).expect("create tmp");
    f.write_all(body.as_bytes()).expect("write tmp");
    path
}

#[test]
fn psych_lua_api_callbacks_are_registered() {
    let expected = [
        "FlxColor",
        "addAnimation",
        "addAnimationByIndices",
        "addAnimationByIndicesLoop",
        "addAnimationByPrefix",
        "addAnimationBySymbol",
        "addAnimationBySymbolIndices",
        "addCameraFollowPoint",
        "addCameraScroll",
        "addCharacterToList",
        "addHScript",
        "addHaxeLibrary",
        "addHealth",
        "addHits",
        "addInstance",
        "addLuaScript",
        "addLuaSprite",
        "addLuaText",
        "addMisses",
        "addOffset",
        "addScore",
        "addToGroup",
        "anyGamepadJustPressed",
        "anyGamepadPressed",
        "anyGamepadReleased",
        "callMethod",
        "callMethodFromClass",
        "callOnHScript",
        "callOnLuas",
        "callOnScripts",
        "callScript",
        "cameraFade",
        "cameraFlash",
        "cameraSetTarget",
        "cameraShake",
        "cancelTimer",
        "cancelTween",
        "characterDance",
        "characterPlayAnim",
        "checkFileExists",
        "close",
        "closeCustomSubstate",
        "createCallback",
        "createGlobalCallback",
        "createInstance",
        "debugPrint",
        "deleteFile",
        "directoryFileList",
        "doTweenAlpha",
        "doTweenAngle",
        "doTweenColor",
        "doTweenX",
        "doTweenY",
        "doTweenZoom",
        "endSong",
        "eraseSaveData",
        "exitSong",
        "flushSaveData",
        "gamepadAnalogX",
        "gamepadAnalogY",
        "gamepadJustPressed",
        "gamepadPressed",
        "gamepadReleased",
        "getCameraFollowX",
        "getCameraFollowY",
        "getCameraScrollX",
        "getCameraScrollY",
        "getCharacterX",
        "getCharacterY",
        "getColorFromHex",
        "getColorFromName",
        "getColorFromString",
        "getDataFromSave",
        "getGraphicMidpointX",
        "getGraphicMidpointY",
        "getHealth",
        "getMidpointX",
        "getMidpointY",
        "getModSetting",
        "getMouseX",
        "getMouseY",
        "getObjectOrder",
        "getPixelColor",
        "getProperty",
        "getPropertyFromClass",
        "getPropertyFromGroup",
        "getPropertyLuaSprite",
        "getRandomBool",
        "getRandomFloat",
        "getRandomInt",
        "getRunningScripts",
        "getScreenPositionX",
        "getScreenPositionY",
        "getShaderBool",
        "getShaderBoolArray",
        "getShaderFloat",
        "getShaderFloatArray",
        "getShaderInt",
        "getShaderIntArray",
        "getSongPosition",
        "getSoundPitch",
        "getSoundTime",
        "getSoundVolume",
        "getTextFont",
        "getTextFromFile",
        "getTextSize",
        "getTextString",
        "getTextWidth",
        "getVar",
        "initLuaShader",
        "initSaveData",
        "insertToCustomSubstate",
        "instanceArg",
        "isRunning",
        "keyJustPressed",
        "keyPressed",
        "keyReleased",
        "keyboardJustPressed",
        "keyboardPressed",
        "keyboardReleased",
        "loadAnimateAtlas",
        "loadFrames",
        "loadGraphic",
        "loadMultipleFrames",
        "loadSong",
        "luaSoundExists",
        "luaSpriteAddAnimationByIndices",
        "luaSpriteAddAnimationByPrefix",
        "luaSpriteExists",
        "luaSpriteMakeGraphic",
        "luaSpritePlayAnimation",
        "luaTextExists",
        "makeAnimatedLuaSprite",
        "makeFlxAnimateSprite",
        "makeGraphic",
        "makeLuaSprite",
        "makeLuaText",
        "mouseClicked",
        "mousePressed",
        "mouseReleased",
        "musicFadeIn",
        "musicFadeOut",
        "noteTweenAlpha",
        "noteTweenAngle",
        "noteTweenDirection",
        "noteTweenX",
        "noteTweenY",
        "objectPlayAnimation",
        "objectsOverlap",
        "openCustomSubstate",
        "pauseSound",
        "playAnim",
        "playMusic",
        "playSound",
        "precacheImage",
        "precacheMusic",
        "precacheSound",
        "removeFromGroup",
        "removeHScript",
        "removeLuaScript",
        "removeLuaSprite",
        "removeLuaText",
        "removeSpriteShader",
        "removeVar",
        "restartSong",
        "resumeSound",
        "runHaxeCode",
        "runHaxeFunction",
        "runTimer",
        "saveFile",
        "scaleLuaSprite",
        "scaleObject",
        "screenCenter",
        "setBlendMode",
        "setCameraFollowPoint",
        "setCameraScroll",
        "setCharacterX",
        "setCharacterY",
        "setDataFromSave",
        "setGraphicSize",
        "setHealth",
        "setHealthBarColors",
        "setHits",
        "setLuaSpriteCamera",
        "setLuaSpriteScrollFactor",
        "setMisses",
        "setObjectCamera",
        "setObjectOrder",
        "setOnHScript",
        "setOnLuas",
        "setOnScripts",
        "setProperty",
        "setPropertyFromClass",
        "setPropertyFromGroup",
        "setPropertyLuaSprite",
        "setRatingFC",
        "setRatingName",
        "setRatingPercent",
        "setScore",
        "setScrollFactor",
        "setShaderBool",
        "setShaderBoolArray",
        "setShaderFloat",
        "setShaderFloatArray",
        "setShaderInt",
        "setShaderIntArray",
        "setShaderSampler2D",
        "setSoundPitch",
        "setSoundTime",
        "setSoundVolume",
        "setSpriteShader",
        "setTextAlignment",
        "setTextAutoSize",
        "setTextBorder",
        "setTextColor",
        "setTextFont",
        "setTextHeight",
        "setTextItalic",
        "setTextSize",
        "setTextString",
        "setTextWidth",
        "setTimeBarColors",
        "setVar",
        "soundFadeCancel",
        "soundFadeIn",
        "soundFadeOut",
        "startCountdown",
        "startDialogue",
        "startTween",
        "startVideo",
        "stopSound",
        "stringEndsWith",
        "stringSplit",
        "stringStartsWith",
        "stringTrim",
        "triggerEvent",
        "updateHitbox",
        "updateHitboxFromGroup",
        "updateScoreText",
    ];

    let names = expected
        .iter()
        .map(|name| format!("{name:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let src = format!(
        r#"
        local missing = {{}}
        for _, name in ipairs({{{names}}}) do
            if type(_G[name]) ~= 'function' then
                table.insert(missing, name .. ':' .. type(_G[name]))
            end
        end
        if #missing > 0 then
            error(table.concat(missing, ','))
        end
        function onCreate()
            setVar('apiSurfaceOk', true)
        end
    "#
    );
    let path = write_tmp("api_surface.lua", &src);

    let mut mgr = ScriptManager::new();
    mgr.load_script(&path);
    assert!(mgr.has_scripts(), "API surface smoke script failed to load");
    mgr.call("onCreate");
    assert!(matches!(
        mgr.state.custom_vars.get("apiSurfaceOk"),
        Some(LuaValue::Bool(true))
    ));
}

#[test]
fn psych_mod_settings_and_remove_var_work() {
    let root = std::env::temp_dir()
        .join("rustic-lua-api-tests")
        .join("TestMod");
    let settings_dir = root.join("data");
    std::fs::create_dir_all(&settings_dir).expect("mk settings dir");
    std::fs::write(
        settings_dir.join("settings.json"),
        r#"
        [
            {"save":"enabled","type":"bool","value":true},
            {"save":"volume","type":"float","value":0.75},
            {"save":"bind","type":"keybind","keyboard":"SPACE","gamepad":"A"}
        ]
        "#,
    )
    .expect("write settings");

    let script = write_tmp(
        "settings_and_remove_var.lua",
        r#"
        local enabled = getModSetting('enabled')
        local volume = getModSetting('volume')
        local bind = getModSetting('bind')
        setVar('tempThing', 123)
        local removed = removeVar('tempThing')
        function onCreate()
            setVar('settingsOk', enabled == true and volume == 0.75 and bind.keyboard == 'SPACE' and removed == true and getVar('tempThing') == nil)
        end
        "#,
    );

    let mut mgr = ScriptManager::new();
    mgr.set_song_metadata(120.0, 1.0, 0.0, "stage", "Hard", "TestMod");
    mgr.set_image_roots(vec![root]);
    mgr.load_script(&script);
    assert!(mgr.has_scripts(), "settings smoke script failed to load");
    mgr.call("onCreate");
    assert!(matches!(
        mgr.state.custom_vars.get("settingsOk"),
        Some(LuaValue::Bool(true))
    ));
}

#[test]
fn psych_start_video_boolean_signature_queues_cutscene() {
    let script = write_tmp(
        "start_video_signature.lua",
        r#"
        function onCreate()
            setVar('videoResult', startVideo('intro-cutscene', true, false))
        end
        "#,
    );

    let mut mgr = ScriptManager::new();
    mgr.load_script(&script);
    assert!(mgr.has_scripts(), "startVideo smoke script failed to load");
    mgr.call("onCreate");
    assert!(matches!(
        mgr.state.custom_vars.get("videoResult"),
        Some(LuaValue::Bool(true))
    ));
    assert_eq!(mgr.state.video_requests.len(), 1);
    assert_eq!(mgr.state.video_requests[0].0, "intro-cutscene");
    assert!(mgr.state.video_requests[0].2);
}

#[test]
fn dynamic_callbacks_and_frame_reload_are_backed_by_state() {
    let script = write_tmp(
        "callbacks_and_reload.lua",
        r#"
        function onCreate()
            createCallback('madeAtRuntime', function(value)
                return value + 7
            end)
            createGlobalCallback('madeGlobal', function(value)
                return value * 3
            end)
            local callbackOk = madeAtRuntime(5) == 12
                and madeGlobal(4) == 12
                and runHaxeCodePost('game.moveCameraSection(2)') == nil
                and isPaused() == false
            makeLuaSprite('reloadMe', 'old/image', 0, 0)
            addLuaSprite('reloadMe', false)
            local reloadOk = loadFrames('reloadMe', 'new/image')
            musicFadeIn(1, 0.2, 0.8)
            setVar('callbackReloadOk', callbackOk and reloadOk)
        end
        "#,
    );

    let mut mgr = ScriptManager::new();
    mgr.load_script(&script);
    assert!(
        mgr.has_scripts(),
        "callback/reload smoke script failed to load"
    );
    mgr.call("onCreate");
    assert!(matches!(
        mgr.state.custom_vars.get("callbackReloadOk"),
        Some(LuaValue::Bool(true))
    ));
    assert!(mgr
        .state
        .lua_sprites
        .get("reloadMe")
        .is_some_and(|sprite| matches!(
            &sprite.kind,
            rustic_scripting::LuaSpriteKind::Animated(image) if image == "new/image"
        )));
    assert!(!mgr.state.sprites_to_add.is_empty());
    assert!(mgr
        .state
        .audio_requests
        .iter()
        .any(|request| matches!(request, rustic_scripting::AudioRequest::SoundFade { tag: None, to, .. } if (*to - 0.8).abs() < f64::EPSILON)));
    assert_eq!(mgr.state.camera_section_requests, vec![2]);
}

#[test]
fn reflection_and_substate_aliases_route_to_existing_systems() {
    let script = write_tmp(
        "reflection_aliases.lua",
        r#"
        function onCreate()
            makeLuaSprite('subSprite', nil, 0, 0)
            makeGraphic('subSprite', 2, 2, 'FFFFFF')
            local addOk = addLuaSpriteSubstate('subSprite')
            local classOk = callMethodFromClass('flixel.FlxG', 'cameras.add', {instanceArg('camHUD'), false}) == true
            local shader = createRuntimeShader('fakeShader')
            local shaderOk = shader == 'fakeShader'
            local removeOk = removeFromGroup('members', -1, 'subSprite', true)
            setVar('reflectionAliasOk', addOk == nil and classOk and shaderOk and removeOk)
        end
        "#,
    );

    let mut mgr = ScriptManager::new();
    mgr.load_script(&script);
    assert!(
        mgr.has_scripts(),
        "reflection alias smoke script failed to load"
    );
    mgr.call("onCreate");
    assert!(matches!(
        mgr.state.custom_vars.get("reflectionAliasOk"),
        Some(LuaValue::Bool(true))
    ));
    assert!(mgr
        .state
        .sprites_to_remove
        .iter()
        .any(|tag| tag == "subSprite"));
}
