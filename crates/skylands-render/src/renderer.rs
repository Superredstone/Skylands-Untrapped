use crate::mesh::{GpuMesh, InstanceRaw};
use crate::pipeline::{
    create_render_pipeline, create_skybox_pipeline, CameraUniform, LightUniform,
};
use crate::texture::GpuTexture;
use crate::ui::{UiBatch, UiPipeline};
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
    pub skybox_pipeline: wgpu::RenderPipeline,
    pub ui_pipeline: UiPipeline,
    pub camera_buffer: wgpu::Buffer,
    pub light_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub skybox_bind_group: wgpu::BindGroup,
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

        let light_uniform = LightUniform::default();
        let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Light Buffer"),
            contents: bytemuck::cast_slice(&[light_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
                label: Some("Camera and Light Bind Group Layout"),
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: light_buffer.as_entire_binding(),
                },
            ],
            label: Some("Camera and Light Bind Group"),
        });

        let skybox_bind_group_layout =
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
                label: Some("Skybox Bind Group Layout"),
            });

        let skybox_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &skybox_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("Skybox Bind Group"),
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

        let skybox_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Skylands Skybox Pipeline Layout"),
                bind_group_layouts: &[&skybox_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = create_render_pipeline(
            &device,
            &pipeline_layout,
            config.format,
            Some(Self::DEPTH_FORMAT),
        );

        let skybox_pipeline = create_skybox_pipeline(
            &device,
            &skybox_pipeline_layout,
            config.format,
            Some(Self::DEPTH_FORMAT),
        );

        let ui_pipeline = UiPipeline::new(&device, &queue, config.format, width, height);

        let (depth_texture, depth_view) = Self::create_depth_texture(&device, width, height);

        Ok(Self {
            surface: Some(surface),
            device,
            queue,
            config,
            size: (width, height),
            render_pipeline,
            skybox_pipeline,
            ui_pipeline,
            camera_buffer,
            light_buffer,
            camera_bind_group,
            skybox_bind_group,
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

        let light_uniform = LightUniform::default();
        let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Light Buffer"),
            contents: bytemuck::cast_slice(&[light_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
                label: Some("Camera and Light Bind Group Layout"),
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: light_buffer.as_entire_binding(),
                },
            ],
            label: Some("Camera and Light Bind Group"),
        });

        let skybox_bind_group_layout =
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
                label: Some("Skybox Bind Group Layout"),
            });

        let skybox_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &skybox_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("Skybox Bind Group"),
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

        let skybox_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Skylands Skybox Pipeline Layout"),
                bind_group_layouts: &[&skybox_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = create_render_pipeline(
            &device,
            &pipeline_layout,
            config.format,
            Some(Self::DEPTH_FORMAT),
        );

        let skybox_pipeline = create_skybox_pipeline(
            &device,
            &skybox_pipeline_layout,
            config.format,
            Some(Self::DEPTH_FORMAT),
        );

        let ui_pipeline = UiPipeline::new(&device, &queue, config.format, width, height);

        let (depth_texture, depth_view) = Self::create_depth_texture(&device, width, height);

        Ok(Self {
            surface: None,
            device,
            queue,
            config,
            size: (width, height),
            render_pipeline,
            skybox_pipeline,
            ui_pipeline,
            camera_buffer,
            light_buffer,
            camera_bind_group,
            skybox_bind_group,
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
            self.ui_pipeline.resize(&self.queue, width, height);
        }
    }

    pub fn update_camera(&self, view_proj: [[f32; 4]; 4]) {
        let uniform = CameraUniform { view_proj };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    pub fn update_light(&self, light: LightUniform) {
        self.queue
            .write_buffer(&self.light_buffer, 0, bytemuck::cast_slice(&[light]));
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
        self.render_scene(None, items, clear_color)
    }

    pub fn render_scene(
        &mut self,
        skybox: Option<&GpuMesh>,
        items: &[(&GpuMesh, Option<&GpuTexture>, &[InstanceRaw])],
        clear_color: [f64; 4],
    ) -> Result<(), wgpu::SurfaceError> {
        self.render_scene_with_ui(skybox, items, None, clear_color)
    }

    pub fn render_scene_with_ui(
        &mut self,
        skybox: Option<&GpuMesh>,
        items: &[(&GpuMesh, Option<&GpuTexture>, &[InstanceRaw])],
        ui_batch: Option<&mut UiBatch>,
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

            // 1. Draw skybox if present
            if let Some(sb) = skybox {
                render_pass.set_pipeline(&self.skybox_pipeline);
                render_pass.set_bind_group(0, &self.skybox_bind_group, &[]);
                render_pass.set_vertex_buffer(0, sb.vertex_buffer.slice(..));
                render_pass.set_index_buffer(sb.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..sb.num_elements, 0, 0..1);
            }

            // 2. Draw scene items
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

        // 3. UI Pass if batch provided and not empty
        if let Some(batch) = ui_batch {
            batch.finish_draw_calls();
            if !batch.is_empty() {
                let v_buf = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("UI Vertex Buffer"),
                        contents: bytemuck::cast_slice(&batch.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                let i_buf = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("UI Index Buffer"),
                        contents: bytemuck::cast_slice(&batch.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });

                let mut ui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Skylands UI Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                ui_pass.set_pipeline(&self.ui_pipeline.pipeline);
                ui_pass.set_bind_group(0, &self.ui_pipeline.uniform_bind_group, &[]);
                ui_pass.set_vertex_buffer(0, v_buf.slice(..));
                ui_pass.set_index_buffer(i_buf.slice(..), wgpu::IndexFormat::Uint32);

                for call in &batch.draw_calls {
                    let tex_bind_group = if call.texture_id == 1 {
                        &self.ui_pipeline.font_atlas.texture.bind_group
                    } else {
                        &self.ui_pipeline.white_texture.bind_group
                    };
                    ui_pass.set_bind_group(1, tex_bind_group, &[]);
                    let end_idx = call.index_offset + call.index_count;
                    ui_pass.draw_indexed(call.index_offset..end_idx, 0, 0..1);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
