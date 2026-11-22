use bytemuck::{Pod, Zeroable};
use dock_core::physics::DotSlot;
use wgpu::util::DeviceExt;

const DOT_SHADER: &str = include_str!("../../../assets/shaders/dot.wgsl");
const MAX_DOTS: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DotGlobals {
    resolution: [f32; 2],
    is_dark: f32,
    scale_factor: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DotInstance {
    center: [f32; 2],
    radius: f32,
    opacity: f32,
}

pub struct DotPipeline {
    pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
}

impl DotPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let init_globals = DotGlobals {
            resolution: [1.0, 1.0],
            is_dark: 0.0,
            scale_factor: 1.0,
        };

        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("dot-globals"),
                                                       contents: bytemuck::bytes_of(&init_globals),
                                                       usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dot-bgl"),
                                                  entries: &[wgpu::BindGroupLayoutEntry {
                                                      binding: 0,
                                                      visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                                                      ty: wgpu::BindingType::Buffer {
                                                          ty: wgpu::BufferBindingType::Uniform,
                                                          has_dynamic_offset: false,
                                                          min_binding_size: None,
                                                      },
                                                      count: None,
                                                  }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dot-bg"),
                                                  layout: &bgl,
                                                  entries: &[wgpu::BindGroupEntry {
                                                      binding: 0,
                                                      resource: globals_buffer.as_entire_binding(),
                                                  }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dot-shader"),
                                                 source: wgpu::ShaderSource::Wgsl(DOT_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dot-pipeline-layout"),
                                                            bind_group_layouts: &[&bgl],
                                                            push_constant_ranges: &[],
        });

        const INSTANCE_ATTRS: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32];

        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<DotInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRS,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dot-pipeline"),
                                                     layout: Some(&pipeline_layout),
                                                     vertex: wgpu::VertexState {
                                                         module: &shader,
                                                         entry_point: Some("vs_main"),
                                                     compilation_options: Default::default(),
                                                     buffers: &[instance_layout],
                                                     },
                                                     fragment: Some(wgpu::FragmentState {
                                                         module: &shader,
                                                         entry_point: Some("fs_main"),
                                                                    compilation_options: Default::default(),
                                                                    targets: &[Some(wgpu::ColorTargetState {
                                                                        format,
                                                                            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                                                                                    write_mask: wgpu::ColorWrites::ALL,
                                                                    })],
                                                     }),
                                                     primitive: wgpu::PrimitiveState::default(),
                                                     depth_stencil: None,
                                                     multisample: wgpu::MultisampleState::default(),
                                                     multiview: None,
                                                     cache: None,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dot-instances"),
                                                   size: (MAX_DOTS * std::mem::size_of::<DotInstance>()) as u64,
                                                   usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                                                   mapped_at_creation: false,
        });

        Self {
            pipeline,
            globals_buffer,
            bind_group,
            instance_buffer,
        }
    }

    pub fn prepare(
        &self,
        queue: &wgpu::Queue,
        resolution: [f32; 2],
        is_dark: f32,
        scale_factor: f32,
        dots: &[DotSlot],
    ) -> u32 {
        let count = dots.len().min(MAX_DOTS);
        if count == 0 {
            return 0;
        }

        let globals = DotGlobals {
            resolution,
            is_dark,
            scale_factor,
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        let instances: Vec<DotInstance> = dots
        .iter()
        .take(count)
        .map(|d| DotInstance {
            center: [d.x * scale_factor, d.y * scale_factor],
            radius: d.radius * scale_factor,
            opacity: d.opacity,
        })
        .collect();
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));

        count as u32
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, count: u32) {
        if count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, 0..count);
    }
}
