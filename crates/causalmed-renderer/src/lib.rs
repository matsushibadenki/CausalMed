//! Native GPU relationship preview. No textures, vertices or frames cross the JS bridge.
use causalmed_core::{Edge, EvidenceKind, Relation};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    count: u32,
    edge: Edge,
}

impl Renderer {
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
        edge: Edge,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(target).map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| e.to_string())?;
        let config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or("surface_unsupported")?;
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("relationship"),
            source: wgpu::ShaderSource::Wgsl(include_str!("graph.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("relationship"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x3],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(config.format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let geometry = geometry(&edge, width as f32 / height.max(1) as f32);
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rust-owned-geometry"),
            contents: bytemuck::cast_slice(&geometry),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vertices,
            count: geometry.len() as u32,
            edge,
        })
    }
    pub fn update(&mut self, edge: Edge) {
        self.edge = edge;
        let vertices = geometry(
            &self.edge,
            self.config.width as f32 / self.config.height as f32,
        );
        self.count = vertices.len() as u32;
        self.vertices = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rust-owned-geometry"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.update(self.edge.clone());
    }
    pub fn render(&mut self) -> Result<(), String> {
        let output = match self.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface
                    .get_current_texture()
                    .map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        };
        let view = output.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("relationship-preview"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.904,
                            g: 0.896,
                            b: 0.871,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.draw(0..self.count, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn geometry(edge: &Edge, aspect: f32) -> Vec<Vertex> {
    let mut vertices = vec![];
    let color = match edge.evidence {
        EvidenceKind::Intervention => [0.018, 0.28, 0.29],
        EvidenceKind::Observation => [0.06, 0.15, 0.32],
        EvidenceKind::Hypothesis => [0.50, 0.13, 0.035],
    };
    let mut tri = |a, b, c| {
        vertices.extend([
            Vertex { position: a, color },
            Vertex { position: b, color },
            Vertex { position: c, color },
        ])
    };
    // Evidence kind is also encoded in line pattern. Association is undirected.
    let segments = if edge.evidence == EvidenceKind::Intervention {
        1
    } else {
        12
    };
    for i in 0..segments {
        let x = -0.38 + i as f32 * 0.76 / segments as f32;
        let end = x + 0.76 / segments as f32 * if segments == 1 { 1.0 } else { 0.55 };
        tri([x, -0.007], [end, -0.007], [end, 0.007]);
        tri([x, -0.007], [end, 0.007], [x, 0.007]);
    }
    if edge.relation == Relation::Activates {
        tri([0.38, 0.0], [0.31, 0.06], [0.31, -0.06]);
    }
    if edge.relation == Relation::Inhibits {
        tri([0.36, -0.07], [0.38, -0.07], [0.38, 0.07]);
        tri([0.36, -0.07], [0.38, 0.07], [0.36, 0.07]);
    }
    for center in [-0.5, 0.5] {
        for i in 0..64 {
            let a = i as f32 * std::f32::consts::TAU / 64.0;
            let b = (i + 1) as f32 * std::f32::consts::TAU / 64.0;
            tri(
                [center, 0.0],
                [center + a.cos() * 0.09, a.sin() * 0.09 * aspect],
                [center + b.cos() * 0.09, b.sin() * 0.09 * aspect],
            );
        }
    }
    vertices
}
