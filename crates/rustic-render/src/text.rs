use glyphon::{
    Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

/// Queued text draw call.
struct TextDraw {
    buffer: Buffer,
    x: f32,
    y: f32,
    bounds_w: f32,
    bounds_h: f32,
}

/// Text rendering system backed by glyphon.
pub struct TextSystem {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    pending: Vec<TextDraw>,
}

impl TextSystem {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        let viewport = Viewport::new(device, &cache);

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            pending: Vec::new(),
        }
    }

    /// Queue text to be drawn this frame. Coordinates are in game-space pixels.
    pub fn draw_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
    ) {
        let metrics = Metrics::new(size, size * 1.2);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);

        let r = (color[0] * 255.0) as u8;
        let g = (color[1] * 255.0) as u8;
        let b = (color[2] * 255.0) as u8;
        let a = (color[3] * 255.0) as u8;

        buffer.set_text(
            &mut self.font_system,
            text,
            &glyphon::Attrs::new()
                .family(Family::Monospace)
                .color(Color::rgba(r, g, b, a)),
            Shaping::Basic,
            None,
        );
        buffer.set_size(&mut self.font_system, Some(1280.0), None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        self.pending.push(TextDraw {
            buffer,
            x,
            y,
            bounds_w: 1280.0,
            bounds_h: 720.0,
        });
    }

    /// Prepare and render all queued text into the given render pass.
    pub fn render<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'a>,
        game_w: f32,
        game_h: f32,
    ) {
        if self.pending.is_empty() {
            return;
        }

        self.viewport.update(
            queue,
            Resolution {
                width: game_w as u32,
                height: game_h as u32,
            },
        );

        let text_areas: Vec<TextArea> = self
            .pending
            .iter()
            .map(|td| TextArea {
                buffer: &td.buffer,
                left: td.x,
                top: td.y,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: td.bounds_w as i32,
                    bottom: td.bounds_h as i32,
                },
                default_color: Color::rgb(255, 255, 255),
                custom_glyphs: &[],
            })
            .collect();

        self.renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .expect("Failed to prepare text");

        self.renderer
            .render(&self.atlas, &self.viewport, pass)
            .expect("Failed to render text");

        self.pending.clear();
    }
}
