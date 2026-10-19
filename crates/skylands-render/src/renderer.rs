use crate::mesh::{GpuMesh, InstanceRaw};
use crate::pipeline::{create_render_pipeline, CameraUniform};
use crate::texture::GpuTexture;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

pub struct Renderer {
    pub surface: Option<wgpu::Surface<'static>>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: (u32, u32),
    pub render_pipeline: wgpu::RenderPipeline,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub default_instance_buffer: wgpu::Buffer,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub fallback_texture: GpuTexture,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
}

impl Renderer {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub async fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("Failed to find suitable GPU adapter")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Skylands Render Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let camera_uniform = CameraUniform::default();
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("Camera Bind Group Layout"),
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("Camera Bind Group"),
        });

        let default_instance = InstanceRaw::default();
        let default_instance_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Default Identity Instance Buffer"),
                contents: bytemuck::cast_slice(&[default_instance]),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let texture_bind_group_layout = GpuTexture::bind_group_layout(&device);
        let fallback_texture = GpuTexture::create_fallback(&device, &queue);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Skylands Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = create_render_pipeline(
            &device,
            &pipeline_layout,
            config.format,
            Some(Self::DEPTH_FORMAT),
        );

        let (depth_texture, depth_view) = Self::create_depth_texture(&device, width, height);

        Ok(Self {
            surface: Some(surface),
            device,
            queue,
            config,
            size: (width, height),
            render_pipeline,
            camera_buffer,
            camera_bind_group,
            default_instance_buffer,
            texture_bind_group_layout,
            fallback_texture,
            depth_texture,
            depth_view,
        })
    }

    pub fn new_headless(width: u32, height: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let width = width.max(1);
        let height = height.max(1);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or("Failed to find suitable GPU adapter")?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Skylands Headless Render Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            },
            None,
        ))?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let camera_uniform = CameraUniform::default();
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("Camera Bind Group Layout"),
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("Camera Bind Group"),
        });

        let default_instance = InstanceRaw::default();
        let default_instance_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Default Identity Instance Buffer"),
                contents: bytemuck::cast_slice(&[default_instance]),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let texture_bind_group_layout = GpuTexture::bind_group_layout(&device);
        let fallback_texture = GpuTexture::create_fallback(&device, &queue);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Skylands Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = create_render_pipeline(
            &device,
            &pipeline_layout,
            config.format,
            Some(Self::DEPTH_FORMAT),
        );

        let (depth_texture, depth_view) = Self::create_depth_texture(&device, width, height);

        Ok(Self {
            surface: None,
            device,
            queue,
            config,
            size: (width, height),
            render_pipeline,
            camera_buffer,
            camera_bind_group,
            default_instance_buffer,
            texture_bind_group_layout,
            fallback_texture,
            depth_texture,
            depth_view,
        })
    }

    pub fn create_depth_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Skylands Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.size = (width, height);
            self.config.width = width;
            self.config.height = height;
            if let Some(surface) = &self.surface {
                surface.configure(&self.device, &self.config);
            }
            let (depth_texture, depth_view) =
                Self::create_depth_texture(&self.device, width, height);
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;
        }
    }

    pub fn update_camera(&self, view_proj: [[f32; 4]; 4]) {
        let uniform = CameraUniform { view_proj };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    pub fn render(
        &mut self,
        meshes: &[&GpuMesh],
        clear_color: [f64; 4],
    ) -> Result<(), wgpu::SurfaceError> {
        let items: Vec<(&GpuMesh, Option<&GpuTexture>)> =
            meshes.iter().map(|m| (*m, None)).collect();
        self.render_textured(&items, clear_color)
    }

    pub fn render_textured(
        &mut self,
        items: &[(&GpuMesh, Option<&GpuTexture>)],
        clear_color: [f64; 4],
    ) -> Result<(), wgpu::SurfaceError> {
        let default_instances = [InstanceRaw::default()];
        let instanced_items: Vec<(&GpuMesh, Option<&GpuTexture>, &[InstanceRaw])> = items
            .iter()
            .map(|(m, t)| (*m, *t, default_instances.as_slice()))
            .collect();
        self.render_instanced(&instanced_items, clear_color)
    }

    pub fn render_instanced(
        &mut self,
        items: &[(&GpuMesh, Option<&GpuTexture>, &[InstanceRaw])],
        clear_color: [f64; 4],
    ) -> Result<(), wgpu::SurfaceError> {
        let surface = match &self.surface {
            Some(s) => s,
            None => return Ok(()),
        };

        let output = surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Skylands Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Skylands Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);

            for (mesh, maybe_tex, instances) in items {
                if instances.is_empty() {
                    continue;
                }
                let bind_group = match maybe_tex {
                    Some(tex) => &tex.bind_group,
                    None => &self.fallback_texture.bind_group,
                };
                render_pass.set_bind_group(1, bind_group, &[]);
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));

                let inst_buf;
                let (buf_slice, count) =
                    if instances.len() == 1 && instances[0] == InstanceRaw::default() {
                        (self.default_instance_buffer.slice(..), 1u32)
                    } else {
                        inst_buf =
                            self.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("Skylands Dynamic Instance Buffer"),
                                    contents: bytemuck::cast_slice(instances),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                        (inst_buf.slice(..), instances.len() as u32)
                    };

                render_pass.set_vertex_buffer(1, buf_slice);
                render_pass
                    .set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..mesh.num_elements, 0, 0..count);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
