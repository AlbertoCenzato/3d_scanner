use msg;

use egui_plot::{Plot, PlotPoints, Points};
use glam::Vec3;
use serde_json;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

static SERVER_IP: &str = "192.168.1.12";

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

impl eframe::App for App {
    /// Called by the frame work to save state before shutdown.
    //fn save(&mut self, storage: &mut dyn eframe::Storage) {
    //    eframe::set_value(storage, eframe::APP_KEY, self);
    //}

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                        msg::response::Response::PointCloud(pc) => {
                            self.points = pc.points;
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

            ui.label("Point cloud:");
            if !self.points.is_empty() {
                let plot_points: PlotPoints = self
                    .points
                    .iter()
                    .map(|v| [v.x as f64, v.y as f64]) // project to XY plane
                    .collect::<Vec<[f64; 2]>>()
                    .into();

                let points = Points::new("Points", plot_points).radius(2.0);

                Plot::new("point_cloud_plot")
                    .view_aspect(1.0)
                    .show_axes([true, true])
                    .show(ui, |plot_ui| {
                        plot_ui.points(points);
                    });
            } else {
                ui.label("No points received yet");
            }

            //let laser = ui.checkbox(checked, text);

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
