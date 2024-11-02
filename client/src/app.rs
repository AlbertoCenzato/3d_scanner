use serde_json;
use std::sync::mpsc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

struct Connection {
    ws: WebSocket,
    rx: mpsc::Receiver<String>,
}

impl Connection {
    fn new(url: &str) -> anyhow::Result<Self> {
        let ws = WebSocket::new(url).unwrap();
        let (tx, rx) = mpsc::channel();

        // Callback to handle incoming WebSocket messages
        let onmessage_callback = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if let Some(txt) = e.data().as_string() {
                tx.send(txt).unwrap();
            }
        });
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget(); // Keep the callback from being dropped

        Ok(Connection { ws, rx })
    }

    fn send_message(&self, message: MessageOut) -> anyhow::Result<()> {
        let (cmd_str, payload) = match message {
            MessageOut::Status => ("status", String::from("{}")),
            MessageOut::Motor(data) => ("motor", serde_json::to_string(&data)?),
            MessageOut::Lasers(data) => ("lasers", serde_json::to_string(&data)?),
        };
        let message = format!("{cmd_str};{payload}");

        // TODO(alberto): handle errors
        self.ws.send_with_str(&message).unwrap();
        Ok(())
    }

    fn try_receive_message(&self) -> anyhow::Result<MessageIn> {
        match self.rx.try_recv() {
            Ok(msg) => {
                let parts: Vec<&str> = msg.split(';').collect();
                let cmd = parts[0];
                let payload = parts[1];
                match cmd {
                    "status" => {
                        let status: Status = serde_json::from_str(payload).unwrap();
                        Ok(MessageIn::Status(status))
                    }
                    _ => Err(anyhow::Error::msg("Unknown command")),
                }
            }
            Err(_) => Err(anyhow::Error::msg("No message available")),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct Status {
    lasers: LasersData,
    motor: f32,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct LasersData {
    laser_1: bool,
    laser_2: bool,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct MotorData {
    speed: f32,
}

enum MessageIn {
    Status(Status),
}

enum MessageOut {
    Status,
    Motor(MotorData),
    Lasers(LasersData),
}

pub struct App {
    connection: Option<Connection>,
    status: Status,
}

impl App {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        //if let Some(storage) = cc.storage {
        //    return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        //}

        App {
            connection: None,
            status: Status {
                lasers: LasersData {
                    laser_1: false,
                    laser_2: false,
                },
                motor: 0_f32,
            },
        }
    }
}

impl eframe::App for App {
    /// Called by the frame work to save state before shutdown.
    //fn save(&mut self, storage: &mut dyn eframe::Storage) {
    //    eframe::set_value(storage, eframe::APP_KEY, self);
    //}

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.connection.is_none() {
            self.connection = Some(Connection::new("ws://localhost:12345").unwrap());
        }

        if let Some(conn) = &self.connection {
            match conn.try_receive_message() {
                Ok(MessageIn::Status(status)) => {
                    self.status = status;
                }
                Err(_) => {}
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

                egui::widgets::global_dark_light_mode_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("3D Scanner");
            if let Some(conn) = &self.connection {
                let ready_state = conn.ws.ready_state();
                match ready_state {
                    WebSocket::CONNECTING => {
                        ui.label("Connecting...");
                    }
                    WebSocket::OPEN => {
                        ui.label("Connected!");
                    }
                    WebSocket::CLOSING => {
                        ui.label("Closing...");
                    }
                    WebSocket::CLOSED => {
                        ui.label("Closed!");
                        self.connection = None;
                    }
                    _ => {
                        ui.label("Unknown state!");
                    }
                }
            }

            ui.separator();

            let fwd = ui.button("Forward");
            let bkw = ui.button("Backward");
            if fwd.is_pointer_button_down_on() {
                log::info!("Forward");
                if let Some(conn) = &self.connection {
                    let motor_data = MotorData { speed: 10_f32 };
                    conn.send_message(MessageOut::Motor(motor_data)).unwrap();
                }
            }
            if bkw.is_pointer_button_down_on() {
                log::info!("Backward");
                if let Some(conn) = &self.connection {
                    let motor_data = MotorData { speed: -10_f32 };
                    conn.send_message(MessageOut::Motor(motor_data)).unwrap();
                }
            }

            ui.separator();

            ui.label(format!("Motor speed: {}", self.status.motor));
            ui.label(format!("Laser 1: {}", self.status.lasers.laser_1));
            ui.label(format!("Laser 2: {}", self.status.lasers.laser_2));

            //let laser = ui.checkbox(checked, text);

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });
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
