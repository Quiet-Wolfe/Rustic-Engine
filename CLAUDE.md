# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

RusticV2 is a from-scratch Rust reimplementation of **Psych Engine** (the dominant Friday Night Funkin' modding engine). The goal is 1:1 feature parity with Psych Engine first, then targeted improvements where the original is weak (e.g., camera zoom smoothness), while maintaining identical Lua API syntax and behavior.

This is the second attempt — V1 failed because the architecture allowed a monolithic main.rs (~2000+ lines) where adding menus (freeplay) caused cascading breakage. V2 must stay modular and commit frequently.

## Build & Run

```bash
cargo build              # debug build
cargo run                # run the engine
cargo test               # all workspace tests
cargo test -p rustic-core # single crate tests
cargo clippy             # lint
```

## Architecture (Workspace Crates)

The project uses a Cargo workspace with isolated crates. Each crate has a single responsibility:

- **rustic-core** — Data types and parsing only. Chart formats, character definitions, stage files, note types, scoring/rating math, asset path resolution, mod loading. Zero rendering or audio dependencies.
- **rustic-audio** — Audio playback via `kira`. Music/vocals sync, sound effects, conductor (BPM/beat tracking with song position).
- **rustic-render** — All drawing. Sparrow XML atlas parsing, sprite animation, camera system, note rendering, HUD, health bar, countdown visuals, stage rendering. Depends on the graphics backend.
- **rustic-gameplay** — Game logic. Input handling, note hit/miss detection, hold note logic, event system, PlayState (the in-song game loop). No rendering — emits events that the render layer consumes.
- **rustic-scripting** — Lua VM integration. Exposes the Psych Engine Lua API (callbacks, property access, sprite manipulation, tweens, camera control). Must match Psych Engine's function signatures.
- **rustic-app** — Binary entry point and state machine. Wires crates together, manages screen transitions (title → menu → freeplay → story → loading → playing → results). Each screen is its own module, not a megafunction.

**Critical rule:** No crate should become a dumping ground. If a module exceeds ~500 lines, it should be split. The V1 failure was a monolithic main.rs — never again.

## References Directory

`references/` contains source material (not compiled into the engine):

- `FNF-PsychEngine/` — Full Psych Engine Haxe source + assets. The primary reference for behavior matching.
- `funkin/` — Base Friday Night Funkin' source (pre-Psych). Useful for understanding original formats.
- `RusticV1/` — The failed first attempt. Good for reusable parsing code (atlas, charts), bad as architectural reference.
- `VS-RetroSpecter-PART-2-Compiled/` — Compiled mod with custom events, modcharts, and Lua scripts. The ultimate integration test — when this mod runs correctly, the engine is ready.

## Key Technical Details

### Asset Formats
- **Sprite atlases**: Sparrow XML format (TexturePacker). Each `<SubTexture>` has name, position, frame offsets, and optional rotation. Animation names are derived by stripping trailing digits from SubTexture names.
- **Charts**: Psych Engine JSON format. Contains `song.notes[]` sections (each with `sectionNotes`, `mustHitSection`, `bpm` changes), `song.events[]`, and metadata.
- **Characters**: JSON files defining atlas prefix mappings, offsets, camera offsets, sing duration, health icon.
- **Stages**: JSON files with sprite layers (foreground/background), positions, scroll factors, zoom.

### Conductor / Timing
The conductor tracks song position in milliseconds and maps it to beats/steps/sections. BPM changes mid-song are supported. All gameplay timing derives from the conductor — never from wall-clock time.

### Note System
- 4 lanes (left/down/up/right), opponent + player strums
- Hit windows: Sick (45ms), Good (90ms), Bad (135ms), Shit (166ms) — match Psych Engine exactly
- Hold notes: sustained, score per tick while held, released = miss remaining
- Note types: custom behaviors via string type field (e.g., "Hurt Note", "Alt Animation")

### Camera System
Two game cameras (game world + HUD overlay). Camera follows current singer with lerped movement. Section-driven camera targets. Zoom pulses on beats with configurable intensity. V2 should smooth the zoom math (Psych Engine's bump implementation is jittery) while keeping the same Lua API.

### Lua Scripting (Phase: Last)
Psych Engine exposes ~200+ Lua functions. Key callback groups:
- Song lifecycle: `onCreate`, `onCreatePost`, `onUpdate`, `onUpdatePost`, `onSongStart`, `onEndSong`
- Note events: `onSpawnNote`, `goodNoteHit`, `opponentNoteHit`, `noteMiss`, `noteMissPress`
- Beat/step: `onBeatHit`, `onStepHit`, `onSectionHit`
- Custom events: `onEvent`, `eventEarlyTrigger`
- Graphics: `makeLuaSprite`, `addLuaSprite`, `setProperty`, `getProperty`, tween functions

## Development Phases

The phases below must be completed in order. Each phase should be playtest-verified before moving on.

- [ ] **Phase 1: Core + Rendering Foundation** — Chart parsing, character/stage file loading, conductor/timing, scoring math, asset path resolution. Window creation, Sparrow atlas loading + animation, basic sprite drawing, camera system (game + HUD cameras). Unit tests for parsing, but verify visually that atlases render correctly before moving on.
- [ ] **Phase 2: Gameplay + Audio + HUD** — Note spawning/scrolling, strum line, input detection, hit/miss judgment, hold notes, health, score tracking. Instrumental + voices sync, conductor-driven timing, sound effects (miss sounds, countdown). Health bar, score/combo display, countdown sequence, rating popups (Sick/Good/Bad/Shit), combo numbers, note splashes. Playable with one hardcoded song, audio synced, full HUD.
- [ ] **Phase 3: Characters & Stage** — Animated character sprites (idle, sing directions, miss), stage background rendering, camera following singer.
- [ ] **Phase 4: Menus** — Title screen, main menu, freeplay (song list + difficulty select + highscores), story mode. **Each menu is its own module.** This is where V1 died.
- [ ] **Phase 5: Mods + Lua + Modcharts** — Mod directory loading, asset override priority (mod → base), custom note types, custom events. Lua VM, callback system, property bridge, sprite/tween/camera API. Modchart note manipulation, shader support, video playback, HScript. Heavy playtesting with VS Retrospecter Part 2.

## Commit Discipline

Commit after every meaningful unit of work — a new parser, a working subsystem, a bug fix. Small, frequent commits. If something breaks, we revert, not debug for hours.
