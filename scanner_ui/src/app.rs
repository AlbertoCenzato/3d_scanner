use msg;

use eframe::egui_wgpu;
use eframe::epaint;
use egui_plot::{Plot, PlotPoints, Points};
use glam::Vec3;
use serde_json;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};
use wgpu;
use wgpu::util::DeviceExt;
use wgpu::TextureFormat;

static SERVER_IP: &str = "192.168.1.12";
const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

struct Connection {
    ws: WebSocket,
    incoming_msg_queue: Rc<RefCell<VecDeque<String>>>,
}

impl Connection {
    fn new(url: &str) -> anyhow::Result<Self> {
        let ws = WebSocket::new(url)
            .map_err(|e| anyhow::Error::msg(format!("Failed to create WebSocket: {e:?}")))?;
        let incoming_msg_queue = Rc::new(RefCell::new(VecDeque::<String>::new()));
        let tx = incoming_msg_queue.clone();

        // Callback to handle incoming WebSocket messages
        let onmessage_callback = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            log::info!("onmessage_callback");
            match e.data().as_string() {
                Some(txt) => {
                    log::info!("Received message {txt}");
                    tx.borrow_mut().push_back(txt)
                }
                None => log::error!("Failed to convert message to string"),
            }
        });
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget(); // Keep the callback from being dropped

        let onerror_callback = Closure::<dyn FnMut(_)>::new(move |event: web_sys::Event| {
            log::error!("WebSocket error: {:?}", event);
        });
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let onopen_callback = Closure::<dyn FnMut(_)>::new(move |_: web_sys::Event| {
            log::info!("WebSocket connection opened");
        });
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        let onclose_callback = Closure::<dyn FnMut(_)>::new(move |_: web_sys::Event| {
            log::info!("WebSocket connection closed");
        });
        ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
        onclose_callback.forget();

        Ok(Connection {
            ws,
            incoming_msg_queue,
        })
    }

    fn send_message(&self, message: msg::command::Command) -> anyhow::Result<()> {
        let json_message = serde_json::to_string(&message)?;
        // TODO(alberto): handle errors
        self.ws.send_with_str(&json_message).unwrap();
        Ok(())
    }

    fn try_receive_message(&self) -> anyhow::Result<Option<msg::response::Response>> {
        let opt_response = self
            .incoming_msg_queue
            .borrow_mut()
            .pop_front()
            .map(|msg| serde_json::from_str(&msg))
            .transpose()?;
        Ok(opt_response)
    }
}

pub struct App {
    connection: Option<Connection>,
    status: msg::response::Status,
    points: Vec<glam::Vec3>,
    render_ctx: Option<RenderCtx>,
}

impl App {
    /// Called once before the first frame.
    pub fn new(_: &eframe::CreationContext<'_>) -> Self {
        log::info!("Initializing app");
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        //if let Some(storage) = cc.storage {
        //    return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        //}

        App {
            connection: None,
            status: msg::response::Status {
                lasers: msg::response::LasersData {
                    laser_1: false,
                    laser_2: false,
                },
                motor_speed: 0_f32,
            },
            points: Vec::new(),
            render_ctx: None,
        }
    }
}

fn to_string(ws_state: u16) -> String {
    let state_str = match ws_state {
        WebSocket::CONNECTING => "connecting",
        WebSocket::OPEN => "open",
        WebSocket::CLOSING => "closing",
        WebSocket::CLOSED => "closed",
        _ => "unknown",
    };

    return state_str.to_string();
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Point {
    position: [f32; 3],
    _padding: f32, // Ensure 16-byte alignment
}

fn init_camera_matrix(width: u32, height: u32) -> glam::Mat4 {
    let eye = glam::Vec3::new(0.0, 0.0, 5.0); // Camera position
    let target = glam::Vec3::ZERO; // Looking at origin
    let up = glam::Vec3::Y; // Up direction
    let view = glam::Mat4::look_at_rh(eye, target, up);
    let fovy = std::f32::consts::FRAC_PI_4; // 45 degrees
    let aspect = width as f32 / height as f32;
    let near = 0.1;
    let far = 100.0;

    let projection = glam::Mat4::perspective_rh_gl(fovy, aspect, near, far);
    return projection * view;
}

struct RenderCtx {
    shader: wgpu::ShaderModule,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
    pointcloud_texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    texture_id: Option<epaint::TextureId>,
}

impl RenderCtx {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> RenderCtx {
        let shader = device.create_shader_module(wgpu::include_wgsl!("point_cloud.wgsl"));

        let camera_matrix = init_camera_matrix(width, height);
        let view_proj_std140: [f32; 16] = camera_matrix.to_cols_array();
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[view_proj_std140]), // mat4x4<f32>
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
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

        return RenderCtx {
            shader,
            camera_buffer,
            camera_bind_group,
            render_pipeline,
            pointcloud_texture,
            texture_view,
            texture_id: None,
        };
    }

    fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertex_buffer: &wgpu::Buffer,
        num_points: u32,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

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
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, Some(&self.camera_bind_group), &[]);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.draw(0..num_points, 0..1);
        drop(render_pass);

        queue.submit(std::iter::once(encoder.finish()));
    }
}

impl eframe::App for App {
    /// Called by the frame work to save state before shutdown.
    //fn save(&mut self, storage: &mut dyn eframe::Storage) {
    //    eframe::set_value(storage, eframe::APP_KEY, self);
    //}

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut gpu_name = "Unknown GPU".to_string();
        if let Some(wgpu_state) = frame.wgpu_render_state() {
            let info = &wgpu_state.adapter;
            gpu_name = format!("{:?}", info);
            let device = &wgpu_state.device;
            let queue = &wgpu_state.queue;

            if self.render_ctx.is_none() {
                log::info!("Setting up render pipeline");
                let mut render_ctx = RenderCtx::new(device, 800, 600);
                let mut renderer = wgpu_state.renderer.write();
                let texture_id = renderer.register_native_texture(
                    device,
                    &render_ctx.texture_view,
                    wgpu::FilterMode::Linear,
                );
                log::info!("Render pipeline setup complete!");
                render_ctx.texture_id = Some(texture_id);
                self.render_ctx = Some(render_ctx);
            }

            if self.points.len() > 0 {
                let point_data: Vec<Point> = self
                    .points
                    .iter()
                    .map(|p| Point {
                        position: [50.0 * p.x, 50.0 * p.y, 10.0 * p.z],
                        _padding: 0.0,
                    })
                    .collect();

                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Point Cloud Vertex Buffer"),
                    contents: bytemuck::cast_slice(&point_data),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let ctx = self.render_ctx.as_ref().unwrap();
                ctx.render(&device, &queue, &vertex_buffer, point_data.len() as u32);
            }
        }

        if self.connection.is_none() {
            let port = msg::DEFAULT_SERVER_PORT;
            let url = format!("ws://{SERVER_IP}:{port}");
            log::info!("Attempting connection to {url}");
            let connection = Connection::new(&url);
            match connection {
                Ok(conn) => {
                    log::info!("Connected to {url}");
                    self.connection = Some(conn);
                }
                Err(e) => {
                    log::error!("Failed to connect to {url}: {e}");
                }
            }
        }

        let mut state = WebSocket::CLOSED;
        if let Some(conn) = &self.connection {
            state = conn.ws.ready_state();
        }

        let c = match state {
            WebSocket::OPEN => Some(self.connection.as_mut().unwrap()),
            WebSocket::CONNECTING => None,
            WebSocket::CLOSING => None,
            WebSocket::CLOSED => {
                self.connection = None;
                None
            }
            _ => None,
        };

        if let Some(conn) = &c {
            match conn.try_receive_message() {
                Ok(msg_opt) => match msg_opt {
                    Some(msg) => match msg {
                        msg::response::Response::Ok => {
                            log::info!("Received OK");
                        }
                        msg::response::Response::Error => {
                            log::info!("Received Error");
                        }
                        msg::response::Response::Close => {
                            log::info!("Received Close");
                            //self.connection = None;
                        }
                        msg::response::Response::Status(status) => {
                            self.status = status;
                        }
                        msg::response::Response::PointCloud(mut pc) => {
                            self.points.append(&mut pc.points);
                            log::info!("Received PointCloud");
                        }
                    },
                    None => {
                        // No message received, nothing to do
                    }
                },
                Err(e) => {
                    log::error!("Failed to receive message: {e}");
                }
            }
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:
            egui::menu::bar(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("3D Scanner");
            ui.label(format!("GPU: {gpu_name}"));
            let state_str = to_string(state);
            ui.label(format!("Connection state {state_str}"));

            ui.separator();

            let status_button = ui.button("Get Status");
            if status_button.clicked() {
                log::info!("Sending status request");
                if let Some(conn) = &c {
                    let command = msg::command::Command::Status;
                    let res = conn.send_message(command);
                    if let Err(e) = res {
                        log::error!("Failed to send 'status' command: {}", e);
                    }
                }
            }

            let start_button = ui.button("Start");
            if start_button.clicked() {
                log::info!("Sending start request");
                if let Some(conn) = &c {
                    let command = msg::command::Command::Replay;
                    let res = conn.send_message(command);
                    if let Err(e) = res {
                        log::error!("Failed to send 'replay' command: {}", e);
                    }
                }
            }

            ui.separator();

            ui.label(format!("Motor speed: {}", self.status.motor_speed));
            ui.label(format!("Laser 1: {}", self.status.lasers.laser_1));
            ui.label(format!("Laser 2: {}", self.status.lasers.laser_2));

            ui.separator();

            let label = match self.render_ctx {
                Some(_) => "Some",
                None => "None",
            };
            ui.label(format!("Rendering pipeline context: {}", label));

            ui.separator();

            ui.label("Point cloud:");

            let ctx = self.render_ctx.as_mut().unwrap();
            ui.image((ctx.texture_id.unwrap(), egui::Vec2::new(800.0, 600.0)));

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });

        ctx.request_repaint(); // triggers a repaint as soon as possible
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
