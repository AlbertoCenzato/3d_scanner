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
use env_logger;
use log::{error, info, warn};
use msg::DEFAULT_SERVER_PORT;
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        image_dir: PathBuf,
        #[clap(default_value = "calibration.json")]
        calibration: PathBuf,
        #[clap(default_value = DEFAULT_SERVER_PORT)]
        port: u16,
        #[clap(default_value = "rerun+http://127.0.0.1:9876/proxy")]
        rerun_connection_string: String,
    },
    Motor {
        degrees: f32,
    },
}

fn main() -> Result<()> {
    env_logger::init(); // Initialize the logger

    let args = Cli::parse();

    let mut motor = make_stepper_motor()?;
    info!("Initialized {}", motor.name());

    match args.cmd {
        Commands::Motor { degrees } => {
            let steps_per_rev = motor.steps_per_rev();
            let steps = (degrees / 360_f32 * steps_per_rev) as u32;
            info!("Moving motor {} degrees, {} steps", degrees, steps);
            motor.step(steps);
        }
        Commands::Run {
            port,
            image_dir,
            calibration,
            rerun_connection_string,
        } => {
            #[cfg(feature = "camera")]
            let camera_type = cameras::CameraType::RaspberryPi;
            #[cfg(not(feature = "camera"))]
            let camera_type = cameras::CameraType::DiskLoader(image_dir.clone());

            info!("Initializing data logger...");
            let data_logger = logging::make_logger("Scanner3D", rerun_connection_string)?;

            info!("Initializing scanner...");
            let mut scanner = scanner::Scanner::new(camera_type, data_logger, &calibration)?;

            server::run_websocket_server(port, &mut scanner)?;
        }
    }

    info!("Bye.");

    Ok(())
}
