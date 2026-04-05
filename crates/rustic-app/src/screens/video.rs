use std::path::PathBuf;

use ffmpeg_next as ffmpeg;
use ffmpeg::format::input;
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{self, Flags};
use ffmpeg::util::frame::video::Video;

/// Video playback state: decodes an MP4 file frame-by-frame using ffmpeg
/// and uploads each frame to a wgpu texture for rendering.
///
/// Usage: call `tick(dt)` in update, then `upload(queue)` + draw in the render pass.
pub struct VideoPlayer {
    /// Format context (kept alive for packet reading).
    ictx: ffmpeg::format::context::Input,
    decoder: ffmpeg::decoder::Video,
    scaler: scaling::Context,
    stream_index: usize,
    /// Time base for converting PTS to seconds.
    time_base: ffmpeg::util::rational::Rational,
    /// Duration of the video in seconds.
    #[allow(dead_code)]
    duration_secs: f64,
    /// Current playback position in seconds.
    position_secs: f64,
    /// Width/height of the video.
    width: u32,
    height: u32,
    /// wgpu texture for the current frame.
    texture: wgpu::Texture,
    /// Bind group for the texture.
    bind_group: wgpu::BindGroup,
    /// Whether playback has finished.
    finished: bool,
    /// Whether the video is playing.
    playing: bool,
    /// RGBA frame buffer (reused each decode).
    frame_buf: Vec<u8>,
    /// Whether the buffer has new data to upload.
    dirty: bool,
    /// Whether we've sent all packets to the decoder.
    flushed: bool,
    /// Path to video file (for logging).
    path: PathBuf,
    /// Lua callback to fire when video finishes.
    on_finish: Option<String>,
}

impl VideoPlayer {
    /// Open a video file and prepare for playback.
    pub fn new(
        path: &std::path::Path,
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Result<Self, String> {
        ffmpeg::init().map_err(|e| format!("ffmpeg init failed: {}", e))?;

        let ictx = input(path).map_err(|e| format!("can't open video {:?}: {}", path, e))?;
        let stream = ictx.streams().best(Type::Video)
            .ok_or_else(|| format!("no video stream in {:?}", path))?;
        let stream_index = stream.index();
        let time_base = stream.time_base();
        let duration_secs = if stream.duration() > 0 {
            stream.duration() as f64 * f64::from(time_base)
        } else {
            ictx.duration() as f64 / 1_000_000.0
        };

        let context_decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
            .map_err(|e| format!("can't create codec context: {}", e))?;
        let decoder = context_decoder.decoder().video()
            .map_err(|e| format!("can't open decoder: {}", e))?;

        let width = decoder.width();
        let height = decoder.height();

        let scaler = scaling::Context::get(
            decoder.format(),
            width, height,
            ffmpeg::util::format::pixel::Pixel::RGBA,
            width, height,
            Flags::BILINEAR,
        ).map_err(|e| format!("can't create scaler: {}", e))?;

        let (texture, bind_group) = Self::create_texture(device, texture_layout, sampler, width, height);

        log::info!("Video loaded: {:?} ({}x{}, {:.1}s)", path, width, height, duration_secs);

        Ok(Self {
            ictx,
            decoder,
            scaler,
            stream_index,
            time_base,
            duration_secs,
            position_secs: 0.0,
            width,
            height,
            texture,
            bind_group,
            finished: false,
            playing: true,
            frame_buf: vec![0u8; (width * height * 4) as usize],
            dirty: false,
            flushed: false,
            path: path.to_path_buf(),
            on_finish: None,
        })
    }

    fn create_texture(
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("video frame"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("video bind group"),
            layout: texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        });
        (texture, bind_group)
    }

    /// Send all available packets to the decoder, then signal EOF.
    fn send_all_packets(&mut self) {
        if self.flushed { return; }
        for (stream, packet) in &mut self.ictx.packets() {
            if stream.index() == self.stream_index {
                let _ = self.decoder.send_packet(&packet);
            }
        }
        let _ = self.decoder.send_eof();
        self.flushed = true;
    }

    /// Advance playback by dt seconds. Decodes frames to catch up to the new position.
    /// Call this from update(). Does NOT touch the GPU — call `upload()` in draw().
    pub fn tick(&mut self, dt: f64) {
        if !self.playing || self.finished { return; }

        self.position_secs += dt;

        // On first tick, send all packets at once (short videos)
        if !self.flushed {
            self.send_all_packets();
        }

        // Decode frames until we catch up to the target time
        loop {
            let mut frame = Video::empty();
            match self.decoder.receive_frame(&mut frame) {
                Ok(()) => {
                    let pts = frame.pts().unwrap_or(0) as f64 * f64::from(self.time_base);

                    // Scale to RGBA
                    let mut rgb_frame = Video::empty();
                    if self.scaler.run(&frame, &mut rgb_frame).is_ok() {
                        self.copy_frame_to_buf(&rgb_frame);
                        self.dirty = true;
                    }

                    // Stop if this frame is past our target
                    if pts >= self.position_secs {
                        return;
                    }
                }
                Err(ffmpeg::Error::Other { errno: libc::EAGAIN }) => {
                    // Shouldn't happen after send_eof, but handle gracefully
                    break;
                }
                Err(ffmpeg::Error::Other { errno: libc::EOF }) => {
                    self.finished = true;
                    self.playing = false;
                    log::info!("Video '{}' finished", self.path.display());
                    break;
                }
                Err(e) => {
                    log::warn!("Video decode error: {}", e);
                    break;
                }
            }
        }
    }

    /// Copy a decoded RGBA frame into our buffer, handling stride.
    fn copy_frame_to_buf(&mut self, rgb_frame: &Video) {
        let data = rgb_frame.data(0);
        let stride = rgb_frame.stride(0);
        let w = self.width as usize;
        let h = self.height as usize;

        if stride == w * 4 {
            let len = (w * h * 4).min(data.len()).min(self.frame_buf.len());
            self.frame_buf[..len].copy_from_slice(&data[..len]);
        } else {
            for row in 0..h {
                let src_off = row * stride;
                let dst_off = row * w * 4;
                let copy_len = w * 4;
                if src_off + copy_len <= data.len() && dst_off + copy_len <= self.frame_buf.len() {
                    self.frame_buf[dst_off..dst_off + copy_len]
                        .copy_from_slice(&data[src_off..src_off + copy_len]);
                }
            }
        }
    }

    /// Upload the decoded frame buffer to the GPU texture.
    /// Call this from draw() where the queue is available.
    pub fn upload(&mut self, queue: &wgpu::Queue) {
        if !self.dirty { return; }
        self.dirty = false;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.frame_buf,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
    }

    /// Get the bind group for rendering the current frame.
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// Video dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn is_finished(&self) -> bool { self.finished }
    pub fn is_playing(&self) -> bool { self.playing }
    pub fn stop(&mut self) { self.playing = false; self.finished = true; }

    pub fn set_on_finish(&mut self, callback: String) {
        self.on_finish = Some(callback);
    }

    pub fn take_on_finish(&mut self) -> Option<String> {
        self.on_finish.take()
    }
}
