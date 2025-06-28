use std::io::Read;

use anyhow::Result;
use image::buffer;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Serialize, Deserialize)]
pub struct LaserCalib {
    // TODO(alberto): generalize to 3D
    pub angle: f32,
    pub baseline: f32,
}

impl LaserCalib {
    pub fn angle_rad(&self) -> f32 {
        return self.angle.to_radians();
    }
}

#[derive(Serialize, Deserialize)]
pub struct RefSysTransform {
    rotation: glam::Vec3,
    translation: glam::Vec3,
}

impl RefSysTransform {
    pub fn as_affine(&self) -> glam::Affine3A {
        let rot = glam::Quat::from_euler(
            glam::EulerRot::XYZ,
            self.rotation.x.to_radians(),
            self.rotation.y.to_radians(),
            self.rotation.z.to_radians(),
        );
        return glam::Affine3A::from_rotation_translation(rot, self.translation);
    }
}

#[derive(Serialize, Deserialize)]
pub struct CameraIntrinsics {
    pub focal_length: f32,
    pub height: f32,
    pub width: f32,
    pub meters_per_px: f32,
}

impl CameraIntrinsics {
    pub fn focal_length_px(&self) -> f32 {
        return self.focal_length / self.meters_per_px;
    }
}

#[derive(Serialize, Deserialize)]
pub struct CameraCalib {
    pub intrinsics: CameraIntrinsics,
    pub extrinsics: RefSysTransform,
    cam_2_img_plane_rotation: glam::Vec3,
}

impl CameraCalib {
    pub fn img_plane_2_cam(&self) -> glam::Affine3A {
        let t = glam::vec3(0_f32, 0_f32, -self.intrinsics.focal_length);
        let rx = self.cam_2_img_plane_rotation.x.to_radians();
        let ry = self.cam_2_img_plane_rotation.y.to_radians();
        let rz = self.cam_2_img_plane_rotation.z.to_radians();
        let rot = glam::Quat::from_euler(glam::EulerRot::XYZ, rx, ry, rz);
        return glam::Affine3A::from_rotation_translation(rot, t).inverse();
    }
}

#[derive(Serialize, Deserialize)]
pub struct Calibration {
    pub camera: CameraCalib,
    pub left_laser: LaserCalib,
    pub right_laser: LaserCalib,
}

fn decorate_with_path(e: std::io::Error, path: &std::path::Path) -> std::io::Error {
    let p = path.display();
    return std::io::Error::new(e.kind(), format!("{p}: {e}"));
}

pub fn load_calibration(path: &std::path::Path) -> Result<Calibration> {
    let file = std::fs::File::open(path).map_err(|e| decorate_with_path(e, path))?;
    let mut reader = std::io::BufReader::new(file);

    let mut buffer = String::new();
    reader
        .read_to_string(&mut buffer)
        .map_err(|e| decorate_with_path(e, path))?;

    let calibration = serde_json::from_str(&buffer)?;
    return Ok(calibration);
}
