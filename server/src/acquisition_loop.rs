use anyhow;
use image;

use msg::response::{PointCloud, Response};

use crate::imgproc;
use crate::motor;

use std::f32::consts::PI;

pub trait OpenCamera {
    fn get_image(&mut self) -> anyhow::Result<image::GrayImage>;
}

pub trait Camera {
    fn acquire_from_camera(&mut self, acquisition_loop: &mut AcquisitionLoop)
        -> anyhow::Result<()>;

    //fn calibration(&self) -> &calibration::Calibration;
}

pub struct AcquisitionLoop {
    pub motor: Box<dyn motor::StepperMotor>,
    pub img_processor: imgproc::ImageProcessor,
    pub scanned_data_queue: std::sync::mpsc::Sender<Response>,
}

impl AcquisitionLoop {
    pub fn run(&mut self, camera: &mut dyn OpenCamera) -> anyhow::Result<()> {
        let angle_per_step = 5_f32.to_radians();
        let steps = (2_f32 * PI / angle_per_step).ceil() as i32;
        for i in 0..steps {
            let image = camera.get_image()?;
            let new_points = self
                .img_processor
                .process_image(&image, i as i64, angle_per_step);

            let response = PointCloud { points: new_points };
            self.scanned_data_queue
                .send(Response::PointCloud(response))?;
            self.motor.step(1);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return Ok(());
    }

    pub fn stop(self) -> Box<dyn motor::StepperMotor> {
        self.motor
    }
}
