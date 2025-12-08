use crate::acquisition_loop;
use crate::calibration;
use crate::cameras;
use crate::imgproc;
use crate::logging;
use crate::logging::Logger;
use crate::motor;

use msg::response::Response;
use std::sync::mpsc;

use std::sync::Arc;

pub struct Scanner {
    data_logger: Arc<dyn logging::Logger>,
    motor: Option<Box<dyn motor::StepperMotor>>,
    camera: Box<dyn acquisition_loop::Camera>,
    calibration: calibration::Calibration,
    laser_1: bool,
    laser_2: bool,
    motor_position: f32,
}

impl Scanner {
    pub fn new(
        camera_type: cameras::CameraType,
        data_logger: Arc<dyn Logger>,
        calibration_path: &std::path::Path,
    ) -> anyhow::Result<Self> {
        let default_calibration = calibration::load_calibration(&calibration_path)?;
        let camera = cameras::make_camera(camera_type)?;
        data_logger.log_camera("world/camera", &default_calibration.camera)?;

        let motor = motor::make_stepper_motor()?;

        let scanner = Self {
            data_logger,
            motor: Some(motor),
            camera,
            calibration: default_calibration,
            laser_1: false,
            laser_2: false,
            motor_position: 0_f32,
        };
        // TODO(alberto): should we return an error if camera logging fails?
        Ok(scanner)
    }

    pub fn start(&mut self, scanned_data_queue: mpsc::Sender<Response>) -> anyhow::Result<()> {
        let img_processor = imgproc::ImageProcessor {
            rec: self.data_logger.clone(),
            calib: self.calibration.clone(),
        };
        let mut acq_loop = acquisition_loop::AcquisitionLoop {
            motor: self.motor.take().expect("Motor not initialized"),
            img_processor: img_processor,
            scanned_data_queue: scanned_data_queue,
        };

        let _ = self.camera.acquire_from_camera(&mut acq_loop)?;

        let motor = acq_loop.stop();
        self.motor = Some(motor);
        return Ok(());
    }

    pub fn stop(&self) {}

    pub fn status(&mut self) -> msg::response::Status {
        self.motor_position += 1_f32;
        msg::response::Status {
            lasers: msg::response::LasersData {
                laser_1: self.laser_1,
                laser_2: self.laser_2,
            },
            motor_speed: self.motor_position,
        }
    }
}
