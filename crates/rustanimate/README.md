# RustAnimate

A renderer-agnostic Rust port of `flxanimate`, designed to parse and render Adobe Animate texture atlases.

## Overview

Adobe Animate exports complex skeletal and sprite-based animations using a combination of JSON timeline data (`Animation.json`), sprite mapping data (`spritemap1.json`), and a packed texture atlas (`spritemap1.png`). 

`rustanimate` parses this JSON structure, recursively traverses the timeline layers, handles nested symbols, resolves frame logic, applies affine 2D transformations, and outputs a flat list of simple 2D draw calls (vertices, UVs, and colors) that any modern graphics backend (wgpu, macroquad, bevy, etc.) can easily render.

## Usage

Add `rustanimate` and `glam` to your `Cargo.toml`.

```rust
use rustanimate::FlxAnimate;

// 1. Load the animation from a directory containing the JSON files.
// Your specific engine should handle loading the `spritemap1.png` file.
let mut anim = FlxAnimate::load("path/to/animation/folder").unwrap();

// 2. In your update loop, advance the animation by delta time (in seconds).
anim.update(delta_time);

// 3. Generate renderer-agnostic draw calls anchored at (x, y).
let draw_calls = anim.render(0.0, 0.0);

// 4. Pass the generated vertices to your engine's rendering pipeline.
for call in draw_calls {
    // call.vertices contains 4 points: position [x,y], uv [u,v], color [r,g,b,a]
    // call.indices contains standard quad indices: [0, 1, 2, 0, 2, 3]
}
```

## Multiple Animations

Many characters package multiple animations (idle, walk, attack) inside a single "master" atlas. `rustanimate` automatically extracts these into the `available_animations` list. You can switch between them using:

```rust
anim.next_anim(); // Cycle to the next animation
anim.prev_anim(); // Cycle to the previous animation

// Or manually set the currently playing symbol by name:
anim.playing_symbol = "ninja-girl-attack".to_string();
```

## Examples

To run the provided `macroquad` example viewer:

```bash
cargo run --example viewer -- path/to/your/atlas/folder
```

Controls for the viewer:
- **Arrows:** Pan camera
- **W/S:** Zoom camera
- **Q/E:** Cycle through available animations
