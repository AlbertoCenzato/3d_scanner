use anyhow;
use image;

use msg::response::{PointCloud, Response};

use crate::imgproc;
use crate::motor;

use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub trait OpenCamera {
    fn get_image(&mut self) -> anyhow::Result<image::GrayImage>;
}

pub trait Camera: Send {
    fn acquire_from_camera(
        &mut self,
        stop_token: Arc<AtomicBool>,
        acquisition_loop: &mut AcquisitionLoop,
    ) -> anyhow::Result<()>;
}

pub struct AcquisitionLoop {
    pub motor: Box<dyn motor::StepperMotor>,
    pub img_processor: imgproc::ImageProcessor,
    pub scanned_data_queue: std::sync::mpsc::Sender<Response>,
}

impl AcquisitionLoop {
    pub fn run(
        &mut self,
        stop_token: Arc<AtomicBool>,
        camera: &mut dyn OpenCamera,
    ) -> anyhow::Result<()> {
        log::info!("AcquisitionLoop: start");
        let angle_per_step = 5_f32.to_radians();
        let steps = (2_f32 * PI / angle_per_step).ceil() as i32;
        for i in 0..steps {
            log::info!("AcquisitionLoop: step {i}/{steps}");
            if stop_token.load(Ordering::Relaxed) {
                log::info!("AcquisitionLoop: stop requested");
                break;
            }
            let image = camera.get_image()?;
            let new_points = self
                .img_processor
                .process_image(&image, i as i64, angle_per_step);

            let response = PointCloud { points: new_points };
            self.scanned_data_queue
                .send(Response::PointCloud(response))?;
            self.motor.step(1);
        }
        log::info!("AcquisitionLoop: stop");
        return Ok(());
    }

    pub fn stop(self) -> Box<dyn motor::StepperMotor> {
        self.motor
    }
}
