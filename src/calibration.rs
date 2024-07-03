use anyhow::Result;
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

pub fn load_calibration(path: &std::path::Path) -> Result<Calibration> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let json: Calibration = serde_json::from_reader(reader)?;
    return Ok(json);
}
