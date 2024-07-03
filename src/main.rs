mod calibration;
mod cameras;
mod logging;

use calibration::{load_calibration, CameraCalib, LaserCalib};
use cameras::get_camera;
use cameras::CameraType;

use logging::{Logger, RerunLogger};

use anyhow::Result;
use clap::Parser;
use image;
use image::DynamicImage;
use rerun;
use rerun::external::glam;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    image_dir: PathBuf,
    #[clap(default_value = "calibration.json")]
    calibration: PathBuf,
    #[clap(default_value = "127.0.0.1")]
    rerun_ip: std::net::Ipv4Addr,
    #[clap(default_value = "9876")]
    rerun_port: u16,
}

const LOW_THRESHOLD: u8 = 30;

fn main() -> Result<()> {
    let args = Args::parse();
    let calib = load_calibration(&args.calibration)?;

    let reurn_server_address =
        std::net::SocketAddr::new(std::net::IpAddr::V4(args.rerun_ip), args.rerun_port);
    let rec = RerunLogger::new("3d_scanner", reurn_server_address)?;
    rec.log_camera("world/camera", &calib.camera)?;

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
        rec.log_image("world/camera/image", img)?;

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

        rec.log_points("world/points_3d_cam", &points_3d_world)?;
        rec.log_points("world/points_3d_world", &point_cloud)?;

        point_cloud.append(&mut points_3d_world);
        i = i + 1;
        image = camera.get_image();
    }

    Ok(())
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
