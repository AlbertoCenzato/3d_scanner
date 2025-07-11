use crate::calibration::CameraCalib;

use anyhow::Result;
use glam::Affine3A;
use image::DynamicImage;
use std::cfg;

pub trait Logger {
    fn log_transform(&self, id: &str, transform: &Affine3A) -> Result<()>;
    fn log_points(&self, id: &str, points: &[glam::Vec3]) -> Result<()>;
    fn log_image(&self, id: &str, image: DynamicImage) -> Result<()>;
    fn log_camera(&self, id: &str, camera: &CameraCalib) -> Result<()>;
    fn set_time_sequence(&self, id: &str, time: i64);
}

#[allow(unused)]
pub fn make_logger(logger_name: &str, address: std::net::SocketAddr) -> Result<Box<dyn Logger>> {
    #[cfg(feature = "rerun")]
    let logger: Box<dyn Logger> = Box::new(rerun::RerunLogger::new(&logger_name, address)?);
    #[cfg(not(feature = "rerun"))]
    let logger: Box<dyn Logger> = Box::new(NullLogger {});
    return Ok(logger);
}

#[cfg(feature = "rerun")]
pub mod rerun {
    use super::*;
    use ::rerun;

    const AXIS_SIZE: f32 = 0.1_f32;

    pub struct RerunLogger {
        pub rec: rerun::RecordingStream,
    }

    impl RerunLogger {
        pub fn new(name: &str, address: std::net::SocketAddr) -> Result<RerunLogger> {
            let connection_timeout = Some(std::time::Duration::from_secs(1));
            let rec = rerun::RecordingStreamBuilder::new(name)
                .connect_opts(address, connection_timeout)?;
            log_world_reference_system(&rec)?;
            return Ok(RerunLogger { rec });
        }
    }

    impl Logger for RerunLogger {
        fn log_transform(&self, id: &str, transform: &Affine3A) -> Result<()> {
            let (_, rotation, translation) = transform.to_scale_rotation_translation();
            let result = self.rec.log_static(
                id,
                &rerun::Transform3D::from_translation_rotation(translation, rotation),
            );
            return Ok(result?);
        }

        fn log_points(&self, id: &str, points: &[glam::Vec3]) -> Result<()> {
            let result = self.rec.log(id, &rerun::Points3D::new(points.to_vec()));
            return Ok(result?);
        }

        fn log_image(&self, id: &str, image: DynamicImage) -> Result<()> {
            let resized_image = image.resize(640, 480, image::imageops::FilterType::Nearest);
            let img = rerun::Image::from_dynamic_image(resized_image)?;
            let result = self.rec.log(id, &img);
            return Ok(result?);
        }

        fn log_camera(&self, id: &str, camera_calibration: &CameraCalib) -> Result<()> {
            let focal = camera_calibration.intrinsics.focal_length_px();
            self.rec.log_static(
                "world/camera",
                &rerun::Pinhole::from_focal_length_and_resolution(
                    [focal, focal],
                    [
                        camera_calibration.intrinsics.width,
                        camera_calibration.intrinsics.height,
                    ],
                )
                .with_camera_xyz(rerun::components::ViewCoordinates::DLB),
            )?;
            let result = self.log_transform(id, &camera_calibration.extrinsics.as_affine());
            return Ok(result?);
        }

        fn set_time_sequence(&self, id: &str, time: i64) {
            self.rec.set_time_sequence(id, time);
        }
    }

    fn make_axis() -> rerun::Arrows3D {
        let camera_axis = vec![
            AXIS_SIZE * glam::Vec3::X,
            AXIS_SIZE * glam::Vec3::Y,
            AXIS_SIZE * glam::Vec3::Z,
        ];
        let colors = vec![
            rerun::Color::from_rgb(255, 0, 0),
            rerun::Color::from_rgb(0, 255, 0),
            rerun::Color::from_rgb(0, 0, 255),
        ];
        return rerun::Arrows3D::from_vectors(camera_axis).with_colors(colors);
    }

    fn log_world_reference_system(
        rec: &rerun::RecordingStream,
    ) -> rerun::RecordingStreamResult<()> {
        let axis = make_axis();
        return rec.log_static("world/axis", &axis);
    }
}

struct NullLogger {}

impl Logger for NullLogger {
    fn log_transform(&self, _id: &str, _transform: &Affine3A) -> Result<()> {
        return Ok(());
    }

    fn log_points(&self, _id: &str, _points: &[glam::Vec3]) -> Result<()> {
        return Ok(());
    }

    fn log_image(&self, _id: &str, _image: DynamicImage) -> Result<()> {
        return Ok(());
    }

    fn log_camera(&self, _id: &str, _camera: &CameraCalib) -> Result<()> {
        return Ok(());
    }

    fn set_time_sequence(&self, _id: &str, _time: i64) {}
}
