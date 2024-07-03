mod cameras;

use anyhow::Result;
use cameras::get_camera;
use cameras::CameraType;
use clap::Parser;
use image;
use image::DynamicImage;
use rerun;
use rerun::external::glam;
use serde::{Deserialize, Serialize};
use serde_json;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    image_dir: PathBuf,
    #[clap(default_value = "calibration.json")]
    calibration: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct LaserCalib {
    // TODO(alberto): generalize to 3D
    angle: f32,
    baseline: f32,
}

impl LaserCalib {
    fn angle_rad(&self) -> f32 {
        return self.angle.to_radians();
    }
}

#[derive(Serialize, Deserialize)]
struct RefSysTransform {
    rotation: glam::Vec3,
    translation: glam::Vec3,
}

impl RefSysTransform {
    fn as_affine(&self) -> glam::Affine3A {
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
struct CameraIntrinsics {
    focal_length: f32,
    height: f32,
    width: f32,
    meters_per_px: f32,
}

impl CameraIntrinsics {
    fn focal_length_px(&self) -> f32 {
        return self.focal_length / self.meters_per_px;
    }
}

#[derive(Serialize, Deserialize)]
struct CameraCalib {
    intrinsics: CameraIntrinsics,
    extrinsics: RefSysTransform,
    cam_2_img_plane_rotation: glam::Vec3,
}

impl CameraCalib {
    fn img_plane_2_cam(&self) -> glam::Affine3A {
        let t = glam::vec3(0_f32, 0_f32, -self.intrinsics.focal_length);
        let rx = self.cam_2_img_plane_rotation.x.to_radians();
        let ry = self.cam_2_img_plane_rotation.y.to_radians();
        let rz = self.cam_2_img_plane_rotation.z.to_radians();
        let rot = glam::Quat::from_euler(glam::EulerRot::XYZ, rx, ry, rz);
        return glam::Affine3A::from_rotation_translation(rot, t).inverse();
    }
}

#[derive(Serialize, Deserialize)]
struct Calibration {
    camera: CameraCalib,
    left_laser: LaserCalib,
    right_laser: LaserCalib,
}

const AXIS_SIZE: f32 = 0.1_f32;
const LOW_THRESHOLD: u8 = 30;

fn main() -> Result<()> {
    let args = Args::parse();
    let calib = load_calibration(&args.calibration)?;

    let reurn_server_address = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
        9876,
    );
    let rec = rerun::RecordingStreamBuilder::new("3d_scanner")
        .connect_opts(reurn_server_address, rerun::default_flush_timeout())?;
    log_world_entities(&rec, &calib.camera)?;

    println!("Processing files from {}", args.image_dir.display());

    let camera_type = CameraType::DiskLoader(args.image_dir);
    let mut camera = get_camera(camera_type)?;

    let img_plane_2_world = calib.camera.extrinsics.as_affine() * calib.camera.img_plane_2_cam();

    let mut point_cloud = Vec::<glam::Vec3>::new();
    let mut i = 0;
    let mut image = camera.get_image();
    while let Some(luma_img) = image {
        rec.set_time_sequence("timeline", i as i64);

        let angle = (1.0 as f32).to_radians();
        let transform = glam::Affine3A::from_rotation_z(angle);
        for point in &mut point_cloud {
            *point = transform.transform_point3(*point);
        }

        println!("Image info: dimensions {:?}", luma_img.dimensions(),);

        let width = luma_img.width() as f32;
        let height = luma_img.height() as f32;
        let img_2_img_center =
            glam::Affine3A::from_translation(-glam::vec3(width / 2_f32, height / 2_f32, 0_f32));

        let focal_length_px = calib.camera.intrinsics.focal_length_px();
        let points: Vec<glam::Vec3> = detect_laser_points(&luma_img)
            .iter()
            .map(|p| glam::vec3(p.x, p.y, focal_length_px))
            .map(|p| img_2_img_center.transform_point3(p))
            .collect();

        let img = DynamicImage::ImageLuma8(luma_img);
        let tensor = rerun::TensorData::from_dynamic_image(img)?;
        rec.log("world/camera/image", &rerun::Image::new(tensor))?;

        let mut left_laser_points = Vec::<glam::Vec3>::new();
        let mut right_laser_points = Vec::<glam::Vec3>::new();
        for point in points {
            if point.x >= 0_f32 {
                right_laser_points.push(point);
            } else {
                left_laser_points.push(point);
            }
        }

        let meters_per_px = calib.camera.intrinsics.meters_per_px;
        let mut right_projected_points: Vec<glam::Vec3> = right_laser_points
            .iter()
            .map(|p| project_on_laser_plane(*p, &calib.right_laser, meters_per_px))
            .collect();
        let mut left_projected_points: Vec<glam::Vec3> = left_laser_points
            .iter()
            .map(|p| project_on_laser_plane(*p, &calib.left_laser, meters_per_px))
            .collect();

        let mut points = Vec::<glam::Vec3>::new();
        points.append(&mut right_projected_points);
        points.append(&mut left_projected_points);

        let mut points_3d_world: Vec<glam::Vec3> = points
            .iter()
            .map(|p| meters_per_px * (*p))
            .map(|p| img_plane_2_world.transform_point3(p))
            .collect();

        rec.log(
            "world/points_3d_cam",
            &rerun::Points3D::new(points_3d_world.clone()).with_colors([0, 0, 255]),
        )?;

        rec.log(
            "world/points_3d_world",
            &rerun::Points3D::new(point_cloud.clone()),
        )?;

        point_cloud.append(&mut points_3d_world);
        i = i + 1;
        image = camera.get_image();
    }

    Ok(())
}

fn load_calibration(path: &std::path::Path) -> Result<Calibration> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let json: Calibration = serde_json::from_reader(reader)?;
    return Ok(json);
}

fn log_world_entities(
    rec: &rerun::RecordingStream,
    camera_calib: &CameraCalib,
) -> rerun::RecordingStreamResult<()> {
    log_world_reference_system(rec)?;
    log_camera_pose(rec, camera_calib)?;
    Ok(())
}

fn log_camera_pose(
    rec: &rerun::RecordingStream,
    camera_calib: &CameraCalib,
) -> rerun::RecordingStreamResult<()> {
    let focal = camera_calib.intrinsics.focal_length_px();
    rec.log_static(
        "world/camera",
        &rerun::Pinhole::from_focal_length_and_resolution(
            [focal, focal],
            [
                camera_calib.intrinsics.width,
                camera_calib.intrinsics.height,
            ],
        )
        .with_camera_xyz(rerun::components::ViewCoordinates::DLB),
    )?;

    let extrinsics = camera_calib.extrinsics.as_affine();
    let (_, rotation, translation) = extrinsics.to_scale_rotation_translation();
    rec.log_static(
        "world/camera",
        &rerun::Transform3D::from_translation_rotation(translation, rotation),
    )?;
    Ok(())
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

fn log_world_reference_system(rec: &rerun::RecordingStream) -> rerun::RecordingStreamResult<()> {
    let axis = make_axis();
    return rec.log_static("world/axis", &axis);
}

fn detect_laser_points(image: &image::GrayImage) -> Vec<glam::Vec2> {
    let mut points = Vec::<glam::Vec2>::new();
    for (_, row) in image.enumerate_rows() {
        let mut laser_start: u32 = 0;
        let mut laser_end: u32 = 0;
        for (x, y, pixel) in row {
            if pixel.0[0] > LOW_THRESHOLD {
                if laser_start == 0 {
                    laser_start = x;
                }
                laser_end = x;
            } else {
                if laser_start != 0 {
                    let x = ((laser_start + laser_end) as f32) / 2_f32;
                    points.push(glam::Vec2::new(x, y as f32));
                    laser_start = 0;
                    laser_end = 0;
                }
            }
        }
    }
    return points;
}

fn project_on_laser_plane(
    p: glam::Vec3,
    laser_calib: &LaserCalib,
    meters_per_px: f32,
) -> glam::Vec3 {
    let laser_baseline_px = laser_calib.baseline / meters_per_px;
    let denominator = p.z * laser_calib.angle_rad().tan() + p.x;
    p * (laser_baseline_px / denominator)
}
