mod calibration;
mod cameras;
mod imgproc;
mod logging;
mod motor;
mod scanner;
mod server;

use motor::make_stepper_motor;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        port: u16,
        image_dir: PathBuf,
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
}

fn main() -> Result<()> {
    let args = Cli::parse();

    let mut motor = make_stepper_motor()?;
    println!("Initialized {}", motor.name());

    match args.cmd {
        Commands::Motor { degrees } => {
            let steps_per_rev = motor.steps_per_rev();
            let steps = (degrees / 360_f32 * steps_per_rev) as u32;
            println!("Moving motor {} degrees, {} steps", degrees, steps);
            motor.step(steps);
        }
        Commands::Run {
            port,
            image_dir,
            calibration,
            rerun_ip,
            rerun_port,
        } => {
            #[cfg(feature = "camera")]
            let camera_type = cameras::CameraType::RaspberryPi;
            #[cfg(not(feature = "camera"))]
            let camera_type = cameras::CameraType::DiskLoader(image_dir.clone());

            let reurn_server_address =
                std::net::SocketAddr::new(std::net::IpAddr::V4(rerun_ip), rerun_port);
            println!("Initializing scanner...");
            let mut scanner =
                scanner::Scanner::new(camera_type, reurn_server_address, &calibration)?;

            server::run_websocket_server(port, &mut scanner)?;
        }
    }

    Ok(())
}
