use crate::js_bindings;
use crate::point_cloud;
use crate::render_ctx::{Point, RenderCtx};
use msg;

use glam::{Mat4, Vec3};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

use wgpu;

static SERVER_IP: &str = "192.168.1.10";

struct Connection {
    ws: WebSocket,
    incoming_msg_queue: Rc<RefCell<VecDeque<web_sys::js_sys::ArrayBuffer>>>,
}

impl Connection {
    fn new(url: &str) -> anyhow::Result<Self> {
        let ws = WebSocket::new(url)
            .map_err(|e| anyhow::Error::msg(format!("Failed to create WebSocket: {e:?}")))?;

        // NOTE(alberto): we use ArrayBuffer to receive binary data
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let incoming_msg_queue =
            Rc::new(RefCell::new(VecDeque::<web_sys::js_sys::ArrayBuffer>::new()));
        let tx = incoming_msg_queue.clone();

        // Callback to handle incoming WebSocket messages
        let onmessage_callback = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            log::info!("onmessage_callback");
            if let Ok(buffer) = e.data().dyn_into::<web_sys::js_sys::ArrayBuffer>() {
                log::info!("Received binary message ({}bytes)", buffer.byte_length());
                tx.borrow_mut().push_back(buffer);
            } else if let Ok(text) = e.data().dyn_into::<web_sys::js_sys::JsString>() {
                log::error!("Text messages are not supported. Received text message {text}");
            } else {
                log::error!("Received unsupported message type");
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
        let binary_message = message.to_bytes();
        // TODO(alberto): handle errors
        self.ws.send_with_u8_array(&binary_message).unwrap();
        Ok(())
    }

    fn try_receive_message(&self) -> anyhow::Result<Option<msg::response::Response>> {
        let opt_response = self
            .incoming_msg_queue
            .borrow_mut()
            .pop_front()
            .map(|msg| {
                let array = web_sys::js_sys::Uint8Array::new(&msg);
                let mut data = vec![0u8; array.length() as usize];
                array.copy_to(&mut data);
                msg::response::Response::from_bytes(&data)
            })
            .transpose()?;
        Ok(opt_response)
    }
}

struct RotateCmd {
    vec: Vec3,
    direction: f32,
}

impl RotateCmd {
    fn new(vec: Vec3, direction: f32) -> Self {
        assert!(direction == -1.0 || direction == 1.0);
        Self { vec, direction }
    }
}

enum UserInteraction {
    Rotation(RotateCmd),
    StatusRequest,
    StartRequest,
    StopRequest,
    DownloadPly,
}

pub struct App {
    connection: Option<Connection>,
    status: msg::response::Status,
    points: Vec<Point>,
    render_ctx: Option<RenderCtx>,
    time_s: f32,
    ui_state: UiState,
}

const ROTATION_SPEED: f32 = 0.1;

#[derive(Clone)]
struct UiState {
    freerun: bool,
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
            time_s: 0.0,
            ui_state: UiState { freerun: false },
        }
    }

    fn draw_ui(
        &self,
        ctx: &egui::Context,
        gpu_name: &str,
        websocket_state: u16,
        mut ui_state: UiState,
    ) -> (Vec<UserInteraction>, UiState) {
        fn add_resizable_image(ui: &mut egui::Ui, texture_id: eframe::epaint::TextureId) {
            const TEXTURE_WIDTH: f32 = 800_f32;
            const TEXTURE_HEIGHT: f32 = 600_f32;
            const ASPECT_RATIO: f32 = TEXTURE_WIDTH / TEXTURE_HEIGHT;
            let view_height = ui.available_height();
            let view_width = ui.available_width();

            let width_diff = view_width - TEXTURE_WIDTH;
            let height_diff = view_height - TEXTURE_HEIGHT;
            let scale = if width_diff < height_diff {
                view_width / TEXTURE_WIDTH
            } else {
                view_height / TEXTURE_HEIGHT
            };

            ui.add(
                egui::Image::new((texture_id, egui::Vec2::new(TEXTURE_WIDTH, TEXTURE_HEIGHT)))
                    .maintain_aspect_ratio(true)
                    .fit_to_original_size(scale),
            );
        }

        fn rotation_btn(ui: &mut egui::Ui, label: &str, vec: Vec3) -> Option<RotateCmd> {
            let mut rotation_cmd = None;
            ui.horizontal(|ui| {
                ui.label(label);
                if ui.button("-").is_pointer_button_down_on() {
                    rotation_cmd = Some(RotateCmd::new(vec, -1.0));
                }
                if ui.button("+").is_pointer_button_down_on() {
                    rotation_cmd = Some(RotateCmd::new(vec, 1.0));
                }
            });
            return rotation_cmd;
        }

        let mut commands = Vec::<UserInteraction>::new();
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

        egui::SidePanel::left("control_buttons").show(ctx, |ui| {
            ui.heading("3D Scanner");
            ui.label(format!("GPU: {gpu_name}"));
            let state_str = to_string(websocket_state);
            ui.label(format!("Connection state {state_str}"));

            ui.separator();

            ui.checkbox(&mut ui_state.freerun, "Freerun");

            if let Some(rot_cmd) = rotation_btn(ui, "X", Vec3::X) {
                commands.push(UserInteraction::Rotation(rot_cmd));
            }
            if let Some(rot_cmd) = rotation_btn(ui, "Y", Vec3::Y) {
                commands.push(UserInteraction::Rotation(rot_cmd));
            }
            if let Some(rot_cmd) = rotation_btn(ui, "Z", Vec3::X) {
                commands.push(UserInteraction::Rotation(rot_cmd));
            }

            if ui.button("Get Status").clicked() {
                commands.push(UserInteraction::StatusRequest);
            }

            if ui.button("Start").clicked() {
                commands.push(UserInteraction::StartRequest);
            }

            if ui.button("Stop").clicked() {
                commands.push(UserInteraction::StopRequest);
            }

            ui.separator();

            ui.label(format!("Motor speed: {}", self.status.motor_speed));
            ui.label(format!("Laser 1: {}", self.status.lasers.laser_1));
            ui.label(format!("Laser 2: {}", self.status.lasers.laser_2));

            if ui.button("Download point cloud").clicked() {
                commands.push(UserInteraction::DownloadPly);
            }

            ui.separator();

            let label = match self.render_ctx {
                Some(_) => "Some",
                None => "None",
            };
            ui.label(format!("Rendering pipeline context: {}", label));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(render_ctx) = &self.render_ctx {
                if let Some(texture_id) = render_ctx.texture_id {
                    add_resizable_image(ui, texture_id);
                }
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });

        (commands, ui_state)
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

impl eframe::App for App {
    /// Called by the frame work to save state before shutdown.
    //fn save(&mut self, storage: &mut dyn eframe::Storage) {
    //    eframe::set_value(storage, eframe::APP_KEY, self);
    //}

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.time_s += 0.01;
        let mut gpu_name = "Unknown GPU".to_string();
        if let Some(wgpu_state) = frame.wgpu_render_state() {
            let info = &wgpu_state.adapter;
            gpu_name = format!("{:?}", info);
            let device = &wgpu_state.device;

            if self.render_ctx.is_none() {
                log::info!("Setting up render pipeline");
                let mut render_ctx = RenderCtx::new(device);
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

            let ctx = self.render_ctx.as_mut().unwrap();

            let camera_matrix = ctx.camera_projection * ctx.camera_position;
            let view_proj_std140: [f32; 16] = camera_matrix.to_cols_array();

            let queue = &wgpu_state.queue;
            queue.write_buffer(
                &ctx.camera_staging_buffer,
                0,
                bytemuck::cast_slice(&view_proj_std140),
            );

            if self.points.is_empty() {
                // Show screensaver animation when no point cloud present
                let command_buffer = ctx.render_screensaver(queue, device, self.time_s);
                queue.submit(std::iter::once(command_buffer));
            } else {
                // Convert points to Pod `Point` and upload
                let num_points = self.points.len() as u32;
                ctx.ensure_vertex_capacity(device, queue, num_points);
                ctx.update_vertex_buffer(queue, &self.points);

                log::debug!("Rendering...");
                let command_buffer = ctx.render(&device, num_points);

                log::debug!("Submitting command buffer...");
                queue.submit(std::iter::once(command_buffer));
            }
        }

        let mut websocket_state = WebSocket::CLOSED;
        if let Some(conn) = &self.connection {
            websocket_state = conn.ws.ready_state();
        }

        let (commands, ui_state) =
            self.draw_ui(ctx, &gpu_name, websocket_state, self.ui_state.clone());
        self.ui_state = ui_state;

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

        let c = match websocket_state {
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
                        msg::response::Response::Error(e) => {
                            log::info!("Received Error: {e}");
                        }
                        msg::response::Response::Close => {
                            log::info!("Received Close");
                            //self.connection = None;
                        }
                        msg::response::Response::Status(status) => {
                            self.status = status;
                        }
                        msg::response::Response::PointCloud(pc) => {
                            for p in &pc.points {
                                let v = 10.0 * p;
                                self.points.push(Point::new(&v));
                            }
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

        for command in commands {
            match command {
                UserInteraction::Rotation(rot) => {
                    if let Some(ctx) = self.render_ctx.as_mut() {
                        let step = rot.direction * ROTATION_SPEED;
                        let q = glam::Quat::from_axis_angle(rot.vec, step);
                        ctx.camera_position = ctx.camera_position * Mat4::from_quat(q);
                    }
                }
                UserInteraction::StatusRequest => {
                    log::info!("Sending status request");
                    if let Some(conn) = &c {
                        let command = msg::command::Command::Status;
                        let res = conn.send_message(command);
                        if let Err(e) = res {
                            log::error!("Failed to send 'status' command: {}", e);
                        }
                    }
                }
                UserInteraction::StartRequest => {
                    log::info!("Sending start request");
                    if let Some(conn) = &c {
                        self.points.clear();
                        let command = msg::command::Command::Replay;
                        let res = conn.send_message(command);
                        if let Err(e) = res {
                            log::error!("Failed to send 'replay' command: {}", e);
                        }
                    }
                }
                UserInteraction::StopRequest => {
                    log::info!("Sending stop request");
                    if let Some(conn) = &c {
                        let command = msg::command::Command::Stop;
                        let res = conn.send_message(command);
                        if let Err(e) = res {
                            log::error!("Failed to send 'stop' command: {}", e);
                        }
                    }
                }
                UserInteraction::DownloadPly => {
                    // trigger download of point cloud in PLY format
                    let ply_data = point_cloud::ply_encode(&self.points);

                    let len = ply_data.len() as u32;
                    let ptr = ply_data.as_ptr() as u32;

                    let res =
                        js_bindings::save_streaming_file_blocking(ptr, len, "point_cloud.ply");
                    match res {
                        Ok(_) => {
                            log::info!("Point cloud download triggered");
                        }
                        Err(e) => {
                            log::error!("Failed to trigger point cloud download: {:?}", e);
                        }
                    }
                }
            }
        }

        if self.ui_state.freerun {
            ctx.request_repaint(); // triggers a repaint as soon as possible
        }
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
