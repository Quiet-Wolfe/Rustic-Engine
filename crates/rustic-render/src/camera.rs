/// Game camera with zoom, follow, and lerp.
/// Supports two cameras: game world and HUD overlay.
pub struct GameCamera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub target_zoom: f32,
    /// Stage-defined camera speed multiplier (default 1.0).
    pub camera_speed: f32,
    /// Optional direct followLerp override from scripts. Values above 1 snap.
    pub follow_lerp: Option<f32>,
    /// Camera shake state: (intensity, remaining_duration).
    pub shake: Option<(f32, f32)>,
    /// Camera flash/fade state: (r, g, b, alpha, remaining, total, fade_in).
    pub flash: Option<(f32, f32, f32, f32, f32, f32, bool)>,
    /// Current shake offset applied to position.
    pub shake_offset: (f32, f32),
}

impl GameCamera {
    pub fn new(zoom: f32) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom,
            target_x: 0.0,
            target_y: 0.0,
            target_zoom: zoom,
            camera_speed: 1.0,
            follow_lerp: None,
            shake: None,
            flash: None,
            shake_offset: (0.0, 0.0),
        }
    }

    /// Smoothly interpolate toward target using Psych Engine's exact formula:
    /// `lerp = 1 - exp(-dt * 2.4 * cameraSpeed)`
    pub fn update(&mut self, dt_secs: f32) {
        let lerp = self
            .follow_lerp
            .map(|v| v.clamp(0.0, 1.0))
            .unwrap_or_else(|| 1.0 - (-dt_secs * 2.4 * self.camera_speed).exp());
        self.x += (self.target_x - self.x) * lerp;
        self.y += (self.target_y - self.y) * lerp;
        self.zoom += (self.target_zoom - self.zoom) * lerp;
        self.update_effects(dt_secs);
    }

    /// Set the position target.
    pub fn follow(&mut self, x: f32, y: f32) {
        self.target_x = x;
        self.target_y = y;
    }

    /// Snap to position immediately (no lerp).
    pub fn snap_to(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
        self.target_x = x;
        self.target_y = y;
    }

    /// Add a zoom bump that decays via the natural lerp.
    pub fn bump_zoom(&mut self, amount: f32) {
        self.zoom += amount;
    }

    /// Start a camera shake effect.
    pub fn start_shake(&mut self, intensity: f32, duration: f32) {
        self.shake = Some((intensity, duration));
    }

    /// Start a camera flash overlay.
    pub fn start_flash(&mut self, color_hex: &str, duration: f32, alpha: f32) {
        let hex = color_hex
            .trim_start_matches('#')
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        let color_val = u32::from_str_radix(hex, 16).unwrap_or(0xFFFFFF);
        let r = ((color_val >> 16) & 0xFF) as f32 / 255.0;
        let g = ((color_val >> 8) & 0xFF) as f32 / 255.0;
        let b = (color_val & 0xFF) as f32 / 255.0;
        self.flash = Some((r, g, b, alpha, duration, duration, true));
    }

    /// Start a Psych-style camera fade. fade_in=true fades from color to clear;
    /// fade_in=false fades from clear to color.
    pub fn start_fade(&mut self, color_hex: &str, duration: f32, fade_in: bool) {
        let hex = color_hex
            .trim_start_matches('#')
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        let color_val = u32::from_str_radix(hex, 16).unwrap_or(0x000000);
        let r = ((color_val >> 16) & 0xFF) as f32 / 255.0;
        let g = ((color_val >> 8) & 0xFF) as f32 / 255.0;
        let b = (color_val & 0xFF) as f32 / 255.0;
        self.flash = Some((r, g, b, 1.0, duration, duration, fade_in));
    }

    /// Update shake and flash effects.
    pub fn update_effects(&mut self, dt: f32) {
        // Shake
        if let Some((intensity, ref mut remaining)) = self.shake {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.shake = None;
                self.shake_offset = (0.0, 0.0);
            } else {
                let t = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as f32
                    / 1_000_000_000.0;
                let scale = intensity * 100.0;
                self.shake_offset.0 = (t * 7919.0).sin() * scale;
                self.shake_offset.1 = (t * 6271.0).cos() * scale;
            }
        }
        // Flash/Fade
        if let Some((_, _, _, _, ref mut remaining, _, fade_in)) = self.flash {
            *remaining -= dt;
            if *remaining <= 0.0 {
                if fade_in {
                    // Flash (fade out) finished -> remove
                    self.flash = None;
                } else {
                    // Fade (fade to color) finished -> stay at 0 remaining time
                    *remaining = 0.0;
                }
            }
        }
    }

    /// Get current flash overlay color and alpha (if active).
    pub fn flash_overlay(&self) -> Option<([f32; 3], f32)> {
        self.flash
            .map(|(r, g, b, alpha, remaining, total, fade_in)| {
                let t = if total > 0.0 { remaining / total } else { 0.0 };
                let eased_alpha = if fade_in {
                    alpha * t
                } else {
                    alpha * (1.0 - t)
                };
                ([r, g, b], eased_alpha)
            })
    }

    /// Convert a world position to screen position given screen dimensions.
    pub fn world_to_screen(
        &self,
        world_x: f32,
        world_y: f32,
        screen_w: f32,
        screen_h: f32,
    ) -> (f32, f32) {
        let sx = (world_x - self.x) * self.zoom + screen_w / 2.0 + self.shake_offset.0;
        let sy = (world_y - self.y) * self.zoom + screen_h / 2.0 + self.shake_offset.1;
        (sx, sy)
    }
}

/// HUD camera — fixed position, independent zoom for UI elements.
pub struct HudCamera {
    pub zoom: f32,
    pub target_zoom: f32,
    pub follow_lerp: f32,
}

impl HudCamera {
    pub fn new(zoom: f32) -> Self {
        Self {
            zoom,
            target_zoom: zoom,
            follow_lerp: 0.04,
        }
    }

    pub fn update(&mut self, dt_secs: f32) {
        let lerp = 1.0 - (1.0 - self.follow_lerp).powf(dt_secs * 60.0);
        self.zoom += (self.target_zoom - self.zoom) * lerp;
    }

    pub fn bump_zoom(&mut self, amount: f32) {
        self.zoom += amount;
    }
}
