use crate::{
    font::Atlas,
    particles::Particles,
    vt::{Grid, ATTR_CONT, ATTR_REVERSE, ATTR_UNDERLINE, BLACK},
};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ScreenU {
    screen: [f32; 2],
    pad: [f32; 2],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostU {
    size: [f32; 2],
    time: f32,
    glitch: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TextVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    is_bg: f32,
    pad: [f32; 3],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PartVertex {
    pos: [f32; 2],
    local: [f32; 2],
    color: [f32; 4],
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    atlas_tex: wgpu::Texture,
    atlas: Atlas,
    screen_ubo: wgpu::Buffer,
    post_ubo: wgpu::Buffer,
    text_bg: wgpu::BindGroup,
    part_bg: wgpu::BindGroup,
    post_bg: wgpu::BindGroup,
    text_pipe: wgpu::RenderPipeline,
    part_pipe: wgpu::RenderPipeline,
    post_pipe: wgpu::RenderPipeline,
    text_vbo: wgpu::Buffer,
    text_cap: usize,
    part_vbo: wgpu::Buffer,
    part_cap: usize,
    pub particles: Particles,
    time: f32,
}
impl Renderer {
    pub async fn new(window: Arc<Window>, fonts: Vec<crate::font::FontBlob>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("no GPU adapter")?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("kiraterm device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: caps.present_modes[0],
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let atlas = Atlas::new(fonts, 20.0)?;
        let atlas_tex = make_atlas(&device, &atlas);
        queue.write_texture(
            atlas_tex.as_image_copy(),
            &atlas.bitmap,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(atlas.width),
                rows_per_image: Some(atlas.height),
            },
            wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
        );
        let (scene, scene_view) = make_scene(&device, config.width, config.height);
        let screen_ubo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("screen ubo"),
            contents: bytemuck::bytes_of(&ScreenU {
                screen: [config.width as f32, config.height as f32],
                pad: [0.; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let post_ubo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("post ubo"),
            contents: bytemuck::bytes_of(&PostU {
                size: [config.width as f32, config.height as f32],
                time: 0.,
                glitch: 0.,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let text_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text layout"),
            entries: &[
                bgl(
                    0,
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                bgl(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                bgl(
                    2,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let part_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("part layout"),
            entries: &[bgl(
                0,
                wgpu::ShaderStages::VERTEX,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            )],
        });
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post layout"),
            entries: &[
                bgl(
                    0,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                bgl(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                bgl(
                    2,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let atlas_view = atlas_tex.create_view(&Default::default());
        let text_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text bg"),
            layout: &text_layout,
            entries: &[
                be(0, screen_ubo.as_entire_binding()),
                be(1, wgpu::BindingResource::TextureView(&atlas_view)),
                be(2, wgpu::BindingResource::Sampler(&sampler)),
            ],
        });
        let part_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("part bg"),
            layout: &part_layout,
            entries: &[be(0, screen_ubo.as_entire_binding())],
        });
        let post_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post bg"),
            layout: &post_layout,
            entries: &[
                be(0, post_ubo.as_entire_binding()),
                be(1, wgpu::BindingResource::TextureView(&scene_view)),
                be(2, wgpu::BindingResource::Sampler(&sampler)),
            ],
        });
        let text_pipe = pipeline(
            &device,
            "text",
            include_str!("shaders/text.wgsl"),
            &text_layout,
            wgpu::TextureFormat::Rgba8Unorm,
            Some(text_desc()),
            wgpu::BlendState::ALPHA_BLENDING,
        );
        let part_pipe = pipeline(
            &device,
            "particles",
            include_str!("shaders/particles.wgsl"),
            &part_layout,
            wgpu::TextureFormat::Rgba8Unorm,
            Some(part_desc()),
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            },
        );
        let post_pipe = pipeline(
            &device,
            "post",
            include_str!("shaders/post.wgsl"),
            &post_layout,
            format,
            None,
            wgpu::BlendState::REPLACE,
        );
        let text_cap = 1024;
        let part_cap = 1024;
        let text_vbo = vbo(&device, text_cap, "text vbo");
        let part_vbo = vbo(&device, part_cap, "part vbo");
        Ok(Self {
            surface,
            device,
            queue,
            config,
            scene,
            scene_view,
            atlas_tex,
            atlas,
            screen_ubo,
            post_ubo,
            text_bg,
            part_bg,
            post_bg,
            text_pipe,
            part_pipe,
            post_pipe,
            text_vbo,
            text_cap,
            part_vbo,
            part_cap,
            particles: Particles::default(),
            time: 0.,
        })
    }
    pub fn cell_size(&self) -> (f32, f32) {
        (self.atlas.cell_w, self.atlas.cell_h)
    }
    pub fn resize(&mut self, s: PhysicalSize<u32>) {
        if s.width == 0 || s.height == 0 {
            return;
        }
        self.config.width = s.width;
        self.config.height = s.height;
        self.surface.configure(&self.device, &self.config);
        let (t, v) = make_scene(&self.device, s.width, s.height);
        self.scene = t;
        self.scene_view = v;
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = self.post_pipe.get_bind_group_layout(0);
        self.post_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post bg"),
            layout: &layout,
            entries: &[
                be(0, self.post_ubo.as_entire_binding()),
                be(1, wgpu::BindingResource::TextureView(&self.scene_view)),
                be(2, wgpu::BindingResource::Sampler(&sampler)),
            ],
        });
    }
    pub fn render(&mut self, grid: &Grid, glitch: f32, dt: f32) -> Result<()> {
        self.time += dt;
        self.queue.write_buffer(
            &self.screen_ubo,
            0,
            bytemuck::bytes_of(&ScreenU {
                screen: [self.config.width as f32, self.config.height as f32],
                pad: [0.; 2],
            }),
        );
        self.queue.write_buffer(
            &self.post_ubo,
            0,
            bytemuck::bytes_of(&PostU {
                size: [self.config.width as f32, self.config.height as f32],
                time: self.time,
                glitch,
            }),
        );
        let tv = self.text_vertices(grid);
        if self.atlas.dirty {
            self.queue.write_texture(
                self.atlas_tex.as_image_copy(),
                &self.atlas.bitmap,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(self.atlas.width),
                    rows_per_image: Some(self.atlas.height),
                },
                wgpu::Extent3d {
                    width: self.atlas.width,
                    height: self.atlas.height,
                    depth_or_array_layers: 1,
                },
            );
            self.atlas.dirty = false;
        }
        let pv = self.part_vertices();
        ensure(
            &self.device,
            &mut self.text_vbo,
            &mut self.text_cap,
            tv.len(),
            "text vbo",
        );
        ensure(
            &self.device,
            &mut self.part_vbo,
            &mut self.part_cap,
            pv.len(),
            "part vbo",
        );
        self.queue
            .write_buffer(&self.text_vbo, 0, bytemuck::cast_slice(&tv));
        self.queue
            .write_buffer(&self.part_vbo, 0, bytemuck::cast_slice(&pv));
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                self.surface.configure(&self.device, &self.config);
                self.surface.get_current_texture()?
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.008,
                            g: 0.010,
                            b: 0.024,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            p.set_pipeline(&self.text_pipe);
            p.set_bind_group(0, &self.text_bg, &[]);
            p.set_vertex_buffer(0, self.text_vbo.slice(..));
            p.draw(0..tv.len() as u32, 0..1);
            p.set_pipeline(&self.part_pipe);
            p.set_bind_group(0, &self.part_bg, &[]);
            p.set_vertex_buffer(0, self.part_vbo.slice(..));
            p.draw(0..pv.len() as u32, 0..1);
        }
        {
            let mut p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            p.set_pipeline(&self.post_pipe);
            p.set_bind_group(0, &self.post_bg, &[]);
            p.draw(0..3, 0..1);
        }
        self.queue.submit(Some(enc.finish()));
        frame.present();
        Ok(())
    }
    fn text_vertices(&mut self, g: &Grid) -> Vec<TextVertex> {
        let cells = g.cells();
        for c in cells {
            self.atlas.get(c.ch);
        }
        let mut v = Vec::new();
        let (cw, ch) = (self.atlas.cell_w, self.atlas.cell_h);
        for y in 0..g.rows {
            for x in 0..g.cols {
                let c = cells[y * g.cols + x];
                if c.attrs & ATTR_CONT != 0 {
                    continue;
                }
                let (fg, bg) = if c.attrs & ATTR_REVERSE != 0 {
                    (c.bg, c.fg)
                } else {
                    (c.fg, c.bg)
                };
                let p = [x as f32 * cw, y as f32 * ch];
                if bg != BLACK {
                    quad_text(&mut v, p, [cw, ch], [0.; 4], rgba(bg), 1.);
                }
                if c.ch != ' ' && c.ch != '\0' {
                    let q = self.atlas.get(c.ch);
                    if q.width > 0 {
                        let gx = p[0] + q.xmin as f32;
                        let gy = p[1] + self.atlas.baseline - q.ymin as f32 - q.height as f32;
                        let uv = [
                            q.x as f32 / self.atlas.width as f32,
                            q.y as f32 / self.atlas.height as f32,
                            (q.x + q.width) as f32 / self.atlas.width as f32,
                            (q.y + q.height) as f32 / self.atlas.height as f32,
                        ];
                        quad_text(
                            &mut v,
                            [gx, gy],
                            [q.width as f32, q.height as f32],
                            uv,
                            rgba(fg),
                            0.,
                        );
                    }
                }
                if c.attrs & ATTR_UNDERLINE != 0 {
                    quad_text(&mut v, [p[0], p[1] + ch - 1.], [cw, 1.], [0.; 4], rgba(fg), 1.);
                }
            }
        }
        if g.cursor_visible {
            let a = (0.6 + 0.4 * (self.time * 4.).sin()).abs();
            quad_text(
                &mut v,
                [g.cx() as f32 * cw, g.cy() as f32 * ch + ch - 2.],
                [cw, 2.],
                [0.; 4],
                [0., 1., 0.86, a],
                1.,
            );
        }
        v
    }
    fn part_vertices(&self) -> Vec<PartVertex> {
        let mut v = Vec::with_capacity(self.particles.items.len() * 6);
        for p in &self.particles.items {
            let s = p.size * (0.5 + 0.5 * p.life);
            let c = [p.color[0], p.color[1], p.color[2], p.life.max(0.)];
            for &(x, y) in &[
                (-1., -1.),
                (1., -1.),
                (1., 1.),
                (-1., -1.),
                (1., 1.),
                (-1., 1.),
            ] {
                v.push(PartVertex {
                    pos: [p.pos[0] + x * s, p.pos[1] + y * s],
                    local: [x, y],
                    color: c,
                });
            }
        }
        v
    }
}
fn rgba(c: crate::vt::Color) -> [f32; 4] {
    [c.0 as f32 / 255., c.1 as f32 / 255., c.2 as f32 / 255., 1.]
}
fn quad_text(v: &mut Vec<TextVertex>, p: [f32; 2], s: [f32; 2], u: [f32; 4], c: [f32; 4], b: f32) {
    for &(x, y, tx, ty) in &[
        (0., 0., u[0], u[1]),
        (1., 0., u[2], u[1]),
        (1., 1., u[2], u[3]),
        (0., 0., u[0], u[1]),
        (1., 1., u[2], u[3]),
        (0., 1., u[0], u[3]),
    ] {
        v.push(TextVertex {
            pos: [p[0] + x * s[0], p[1] + y * s[1]],
            uv: [tx, ty],
            color: c,
            is_bg: b,
            pad: [0.; 3],
        })
    }
}
fn bgl(
    binding: u32,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    }
}
fn be(binding: u32, resource: wgpu::BindingResource) -> wgpu::BindGroupEntry {
    wgpu::BindGroupEntry { binding, resource }
}
fn make_atlas(d: &wgpu::Device, a: &Atlas) -> wgpu::Texture {
    d.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas"),
        size: wgpu::Extent3d {
            width: a.width,
            height: a.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
fn make_scene(d: &wgpu::Device, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let t = d.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene"),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let v = t.create_view(&Default::default());
    (t, v)
}
fn vbo(d: &wgpu::Device, n: usize, label: &str) -> wgpu::Buffer {
    d.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (n * std::mem::size_of::<TextVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
fn ensure(d: &wgpu::Device, b: &mut wgpu::Buffer, cap: &mut usize, n: usize, label: &str) {
    if n > *cap {
        *cap = n.next_power_of_two();
        *b = vbo(d, *cap, label)
    }
}
const TEXT_ATTRS: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Float32x4,3=>Float32];
const PART_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Float32x4];
fn text_desc() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<TextVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &TEXT_ATTRS,
    }
}
fn part_desc() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<PartVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &PART_ATTRS,
    }
}
fn pipeline(
    d: &wgpu::Device,
    label: &str,
    src: &str,
    bgl: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    vbuf: Option<wgpu::VertexBufferLayout>,
    blend: wgpu::BlendState,
) -> wgpu::RenderPipeline {
    let s = d.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(src.into()),
    });
    let l = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[bgl],
        push_constant_ranges: &[],
    });
    let bufs: Vec<_> = vbuf.into_iter().collect();
    d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&l),
        vertex: wgpu::VertexState {
            module: &s,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &bufs,
        },
        fragment: Some(wgpu::FragmentState {
            module: &s,
            entry_point: "fs_main",
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    })
}
