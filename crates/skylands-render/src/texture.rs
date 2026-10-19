use thiserror::Error;

#[derive(Error, Debug)]
pub enum TextureRenderError {
    #[error("Texture dimension error: width {0} or height {1} is 0")]
    ZeroDimensions(u32, u32),
    #[error("Buffer size mismatch: expected at least {expected} bytes, got {actual}")]
    BufferMismatch { expected: usize, actual: usize },
}

#[derive(Clone)]
pub struct GpuTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
}

impl GpuTexture {
    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    // ponytail: linear filtering default; mipmapping/anisotropy deferred until multi-lod support
    pub fn from_rgba(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
        label: Option<&str>,
    ) -> Result<Self, TextureRenderError> {
        if width == 0 || height == 0 {
            return Err(TextureRenderError::ZeroDimensions(width, height));
        }

        let expected_len = (width * height * 4) as usize;
        if rgba.len() < expected_len {
            return Err(TextureRenderError::BufferMismatch {
                expected: expected_len,
                actual: rgba.len(),
            });
        }

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Linear Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let layout = Self::bind_group_layout(device);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Ok(Self {
            texture,
            view,
            sampler,
            bind_group,
        })
    }

    pub fn create_fallback(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::from_rgba(
            device,
            queue,
            &[255, 255, 255, 255],
            1,
            1,
            Some("Fallback 1x1 White Texture"),
        )
        .expect("Valid fallback texture")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_dimensions_error() {
        let err = TextureRenderError::ZeroDimensions(0, 10);
        assert_eq!(
            err.to_string(),
            "Texture dimension error: width 0 or height 10 is 0"
        );
    }

    #[test]
    fn test_buffer_mismatch_error() {
        let err = TextureRenderError::BufferMismatch {
            expected: 64,
            actual: 16,
        };
        assert_eq!(
            err.to_string(),
            "Buffer size mismatch: expected at least 64 bytes, got 16"
        );
    }

    #[test]
    fn test_gpu_texture_init() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            })) {
                Some(a) => a,
                None => return,
            };

        let (device, queue) = match pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default(), None),
        ) {
            Ok(pair) => pair,
            Err(_) => return,
        };

        let fallback = GpuTexture::create_fallback(&device, &queue);
        assert_eq!(fallback.texture.width(), 1);
        assert_eq!(fallback.texture.height(), 1);

        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ];
        let tex = GpuTexture::from_rgba(&device, &queue, &rgba, 2, 2, Some("Test2x2"))
            .expect("Valid texture");
        assert_eq!(tex.texture.width(), 2);
        assert_eq!(tex.texture.height(), 2);
    }
}
