use std::io::Read;

use anyhow::Result;
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json;

pub struct LaserCalib {
    // TODO(alberto): generalize to 3D
    pub angle: f32,
    pub baseline: f32,
}

impl<'de> Deserialize<'de> for LaserCalib {
    fn deserialize<D>(deserializer: D) -> Result<LaserCalib, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = RefSysTransform::deserialize(deserializer)?;
        if helper.rotation_euler_deg[1] != 0_f32 || helper.rotation_euler_deg[2] != 0_f32 {
            return Err(de::Error::custom(
                "Laser supports only X-axis rotation for now",
            ));
        }
        if helper.translation_m[0] != 0_f32 || helper.translation_m[2] != 0_f32 {
            return Err(de::Error::custom(
                "Laser supports only Y-axis translation for now",
            ));
        }
        return Ok(LaserCalib {
            angle: helper.rotation_euler_deg[0],
            baseline: helper.translation_m[1],
        });
    }
}

impl LaserCalib {
    pub fn angle_rad(&self) -> f32 {
        return self.angle.to_radians();
    }
}

#[derive(Serialize, Deserialize)]
pub struct RefSysTransform {
    rotation_euler_deg: glam::Vec3,
    translation_m: glam::Vec3,
}

impl RefSysTransform {
    pub fn as_affine(&self) -> glam::Affine3A {
        let rot = glam::Quat::from_euler(
            glam::EulerRot::XYZ,
            self.rotation_euler_deg.x.to_radians(),
            self.rotation_euler_deg.y.to_radians(),
            self.rotation_euler_deg.z.to_radians(),
        );
        return glam::Affine3A::from_rotation_translation(rot, self.translation_m);
    }
}

#[derive(Serialize, Deserialize)]
pub struct CameraIntrinsics {
    pub focal_length_m: f32,
    pub height_px: f32,
    pub width_px: f32,
    pub meters_per_px: f32,
}

impl CameraIntrinsics {
    pub fn focal_length_px(&self) -> f32 {
        return self.focal_length_m / self.meters_per_px;
    }
}

#[derive(Serialize, Deserialize)]
pub struct CameraCalib {
    pub intrinsics: CameraIntrinsics,
    pub extrinsics: RefSysTransform,
    cam_2_img_plane_rotation_deg: glam::Vec3,
}

impl CameraCalib {
    pub fn img_plane_2_cam(&self) -> glam::Affine3A {
        let t = glam::vec3(0_f32, 0_f32, -self.intrinsics.focal_length_m);
        let rx = self.cam_2_img_plane_rotation_deg.x.to_radians();
        let ry = self.cam_2_img_plane_rotation_deg.y.to_radians();
        let rz = self.cam_2_img_plane_rotation_deg.z.to_radians();
        let rot = glam::Quat::from_euler(glam::EulerRot::XYZ, rx, ry, rz);
        return glam::Affine3A::from_rotation_translation(rot, t).inverse();
    }
}

#[derive(Deserialize)]
pub struct Calibration {
    pub camera: CameraCalib,
    pub laser_left: LaserCalib,
    pub laser_right: LaserCalib,
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
