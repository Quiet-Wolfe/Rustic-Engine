use std::path::{Path, PathBuf};

pub struct VideoPlayer {
    path: PathBuf,
    on_finish: Option<String>,
    bind_group: wgpu::BindGroup,
}

impl VideoPlayer {
    pub fn new(
        path: &Path,
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Result<Self, String> {
        log::warn!(
            "Video playback is not supported on Android, skipping {:?}",
            path
        );

        // Create a 1x1 transparent texture
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dummy video frame"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dummy video bind group"),
            layout: texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        Ok(Self {
            path: path.to_path_buf(),
            on_finish: None,
            bind_group,
        })
    }

    pub fn set_on_finish(&mut self, cb: String) {
        self.on_finish = Some(cb);
    }

    pub fn on_finish(&self) -> Option<&str> {
        self.on_finish.as_deref()
    }

    pub fn stop(&mut self) {}

    pub fn audio_path(&self) -> Option<&Path> {
        None
    }

    pub fn tick(&mut self, _wall_clock_ms: f64) -> Option<()> {
        // Finish immediately on Android
        None
    }

    pub fn upload(&mut self, _queue: &wgpu::Queue) {}

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (1, 1)
    }
}
