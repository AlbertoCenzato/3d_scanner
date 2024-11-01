mod calibration;
mod cameras;
mod img_processing;
mod logging;
mod motor;

use calibration::load_calibration;
use motor::make_stepper_motor;

use logging::make_logger;

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
    }

    Ok(())
}
