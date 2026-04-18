use winit::event::TouchPhase;
use winit::keyboard::KeyCode;

use rustic_audio::AudioEngine;
use rustic_core::paths::AssetPaths;
use rustic_render::gpu::{GpuState, GpuTexture};
use rustic_render::sprites::{AnimationController, SpriteAtlas};

use crate::screen::Screen;
use super::freeplay::FreeplayScreen;
use super::mods::ModsScreen;
use super::options::OptionsScreen;
use super::story_menu::StoryMenuScreen;

const GAME_W: f32 = 1280.0;
const GAME_H: f32 = 720.0;

/// Menu item names that match the Sparrow atlas animation prefixes.
const MENU_ITEMS: [&str; 4] = ["story_mode", "freeplay", "mods", "options"];

struct MenuItem {
    atlas: SpriteAtlas,
    tex: GpuTexture,
    anim: AnimationController,
    y: f32,
    alpha: f32,
}

pub struct MainMenuScreen {
    audio: Option<AudioEngine>,
    bg_tex: Option<GpuTexture>,
    items: Vec<MenuItem>,
    cur_selected: usize,
    confirmed: bool,
    confirm_timer: f32,
    next: Option<Box<dyn Screen>>,
    /// Camera Y position (lerps toward selected item).
    cam_y: f32,
    cam_y_target: f32,
}

impl MainMenuScreen {
    pub fn new() -> Self {
        Self {
            audio: None,
            bg_tex: None,
            items: Vec::new(),
            cur_selected: 0,
            confirmed: false,
            confirm_timer: 0.0,
            next: None,
            cam_y: 0.0,
            cam_y_target: 0.0,
        }
    }

    fn change_selection(&mut self, delta: i32) {
        let len = self.items.len() as i32;
        if len == 0 { return; }
        self.cur_selected = ((self.cur_selected as i32 + delta).rem_euclid(len)) as usize;
        self.update_item_anims();
        self.update_cam_target();

        if let Some(audio) = &mut self.audio {
            let paths = AssetPaths::platform_default();
            if let Some(sfx) = paths.sound("scrollMenu") {
                audio.play_sound(&sfx, 0.7);
            }
        }
    }

    fn update_item_anims(&mut self) {
        for (i, item) in self.items.iter_mut().enumerate() {
            let name = MENU_ITEMS[i];
            if i == self.cur_selected {
                let anim_name = format!("{} selected", name);
                item.anim.play(&anim_name, 24.0, true);
            } else {
                let anim_name = format!("{} idle", name);
                item.anim.play(&anim_name, 24.0, true);
            }
        }
    }

    fn update_cam_target(&mut self) {
        if let Some(item) = self.items.get(self.cur_selected) {
            // Psych Engine: camFollow.y = menuItem.getGraphicMidpoint().y
            // The midpoint is roughly item.y + frame_height/2, but we use item.y
            // since we offset items relative to cam_y later. Target the item's
            // center screen position.
            self.cam_y_target = item.y + 70.0 - GAME_H / 2.0;
        }
    }
}

impl Screen for MainMenuScreen {
    fn init(&mut self, gpu: &GpuState) {
        let paths = AssetPaths::platform_default();

        // Background — scaled 1.175x
        if let Some(bg_path) = paths.image("menuBG") {
            self.bg_tex = Some(gpu.load_texture_from_path(&bg_path));
        }

        // Menu items
        let item_count = MENU_ITEMS.len();

        for (i, &name) in MENU_ITEMS.iter().enumerate() {
            let xml_path = paths.find(&format!("images/mainmenu/menu_{}.xml", name));
            let png_path = paths.find(&format!("images/mainmenu/menu_{}.png", name));
            let (Some(xml_path), Some(png_path)) = (xml_path, png_path) else { continue };

            let xml = std::fs::read_to_string(&xml_path).unwrap_or_default();
            let mut atlas = SpriteAtlas::from_xml(&xml);
            let idle_anim = format!("{} idle", name);
            let selected_anim = format!("{} selected", name);
            atlas.add_by_prefix(&idle_anim, &idle_anim);
            atlas.add_by_prefix(&selected_anim, &selected_anim);

            let tex = gpu.load_texture_from_path(&png_path);
            let mut anim = AnimationController::new();
            anim.play(&idle_anim, 24.0, true);

            // Psych Engine positioning: y = (num * 140) + 90, then offset by (4 - count) * 70
            let y = (i as f32 * 140.0) + 90.0 + (4.0 - item_count as f32) * 70.0;

            self.items.push(MenuItem { atlas, tex, anim, y, alpha: 1.0 });
        }

        // Set initial selected item
        self.update_item_anims();
        self.update_cam_target();
        self.cam_y = self.cam_y_target;

        // Audio — freakyMenu continues (skip if already passed from previous screen)
        if self.audio.is_none() {
            if let Some(music) = paths.music("freakyMenu") {
                let mut audio = AudioEngine::new();
                audio.play_loop_music_vol(&music, 0.7);
                self.audio = Some(audio);
            }
        }
    }

    fn handle_key(&mut self, key: KeyCode) {
        if self.confirmed { return; }

        match key {
            KeyCode::ArrowUp | KeyCode::KeyW => self.change_selection(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => self.change_selection(1),
            KeyCode::Enter | KeyCode::Space => {
                self.confirmed = true;
                if let Some(audio) = &mut self.audio {
                    let paths = AssetPaths::platform_default();
                    if let Some(sfx) = paths.sound("confirmMenu") {
                        audio.play_sound(&sfx, 0.7);
                    }
                }
            }
            KeyCode::Escape | KeyCode::Backspace => {
                if let Some(audio) = &mut self.audio {
                    let paths = AssetPaths::platform_default();
                    if let Some(sfx) = paths.sound("cancelMenu") {
                        audio.play_sound(&sfx, 0.7);
                    }
                }
                self.next = Some(Box::new(super::title::TitleScreen::new()));
            }
            _ => {}
        }
    }

    fn handle_touch(&mut self, _id: u64, phase: TouchPhase, _x: f64, y: f64) {
        if phase != TouchPhase::Started || self.confirmed { return; }
        let y = y as f32;

        // Detect which menu item was tapped by Y position
        for (i, item) in self.items.iter().enumerate() {
            let item_screen_y = item.y - self.cam_y;
            // Each item is roughly 140px tall
            if y >= item_screen_y && y < item_screen_y + 140.0 {
                if i == self.cur_selected {
                    // Already selected — confirm
                    self.handle_key(KeyCode::Enter);
                } else {
                    // Select this item
                    self.cur_selected = i;
                    self.update_item_anims();
                    self.update_cam_target();
                    if let Some(audio) = &mut self.audio {
                        let paths = AssetPaths::platform_default();
                        if let Some(sfx) = paths.sound("scrollMenu") {
                            audio.play_sound(&sfx, 0.7);
                        }
                    }
                }
                return;
            }
        }
    }

    fn update(&mut self, dt: f32) {
        // Update item animations
        for item in &mut self.items {
            let fc = item.atlas.frame_count(&item.anim.current_anim);
            item.anim.update(dt, fc);
        }

        // Camera lerp — Psych uses 0.15 * (60/fps), we use exponential lerp
        let lerp = 1.0 - (-dt * 9.0).exp();
        self.cam_y += (self.cam_y_target - self.cam_y) * lerp;

        // Confirm: fade non-selected items and transition after ~1 second (flicker duration)
        if self.confirmed {
            self.confirm_timer += dt;

            // Fade non-selected items (0.4s quadOut)
            for (i, item) in self.items.iter_mut().enumerate() {
                if i != self.cur_selected {
                    let t = (self.confirm_timer / 0.4).min(1.0);
                    item.alpha = 1.0 - t * t; // quadOut-ish
                }
            }

            // Transition after flicker completes (~1 second)
            if self.confirm_timer >= 1.0 {
                match MENU_ITEMS.get(self.cur_selected) {
                    Some(&"freeplay") => {
                        self.next = Some(Box::new(FreeplayScreen::new()));
                    }
                    Some(&"story_mode") => {
                        self.next = Some(Box::new(StoryMenuScreen::new()));
                    }
                    Some(&"mods") => {
                        self.next = Some(Box::new(ModsScreen::new()));
                    }
                    Some(&"options") => {
                        self.next = Some(Box::new(OptionsScreen::new()));
                    }
                    _ => {
                        // Credits or fallback - just go to freeplay for now
                        self.next = Some(Box::new(FreeplayScreen::new()));
                    }
                }
            }
        }
    }

    fn draw(&mut self, gpu: &mut GpuState) {
        if !gpu.begin_frame() { return; }

        // Background — scaled 1.175x, scrollFactor(0, 0.18) parallax on Y
        if let Some(bg) = &self.bg_tex {
            let scale = 1.175;
            let w = bg.width as f32 * scale;
            let h = bg.height as f32 * scale;
            let x = (GAME_W - w) / 2.0;
            // Psych: bg at y=-80, scrollFactor(0, 0.18) means BG moves at 18% of camera
            let y = -80.0 - self.cam_y * 0.18;
            gpu.push_texture_region(
                bg.width as f32, bg.height as f32,
                0.0, 0.0, bg.width as f32, bg.height as f32,
                x, y, w, h,
                false, [1.0, 1.0, 1.0, 1.0],
            );
            gpu.draw_batch(Some(bg));
        }

        // Menu items — centered horizontally, offset by camera
        for (i, item) in self.items.iter().enumerate() {
            let anim_name = &item.anim.current_anim;
            let frame = item.atlas.get_frame(anim_name, item.anim.frame_index);
            if let Some(frame) = frame {
                // Center horizontally based on frame_w (like screenCenter(X))
                let x = (GAME_W - frame.frame_w) / 2.0;
                let y = item.y - self.cam_y;
                let a = item.alpha;

                // Flicker effect for selected item during confirm
                let visible = if self.confirmed && i == self.cur_selected {
                    let flicker_idx = (self.confirm_timer / 0.06) as i32;
                    flicker_idx % 2 == 0
                } else {
                    true
                };

                if visible {
                    let color = [a, a, a, a]; // premultiplied white * alpha
                    gpu.draw_sprite_frame(
                        frame, item.tex.width as f32, item.tex.height as f32,
                        x, y, 1.0, false, color,
                    );
                    gpu.draw_batch(Some(&item.tex));
                }
            }
        }

        // Version text
        gpu.draw_text("RusticV2", 12.0, GAME_H - 24.0, 16.0, [1.0, 1.0, 1.0, 1.0]);

        crate::debug_overlay::finish_frame(gpu);
    }

    fn next_screen(&mut self) -> Option<Box<dyn Screen>> {
        self.next.take()
    }

    fn take_audio(&mut self) -> Option<AudioEngine> {
        self.audio.take()
    }

    fn set_audio(&mut self, audio: AudioEngine) {
        self.audio = Some(audio);
    }
}
