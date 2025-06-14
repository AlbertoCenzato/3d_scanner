use bytemuck;
use eframe::epaint;
use glam::{Mat4, Vec3};
use wgpu::CommandBuffer;

const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Point {
    position: [f32; 3],
    _padding: f32, // Ensure 16-byte alignment
}

impl Point {
    pub fn new(vec: &Vec3) -> Point {
        Point {
            position: [vec.x, vec.y, vec.z],
            _padding: 0.0,
        }
    }
}

pub fn init_camera_matrix(width: u32, height: u32, camera_position: Vec3) -> Mat4 {
    let target = Vec3::ZERO; // Looking at origin
    let up = Vec3::Y; // Up direction
    let view = Mat4::look_at_rh(camera_position, target, up);
    let fovy = std::f32::consts::FRAC_PI_4; // 45 degrees
    let aspect = width as f32 / height as f32;
    let near = 0.1;
    let far = 100.0;

    let projection = Mat4::perspective_rh_gl(fovy, aspect, near, far);
    return projection * view;
}

pub struct RenderCtx {
    shader: wgpu::ShaderModule,
    pub camera_buffer_size: wgpu::BufferAddress,
    pub camera_staging_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
    pointcloud_texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub texture_id: Option<epaint::TextureId>,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl RenderCtx {
    pub fn new(device: &wgpu::Device) -> RenderCtx {
        let shader = device.create_shader_module(wgpu::include_wgsl!("point_cloud.wgsl"));

        let camera_buffer_size = std::mem::size_of::<[f32; 16]>() as wgpu::BufferAddress;
        let camera_staging_flags = wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
        let camera_staging_descriptor = wgpu::BufferDescriptor {
            label: Some("Camera Staging Buffer"),
            usage: camera_staging_flags,
            size: camera_buffer_size,
            mapped_at_creation: false,
        };
        let camera_staging_buffer = device.create_buffer(&camera_staging_descriptor);

        let camera_usage_flags = wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST;
        let camera_descriptor = wgpu::BufferDescriptor {
            label: Some("Camera Buffer"),
            usage: camera_usage_flags,
            size: camera_buffer_size,
            mapped_at_creation: false,
        };
        let camera_buffer = device.create_buffer(&camera_descriptor);

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
                label: Some("camera_bind_group_layout"),
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        // ------ setup texture to render to -------------
        let texture_extent = wgpu::Extent3d {
            width: 1024,
            height: 768,
            depth_or_array_layers: 1,
        };

        let pointcloud_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Point Cloud Render Target"),
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let texture_view = pointcloud_texture.create_view(&Default::default());

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&Default::default());

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Point Cloud Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Point>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x3,
                    }],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TEXTURE_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::PointList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        return RenderCtx {
            shader,
            camera_buffer_size,
            camera_staging_buffer,
            camera_buffer,
            camera_bind_group,
            render_pipeline,
            pointcloud_texture,
            texture_view,
            texture_id: None,
            depth_texture,
            depth_view,
        };
    }

    pub fn render(
        &self,
        device: &wgpu::Device,
        vertex_buffer: &wgpu::Buffer,
        num_points: u32,
    ) -> CommandBuffer {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        encoder.copy_buffer_to_buffer(
            &self.camera_staging_buffer,
            0,
            &self.camera_buffer,
            0,
            self.camera_buffer_size,
        );
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
        render_pass.set_bind_group(0, Some(&self.camera_bind_group), &[]);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.draw(0..num_points, 0..1);
        drop(render_pass);

        return encoder.finish();
    }
}
