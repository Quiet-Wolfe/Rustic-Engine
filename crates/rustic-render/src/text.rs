use glyphon::{
    Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

/// Queued text draw call (game-space coordinates).
struct TextDraw {
    buffer: Buffer,
    x: f32,
    y: f32,
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
        let font_system = Self::create_font_system();
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
        // Layout width in game-space — will be scaled to pixels during render
        buffer.set_size(&mut self.font_system, Some(1280.0), None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        self.pending.push(TextDraw { buffer, x, y });
    }

    /// Prepare and render all queued text into the given render pass.
    /// `vp_w`/`vp_h` are the actual pixel dimensions of the letterboxed viewport.
    pub fn render<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'a>,
        game_w: f32,
        _game_h: f32,
        vp_w: f32,
        vp_h: f32,
    ) {
        if self.pending.is_empty() {
            return;
        }

        // Tell glyphon the actual pixel resolution so glyphs are rasterized sharply
        self.viewport.update(
            queue,
            Resolution {
                width: vp_w as u32,
                height: vp_h as u32,
            },
        );

        // Scale factor from game-space to viewport pixels
        let scale = vp_w / game_w;

        let text_areas: Vec<TextArea> = self
            .pending
            .iter()
            .map(|td| TextArea {
                buffer: &td.buffer,
                left: td.x * scale,
                top: td.y * scale,
                scale,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: vp_w as i32,
                    bottom: vp_h as i32,
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

    #[cfg(not(target_os = "android"))]
    fn create_font_system() -> FontSystem {
        FontSystem::new()
    }

    #[cfg(target_os = "android")]
    fn create_font_system() -> FontSystem {
        let mut db = glyphon::fontdb::Database::new();
        // Android doesn't have fontconfig — load system fonts manually
        db.load_fonts_dir("/system/fonts");
        db.set_monospace_family("Droid Sans Mono");
        db.set_sans_serif_family("Roboto");
        db.set_serif_family("Roboto");
        FontSystem::new_with_locale_and_db("en-US".to_string(), db)
    }
}
