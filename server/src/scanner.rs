use crate::acquisition_loop;
use crate::calibration;
use crate::cameras;
use crate::imgproc;
use crate::logging::LoggerHandle;
use crate::motor;
use log;

use anyhow::Ok;
use msg::response::Response;
use std::fmt;
use std::fmt::Display;
use std::sync::mpsc;

use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};
use std::thread;

pub struct IdleState {
    hw: HW,
}

impl IdleState {
    pub fn start(
        self,
        logger: LoggerHandle,
        calib: calibration::Calibration,
        scanned_data_queue: mpsc::Sender<msg::response::Response>,
    ) -> RunningState {
        let hw = self.hw;
        let stop_token = Arc::new(AtomicBool::new(false));
        let stop = stop_token.clone();
        let thread_handle = thread::spawn(move || {
            log::info!("Acquisition thread: start");
            let res = run(stop, hw, logger, calib, scanned_data_queue);
            log::info!("Run finished: {:?}", res.1);
            log::info!("Acquisition thread: stop");
            return res;
        });
        RunningState {
            stop_token,
            thread_handle,
        }
    }
}

pub struct RunningState {
    stop_token: Arc<AtomicBool>,
    thread_handle: thread::JoinHandle<(IdleState, anyhow::Result<()>)>,
}

impl RunningState {
    pub fn stop(self) -> (IdleState, anyhow::Result<()>) {
        self.stop_token.store(true, Ordering::Relaxed);
        self.thread_handle.join().unwrap()
    }
}

enum State {
    Idle(IdleState),
    Running(RunningState),
}

#[derive(Debug)]
pub enum ScannerError {
    AlreadyRunning,
    NotRunning,
    CommandNotImplemented(String),
    ExecutionError(anyhow::Error),
}

impl Display for ScannerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ScannerError::AlreadyRunning => write!(f, "Scanner is already running"),
            ScannerError::NotRunning => write!(f, "Scanner is not running"),
            ScannerError::CommandNotImplemented(command) => {
                write!(f, "Command '{}' not implemented", command)
            }
            ScannerError::ExecutionError(error) => {
                write!(f, "Execution error: {}", error)
            }
        }
    }
}

impl std::error::Error for ScannerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScannerError::AlreadyRunning => None,
            ScannerError::NotRunning => None,
            ScannerError::CommandNotImplemented(_) => None,
            ScannerError::ExecutionError(error) => error.source(),
        }
    }
}

struct HW {
    motor: Box<dyn motor::StepperMotor + Send>,
    camera: Box<dyn acquisition_loop::Camera + Send>,
}

pub struct Scanner {
    data_logger: LoggerHandle,
    state: Option<State>,
    calibration: calibration::Calibration,
    laser_1: bool,
    laser_2: bool,
    motor_position: f32,
}

impl Scanner {
    pub fn new(
        camera_type: cameras::CameraType,
        data_logger: LoggerHandle,
        calibration_path: &std::path::Path,
    ) -> anyhow::Result<Self> {
        let default_calibration = calibration::load_calibration(&calibration_path)?;
        let camera = cameras::make_camera(camera_type)?;
        let motor = motor::make_stepper_motor()?;

        let e = data_logger.log_camera("world/camera", &default_calibration.camera);
        if let Some(error) = e.err() {
            log::warn!("Failed to log camera data: {error}");
        }

        let hw = HW { motor, camera };
        let state = State::Idle(IdleState { hw });
        let scanner = Self {
            data_logger,
            state: Some(state),
            calibration: default_calibration,
            laser_1: false,
            laser_2: false,
            motor_position: 0_f32,
        };
        Ok(scanner)
    }

    pub fn start(
        &mut self,
        scanned_data_queue: mpsc::Sender<Response>,
    ) -> Result<(), ScannerError> {
        assert!(self.state.is_some(), "Scanner state is not initialized");

        let state = self.state.take();
        let result = match state.unwrap() {
            State::Idle(idle_state) => {
                let running = idle_state.start(
                    self.data_logger.clone(),
                    self.calibration.clone(),
                    scanned_data_queue,
                );
                self.state = Some(State::Running(running));
                Result::<(), ScannerError>::Ok(())
            }
            State::Running(running_state) => {
                self.state = Some(State::Running(running_state));
                Err(ScannerError::AlreadyRunning)
            }
        };
        result
    }

    pub fn stop(&mut self) -> Result<(), ScannerError> {
        assert!(self.state.is_some(), "Scanner state is not initialized");

        let state = self.state.take();
        let result = match state.unwrap() {
            State::Running(running_state) => {
                let (idle_state, result) = running_state.stop();
                self.state = Some(State::Idle(idle_state));
                result.map_err(|e| ScannerError::ExecutionError(e))
            }
            State::Idle(idle_state) => {
                self.state = Some(State::Idle(idle_state));
                Err(ScannerError::NotRunning)
            }
        };

        result
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

fn run(
    stop_token: Arc<AtomicBool>,
    hw: HW,
    data_logger: LoggerHandle,
    calib: calibration::Calibration,
    output_queue: mpsc::Sender<Response>,
) -> (IdleState, anyhow::Result<()>) {
    let img_processor = imgproc::ImageProcessor { data_logger, calib };

    let HW { motor, mut camera } = hw;

    let mut acq_loop = acquisition_loop::AcquisitionLoop {
        motor: motor,
        img_processor: img_processor,
        scanned_data_queue: output_queue,
    };

    let result = camera.acquire_from_camera(stop_token, &mut acq_loop);

    let motor = acq_loop.stop();

    let hw = HW { motor, camera };
    let state = IdleState { hw };
    return (state, result);
}
