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
        }
    }

    /// Smoothly interpolate toward target using Psych Engine's exact formula:
    /// `lerp = 1 - exp(-dt * 2.4 * cameraSpeed)`
    pub fn update(&mut self, dt_secs: f32) {
        let lerp = 1.0 - (-dt_secs * 2.4 * self.camera_speed).exp();
        self.x += (self.target_x - self.x) * lerp;
        self.y += (self.target_y - self.y) * lerp;
        self.zoom += (self.target_zoom - self.zoom) * lerp;
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

    /// Convert a world position to screen position given screen dimensions.
    pub fn world_to_screen(&self, world_x: f32, world_y: f32, screen_w: f32, screen_h: f32) -> (f32, f32) {
        let sx = (world_x - self.x) * self.zoom + screen_w / 2.0;
        let sy = (world_y - self.y) * self.zoom + screen_h / 2.0;
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
