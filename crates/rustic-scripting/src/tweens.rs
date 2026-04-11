use std::collections::HashMap;
use std::f32::consts::PI;

/// A running tween that interpolates a property over time.
#[derive(Debug, Clone)]
pub struct Tween {
    pub tag: String,
    /// Target object identifier (sprite tag, "camGame", etc.)
    pub target: String,
    /// Which property to tween (x, y, alpha, angle, zoom).
    pub property: TweenProperty,
    /// Starting value (captured when tween starts).
    pub start_value: f32,
    /// Target value.
    pub end_value: f32,
    /// Total duration in seconds.
    pub duration: f32,
    /// Elapsed time in seconds.
    pub elapsed: f32,
    /// Easing function.
    pub ease: EaseFunc,
    /// Whether this tween has completed.
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TweenProperty {
    X,
    Y,
    Alpha,
    Angle,
    Zoom,
    ScaleX,
    ScaleY,
    RedOffset,
    GreenOffset,
    BlueOffset,
    OffsetX,
    OffsetY,
}

/// Easing function type.
#[derive(Debug, Clone, Copy)]
pub enum EaseFunc {
    Linear,
    QuadIn, QuadOut, QuadInOut,
    CubeIn, CubeOut, CubeInOut,
    QuartIn, QuartOut, QuartInOut,
    QuintIn, QuintOut, QuintInOut,
    SineIn, SineOut, SineInOut,
    ExpoIn, ExpoOut, ExpoInOut,
    CircIn, CircOut, CircInOut,
    BackIn, BackOut, BackInOut,
    BounceIn, BounceOut, BounceInOut,
    ElasticIn, ElasticOut, ElasticInOut,
    SmoothStepIn, SmoothStepOut, SmoothStepInOut,
    SmootherStepIn, SmootherStepOut, SmootherStepInOut,
}

impl EaseFunc {
    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "quadin" => Self::QuadIn,
            "quadout" => Self::QuadOut,
            "quadinout" => Self::QuadInOut,
            "cubein" => Self::CubeIn,
            "cubeout" => Self::CubeOut,
            "cubeinout" => Self::CubeInOut,
            "quartin" => Self::QuartIn,
            "quartout" => Self::QuartOut,
            "quartinout" => Self::QuartInOut,
            "quintin" => Self::QuintIn,
            "quintout" => Self::QuintOut,
            "quintinout" => Self::QuintInOut,
            "sinein" => Self::SineIn,
            "sineout" => Self::SineOut,
            "sineinout" => Self::SineInOut,
            "expoin" => Self::ExpoIn,
            "expoout" => Self::ExpoOut,
            "expoinout" => Self::ExpoInOut,
            "circin" => Self::CircIn,
            "circout" => Self::CircOut,
            "circinout" => Self::CircInOut,
            "backin" => Self::BackIn,
            "backout" => Self::BackOut,
            "backinout" => Self::BackInOut,
            "bouncein" => Self::BounceIn,
            "bounceout" => Self::BounceOut,
            "bounceinout" => Self::BounceInOut,
            "elasticin" => Self::ElasticIn,
            "elasticout" => Self::ElasticOut,
            "elasticinout" => Self::ElasticInOut,
            "smoothstepin" => Self::SmoothStepIn,
            "smoothstepout" => Self::SmoothStepOut,
            "smoothstepinout" => Self::SmoothStepInOut,
            "smootherstepin" => Self::SmootherStepIn,
            "smootherstepout" => Self::SmootherStepOut,
            "smootherstepinout" => Self::SmootherStepInOut,
            _ => Self::Linear,
        }
    }

    /// Evaluate the easing at t ∈ [0, 1], returning a value in ~[0, 1].
    pub fn eval(self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::QuadIn => t * t,
            Self::QuadOut => -t * (t - 2.0),
            Self::QuadInOut => {
                let t = t * 2.0;
                if t < 1.0 { 0.5 * t * t }
                else { let t = t - 1.0; -0.5 * (t * (t - 2.0) - 1.0) }
            }
            Self::CubeIn => t * t * t,
            Self::CubeOut => { let t = t - 1.0; t * t * t + 1.0 }
            Self::CubeInOut => {
                let t = t * 2.0;
                if t < 1.0 { 0.5 * t * t * t }
                else { let t = t - 2.0; 0.5 * (t * t * t + 2.0) }
            }
            Self::QuartIn => t * t * t * t,
            Self::QuartOut => { let t = t - 1.0; -(t * t * t * t - 1.0) }
            Self::QuartInOut => {
                let t = t * 2.0;
                if t < 1.0 { 0.5 * t * t * t * t }
                else { let t = t - 2.0; -0.5 * (t * t * t * t - 2.0) }
            }
            Self::QuintIn => t * t * t * t * t,
            Self::QuintOut => { let t = t - 1.0; t * t * t * t * t + 1.0 }
            Self::QuintInOut => {
                let t = t * 2.0;
                if t < 1.0 { 0.5 * t * t * t * t * t }
                else { let t = t - 2.0; 0.5 * (t * t * t * t * t + 2.0) }
            }
            Self::SineIn => 1.0 - (t * PI / 2.0).cos(),
            Self::SineOut => (t * PI / 2.0).sin(),
            Self::SineInOut => -0.5 * ((PI * t).cos() - 1.0),
            Self::ExpoIn => {
                if t == 0.0 { 0.0 } else { 2.0_f32.powf(10.0 * (t - 1.0)) }
            }
            Self::ExpoOut => {
                if t == 1.0 { 1.0 } else { 1.0 - 2.0_f32.powf(-10.0 * t) }
            }
            Self::ExpoInOut => {
                if t == 0.0 { return 0.0; }
                if t == 1.0 { return 1.0; }
                let t = t * 2.0;
                if t < 1.0 { 0.5 * 2.0_f32.powf(10.0 * (t - 1.0)) }
                else { 0.5 * (2.0 - 2.0_f32.powf(-10.0 * (t - 1.0))) }
            }
            Self::CircIn => 1.0 - (1.0 - t * t).sqrt(),
            Self::CircOut => { let t = t - 1.0; (1.0 - t * t).sqrt() }
            Self::CircInOut => {
                let t = t * 2.0;
                if t < 1.0 { -0.5 * ((1.0 - t * t).sqrt() - 1.0) }
                else { let t = t - 2.0; 0.5 * ((1.0 - t * t).sqrt() + 1.0) }
            }
            Self::BackIn => {
                let s = 1.70158_f32;
                t * t * ((s + 1.0) * t - s)
            }
            Self::BackOut => {
                let s = 1.70158_f32;
                let t = t - 1.0;
                t * t * ((s + 1.0) * t + s) + 1.0
            }
            Self::BackInOut => {
                let s = 1.70158_f32 * 1.525;
                let t = t * 2.0;
                if t < 1.0 { 0.5 * (t * t * ((s + 1.0) * t - s)) }
                else { let t = t - 2.0; 0.5 * (t * t * ((s + 1.0) * t + s) + 2.0) }
            }
            Self::BounceOut => bounce_out(t),
            Self::BounceIn => 1.0 - bounce_out(1.0 - t),
            Self::BounceInOut => {
                if t < 0.5 { (1.0 - bounce_out(1.0 - 2.0 * t)) * 0.5 }
                else { bounce_out(2.0 * t - 1.0) * 0.5 + 0.5 }
            }
            Self::ElasticIn => {
                if t == 0.0 || t == 1.0 { return t; }
                let t = t - 1.0;
                -(2.0_f32.powf(10.0 * t) * (t * 10.0 - 10.75).sin() * (2.0 * PI / 3.0))
            }
            Self::ElasticOut => {
                if t == 0.0 || t == 1.0 { return t; }
                2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * (2.0 * PI / 3.0)).sin() + 1.0
            }
            Self::ElasticInOut => {
                if t == 0.0 || t == 1.0 { return t; }
                let t = t * 2.0;
                if t < 1.0 {
                    let t = t - 1.0;
                    -0.5 * 2.0_f32.powf(10.0 * t) * ((t * 10.0 - 10.75) * (2.0 * PI / 4.5)).sin()
                } else {
                    let t = t - 1.0;
                    2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * (2.0 * PI / 4.5)).sin() * 0.5 + 1.0
                }
            }
            Self::SmoothStepIn => smooth_step_in(t),
            Self::SmoothStepOut => smooth_step_out(t),
            Self::SmoothStepInOut => smooth_step_in_out(t),
            Self::SmootherStepIn => smoother_step_in(t),
            Self::SmootherStepOut => smoother_step_out(t),
            Self::SmootherStepInOut => smoother_step_in_out(t),
        }
    }
}

fn bounce_out(t: f32) -> f32 {
    if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        let t = t - 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        let t = t - 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        let t = t - 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    }
}

fn smooth_step_in(t: f32) -> f32 {
    // smoothstep is t*t*(3-2t), in-variant = 1 - smoothstep(1-t)
    let t2 = 1.0 - t;
    1.0 - t2 * t2 * (3.0 - 2.0 * t2)
}

fn smooth_step_out(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn smooth_step_in_out(t: f32) -> f32 {
    if t < 0.5 {
        let t = t * 2.0;
        0.5 * (1.0 - { let t2 = 1.0 - t; t2 * t2 * (3.0 - 2.0 * t2) })
    } else {
        let t = (t - 0.5) * 2.0;
        0.5 * t * t * (3.0 - 2.0 * t) + 0.5
    }
}

fn smoother_step_in(t: f32) -> f32 {
    let t2 = 1.0 - t;
    1.0 - t2 * t2 * t2 * (t2 * (t2 * 6.0 - 15.0) + 10.0)
}

fn smoother_step_out(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn smoother_step_in_out(t: f32) -> f32 {
    if t < 0.5 {
        let t = t * 2.0;
        0.5 * smoother_step_in(t)
    } else {
        let t = (t - 0.5) * 2.0;
        0.5 * smoother_step_out(t) + 0.5
    }
}

impl Tween {
    /// Get the current interpolated value.
    pub fn current_value(&self) -> f32 {
        let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
        let eased = self.ease.eval(t);
        self.start_value + (self.end_value - self.start_value) * eased
    }

    /// Advance the tween by dt seconds. Returns true if just finished.
    pub fn advance(&mut self, dt: f32) -> bool {
        if self.finished { return false; }
        self.elapsed += dt;
        if self.elapsed >= self.duration {
            self.elapsed = self.duration;
            self.finished = true;
            return true;
        }
        false
    }
}

/// A running timer created by runTimer.
#[derive(Debug, Clone)]
pub struct LuaTimer {
    pub tag: String,
    pub duration: f32,
    pub elapsed: f32,
    pub loops_total: i32,
    pub loops_done: i32,
    pub finished: bool,
}

impl LuaTimer {
    pub fn advance(&mut self, dt: f32) -> Vec<i32> {
        if self.finished { return Vec::new(); }
        let mut completions = Vec::new();
        self.elapsed += dt;
        while self.elapsed >= self.duration && !self.finished {
            self.elapsed -= self.duration;
            self.loops_done += 1;
            let remaining = if self.loops_total > 0 {
                self.loops_total - self.loops_done
            } else {
                // 0 = infinite loops
                1
            };
            completions.push(remaining);
            if self.loops_total > 0 && self.loops_done >= self.loops_total {
                self.finished = true;
            }
        }
        completions
    }
}

/// Manages all active tweens and timers.
pub struct TweenManager {
    pub tweens: HashMap<String, Tween>,
    pub timers: HashMap<String, LuaTimer>,
    /// Completed tween tags + their target vars, to fire onTweenCompleted callbacks.
    pub completed_tweens: Vec<(String, String)>,
    /// Completed timer ticks: (tag, loops_done, loops_remaining).
    pub completed_timers: Vec<(String, i32, i32)>,
    /// Tags of tweens that finished on the previous frame, pending removal.
    /// Kept for one extra frame so apply_to_sprites can apply the final value
    /// and has_active_tween checks prevent stale Lua table overwrites.
    pending_removal: Vec<String>,
}

impl TweenManager {
    pub fn new() -> Self {
        Self {
            tweens: HashMap::new(),
            timers: HashMap::new(),
            completed_tweens: Vec::new(),
            completed_timers: Vec::new(),
            pending_removal: Vec::new(),
        }
    }

    pub fn add_tween(&mut self, tween: Tween) {
        self.tweens.insert(tween.tag.clone(), tween);
    }

    pub fn cancel_tween(&mut self, tag: &str) {
        // Exact match
        self.tweens.remove(tag);
        // Also cancel sub-property tweens created by startTween (e.g., "tag_scale.x", "tag_y")
        let prefix = format!("{}_", tag);
        self.tweens.retain(|k, _| !k.starts_with(&prefix));
    }

    pub fn add_timer(&mut self, timer: LuaTimer) {
        self.timers.insert(timer.tag.clone(), timer);
    }

    pub fn cancel_timer(&mut self, tag: &str) {
        self.timers.remove(tag);
    }

    /// Update all tweens and timers.
    /// Finished tweens are kept in the HashMap for one extra frame so that
    /// apply_to_sprites can apply their final value and the Lua strum sync's
    /// has_active_tween check still sees them. They are removed on the NEXT
    /// call to update().
    pub fn update(&mut self, dt: f32) {
        // Remove tweens that finished on the PREVIOUS frame
        for tag in self.pending_removal.drain(..) {
            self.tweens.remove(&tag);
        }

        // Advance all remaining tweens
        for (tag, tween) in &mut self.tweens {
            if tween.advance(dt) {
                self.pending_removal.push(tag.clone());
                self.completed_tweens.push((tag.clone(), tween.target.clone()));
            }
        }

        // Update timers
        let mut finished_timer_tags = Vec::new();
        for (tag, timer) in &mut self.timers {
            let completions = timer.advance(dt);
            for remaining in completions {
                self.completed_timers.push((tag.clone(), timer.loops_done, remaining));
            }
            if timer.finished {
                finished_timer_tags.push(tag.clone());
            }
        }
        for tag in finished_timer_tags {
            self.timers.remove(&tag);
        }
    }

    /// Get the current value of a tween (if active).
    pub fn get_tween_value(&self, tag: &str) -> Option<(f32, &str, &TweenProperty)> {
        self.tweens.get(tag).map(|t| (t.current_value(), t.target.as_str(), &t.property))
    }

    /// Apply all active tween values to sprite state, strum state, and collect game property tweens.
    pub fn apply_to_sprites(
        &self,
        sprites: &mut HashMap<String, crate::script_state::LuaSprite>,
        strum_props: &mut [crate::script_state::StrumProps; 8],
    ) -> Vec<(String, TweenProperty, f32)> {
        let mut game_tweens = Vec::new();
        for tween in self.tweens.values() {
            let val = tween.current_value();

            // Variable tweens (__var_name): collect for custom_vars update
            if tween.target.starts_with("__var_") {
                game_tweens.push((tween.target.clone(), tween.property.clone(), val));
                continue;
            }

            // Check if it's a strum tween (__strum_opponent_N or __strum_player_N)
            if tween.target.starts_with("__strum_") {
                let idx = if tween.target.starts_with("__strum_opponent_") {
                    tween.target["__strum_opponent_".len()..].parse::<usize>().ok()
                } else if tween.target.starts_with("__strum_player_") {
                    tween.target["__strum_player_".len()..].parse::<usize>().map(|i| i + 4).ok()
                } else {
                    None
                };
                if let Some(si) = idx {
                    if si < 8 {
                        strum_props[si].custom = true;
                        match tween.property {
                            TweenProperty::X => strum_props[si].x = val,
                            TweenProperty::Y => strum_props[si].y = val,
                            TweenProperty::Alpha => strum_props[si].alpha = val,
                            TweenProperty::Angle => strum_props[si].angle = val,
                            TweenProperty::ScaleX => strum_props[si].scale_x = val,
                            TweenProperty::ScaleY => strum_props[si].scale_y = val,
                            _ => {}
                        }
                    }
                }
                continue;
            }

            if let Some(sprite) = sprites.get_mut(&tween.target) {
                match tween.property {
                    TweenProperty::X => sprite.x = val,
                    TweenProperty::Y => sprite.y = val,
                    TweenProperty::Alpha => sprite.alpha = val,
                    TweenProperty::Angle => sprite.angle = val,
                    TweenProperty::ScaleX => sprite.scale_x = val,
                    TweenProperty::ScaleY => sprite.scale_y = val,
                    TweenProperty::RedOffset => sprite.color_red_offset = val,
                    TweenProperty::GreenOffset => sprite.color_green_offset = val,
                    TweenProperty::BlueOffset => sprite.color_blue_offset = val,
                    TweenProperty::OffsetX => sprite.offset_x = val,
                    TweenProperty::OffsetY => sprite.offset_y = val,
                    TweenProperty::Zoom => {
                        game_tweens.push((tween.target.clone(), TweenProperty::Zoom, val));
                    }
                }
            } else {
                // Not a sprite — it's a game property tween (camera zoom, etc.)
                game_tweens.push((tween.target.clone(), tween.property.clone(), val));
            }
        }
        game_tweens
    }
}

impl Default for TweenManager {
    fn default() -> Self {
        Self::new()
    }
}
