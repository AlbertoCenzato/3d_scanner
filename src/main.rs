mod calibration;
mod cameras;
mod img_processing;
mod logging;
mod motor;

use calibration::load_calibration;
use logging::make_logger;
use motor::make_stepper_motor;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use websocket::sync::Server;

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        image_dir: PathBuf,
        output_dir: PathBuf,
        #[clap(default_value = "calibration.json")]
        calibration: PathBuf,
        #[clap(default_value = "127.0.0.1")]
        rerun_ip: std::net::Ipv4Addr,
        #[clap(default_value = "9876")]
        rerun_port: u16,
    },
    Motor {
        degrees: f32,
    },
    Server {
        port: u16,
    },
}

fn main() -> Result<()> {
    let args = Cli::parse();

    let mut motor = make_stepper_motor()?;
    println!("Initialized {}", motor.name());

    match args.cmd {
        Commands::Run {
            image_dir,
            output_dir,
            calibration,
            rerun_ip,
            rerun_port,
        } => {
            #[cfg(feature = "camera")]
            let camera_type = cameras::CameraType::RaspberryPi;
            #[cfg(not(feature = "camera"))]
            let camera_type = cameras::CameraType::DiskLoader(image_dir.clone());

            let camera = cameras::make_camera(camera_type)?;
            let calib = load_calibration(&calibration)?;

            let reurn_server_address =
                std::net::SocketAddr::new(std::net::IpAddr::V4(rerun_ip), rerun_port);
            let rec = make_logger("3d_scanner", reurn_server_address)?;
            rec.log_camera("world/camera", &calib.camera)?;

            println!("Processing files from {}", image_dir.display());

            let _point_cloud = camera.acquire_from_camera(rec.as_ref(), &calib, motor.as_mut())?;
        }
        Commands::Motor { degrees } => {
            let steps_per_rev = motor.steps_per_rev();
            let steps = (degrees / 360_f32 * steps_per_rev) as u32;
            println!("Moving motor {} degrees, {} steps", degrees, steps);
            motor.step(steps);
        }
        Commands::Server { port } => {
            let connection_string = format!("0.0.0.0:{}", port);
            let server = Server::bind(connection_string)?;
            println!("WebSocket server listening on port {}", port);

            for request in server.filter_map(Result::ok) {
                if let Ok(client) = request.accept() {
                    let peer_address = client.peer_addr()?;
                    println!("Connection from {}", peer_address);

                    let (mut receiver, mut sender) = client.split()?;
                    for message in receiver.incoming_messages() {
                        if let Err(e) = message {
                            println!("Error: {:?}", e);
                            break;
                        }

                        let message = message.unwrap();
                        println!("Received message: {:?}", message);
                        match message {
                            websocket::OwnedMessage::Close(_) => {
                                let message = websocket::Message::close();
                                sender.send_message(&message).unwrap();
                                println!("Client {} disconnected", peer_address);
                                break;
                            }
                            websocket::OwnedMessage::Ping(ping) => {
                                let message = websocket::Message::pong(ping);
                                sender.send_message(&message).unwrap();
                                println!("Ponged");
                            }
                            websocket::OwnedMessage::Text(text) => {
                                println!("Text message received: {}", text);
                            }
                            websocket::OwnedMessage::Binary(_) => {
                                println!("Binary message received, not supported");
                            }
                            _ => {
                                println!("Unsupported message type");
                            }
                        };
                    }
                    println!("Listening for incoming connections...");
                }
            }
        }
    }

    Ok(())
}
