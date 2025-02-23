use crate::calibration;
use crate::cameras;
use crate::logging;
use crate::motor;

use websocket::sync::Writer;

pub struct Scanner {
    data_logger: Box<dyn logging::Logger>,
    motor: Box<dyn motor::StepperMotor>,
    camera: Box<dyn cameras::Camera>,
    calibration: calibration::Calibration,
    laser_1: bool,
    laser_2: bool,
}

impl Scanner {
    pub fn new(
        camera_type: cameras::CameraType,
        data_logger_address: std::net::SocketAddr,
        calibration_path: &std::path::Path,
    ) -> anyhow::Result<Self> {
        let data_logger = logging::make_logger("data_logger", data_logger_address)?;
        let motor = motor::make_stepper_motor()?;
        let camera = cameras::make_camera(camera_type)?;
        let calibration = calibration::load_calibration(&calibration_path)?;

        let scanner = Self {
            data_logger,
            motor,
            camera,
            calibration,
            laser_1: false,
            laser_2: false,
        };
        // TODO(alberto): should we return an error if camera logging fails?
        scanner
            .data_logger
            .log_camera("world/camera", &scanner.calibration.camera)?;
        Ok(scanner)
    }

    pub fn start(
        &mut self,
        websocket: Writer<websocket::stream::sync::TcpStream>,
    ) -> anyhow::Result<()> {
        let _ = self.camera.acquire_from_camera(
            self.data_logger.as_ref(),
            &self.calibration,
            self.motor.as_mut(),
        )?;
        //websocket.send_message(point_cloud);
        Ok(())
    }

    pub fn stop(&self) {}

    pub fn status(&self) -> msg::response::Status {
        msg::response::Status {
            lasers: msg::response::LasersData {
                laser_1: self.laser_1,
                laser_2: self.laser_2,
            },
            motor_speed: 0.0,
        }
    }
}
