use crate::acquisition_loop;
use crate::calibration;
use crate::cameras;
use crate::imgproc;
use crate::logging::LoggerHandle;
use crate::motor;

use anyhow::Ok;
use msg::response::Response;
use std::sync::mpsc;

use std::sync::Arc;
use std::thread;

struct IdleState {
    hw: HW,
}

impl IdleState {
    pub fn start(
        self,
        logger: Arc<LoggerHandle>,
        calib: calibration::Calibration,
        scanned_data_queue: mpsc::Sender<msg::response::Response>,
    ) -> RunningState {
        let hw = self.hw;
        let thread_handle = thread::spawn(move || {
            return run(hw, logger, calib, scanned_data_queue);
        });
        RunningState { thread_handle }
    }
}

struct RunningState {
    thread_handle: thread::JoinHandle<(IdleState, anyhow::Result<()>)>,
}

impl RunningState {
    pub fn stop(self) -> (IdleState, anyhow::Result<()>) {
    	// TODO: request stop
        self.thread_handle.join().unwrap()
    }
}

struct HW {
    motor: Box<dyn motor::StepperMotor + Send>,
    camera: Box<dyn acquisition_loop::Camera + Send>,
}

pub struct Scanner<State> {
    data_logger: Arc<LoggerHandle>,
    state: State,
    calibration: calibration::Calibration,
    laser_1: bool,
    laser_2: bool,
    motor_position: f32,
}

impl Scanner<IdleState> {
    pub fn new(
        camera_type: cameras::CameraType,
        data_logger: Arc<LoggerHandle>,
        calibration_path: &std::path::Path,
    ) -> anyhow::Result<Self> {
        let default_calibration = calibration::load_calibration(&calibration_path)?;
        let camera = cameras::make_camera(camera_type)?;
        data_logger.log_camera("world/camera", &default_calibration.camera)?;

        let motor = motor::make_stepper_motor()?;

        let scanner = Self {
            data_logger,
            status: ScannerAcquisitionStatus::Idle(HW { motor, camera }),
            calibration: default_calibration,
            laser_1: false,
            laser_2: false,
            motor_position: 0_f32,
        };
        // TODO(alberto): should we return an error if camera logging fails?
        Ok(scanner)
    }

    pub fn start(self, scanned_data_queue: mpsc::Sender<Response>) -> Scanner<RunningState> {
        let running = self.state.start(self.data_logger.clone(), self.calibration.clone(), scanned_data_queue);
        Scanner {
            data_logger: self.data_logger,
            state: RunningState {
                running,
            },
            calibration: self.calibration,
            laser_1: self.laser_1,
            laser_2: self.laser_2,
            motor_position: self.motor_position,
        }
    }
}

impl Scanner<RunningState> {
	pub fn stop(self) -> (Scanner<IdleState>, anyhow::Result<()>) {
        let (idle_state, result) = self.state.stop();
        let scanner = Scanner {
            data_logger: self.data_logger,
            state: idle_state,
            calibration: self.calibration,
            laser_1: self.laser_1,
            laser_2: self.laser_2,
            motor_position: self.motor_position,
        };
        (scanner, result)
    }
}

    /*
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
    */
}

fn run(
    hw: HW,
    data_logger: Arc<LoggerHandle>,
    calib: calibration::Calibration,
    output_queue: mpsc::Sender<Response>,
) -> (IdleState, anyhow::Result<()>) {
    let img_processor = imgproc::ImageProcessor {
        rec: data_logger,
        calib,
    };

    let HW { motor, mut camera } = hw;

    let mut acq_loop = acquisition_loop::AcquisitionLoop {
        motor: motor,
        img_processor: img_processor,
        scanned_data_queue: output_queue,
    };

    let result = camera.acquire_from_camera(&mut acq_loop);

    let motor = acq_loop.stop();

    return (HW { motor, camera }, result);
}
