mod cameras;

use cameras::get_camera;
use cameras::CameraType;
use clap::Parser;
use image;
use image::DynamicImage;
use rerun;
use rerun::external::glam;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    image_dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct LaserCalib {
    angle: f32,
    baseline: f32,
}

#[derive(Serialize, Deserialize)]
struct CameraIntrinsics {
    focal_length: f32,
    height: f32,
    width: f32,
    meters_per_px: f32,
}

const AXIS_SIZE: f32 = 0.01_f32;
const LOW_THRESHOLD: u8 = 30;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let right_laser_calib = LaserCalib {
        angle: 20_f32.to_radians(),
        baseline: 0.2,
    };
    let left_laser_calib = LaserCalib {
        angle: -20_f32.to_radians(),
        baseline: -0.2,
    };

    let camera_intrinsics = CameraIntrinsics {
        focal_length: 0.00474,
        height: 1280.0,
        width: 720.0,
        meters_per_px: 0.000005039,
    };

    let cam_2_world = glam::Affine3A::from_translation(glam::vec3(0_f32, 0.15_f32, 0.4_f32))
        * glam::Affine3A::from_rotation_y(90_f32.to_radians())
        * glam::Affine3A::from_rotation_x(90_f32.to_radians());
    let world_2_cam = cam_2_world.inverse();

    let reurn_server_address = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
        9876,
    );
    let rec = rerun::RecordingStreamBuilder::new("monkey_head")
        .connect_opts(reurn_server_address, rerun::default_flush_timeout())?;
    log_world_reference_system(&rec)?;

    let args = Args::parse();
    let mut camera = DiskLoaderCamera::from_directory(&args.image_dir)?;

    println!("Processing files from {}", args.image_dir.display());

    let camera_type = CameraType::DiskLoader(args.image_dir);
    let mut camera = get_camera(camera_type)?;

    let focal_length_px = camera_intrinsics.focal_length / camera_intrinsics.meters_per_px;
    rec.log_static(
        "world/camera",
        &rerun::Pinhole::from_focal_length_and_resolution(
            [focal_length_px, focal_length_px],
            [camera_intrinsics.width, camera_intrinsics.height],
        ),
    )?;
    let (_, rotation, translation) = world_2_cam.to_scale_rotation_translation();
    rec.log_static(
        "world/camera",
        &rerun::Transform3D::from_translation_rotation(translation, rotation),
    )?;

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

        let width = luma_img.width();
        let height = luma_img.height();
        let img_2_cam2 = glam::Affine2::from_translation(-glam::Vec2::new(
            (width as f32) / 2_f32,
            (height as f32) / 2_f32,
        ));

        let points: Vec<glam::Vec3> = detect_laser_points(&luma_img)
            .iter()
            .map(|p| img_2_cam2.transform_point2(*p))
            .map(|p| glam::vec3(p.x, p.y, focal_length_px))
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

        let mut right_projected_points: Vec<glam::Vec3> = right_laser_points
            .iter()
            .map(|p| {
                project_on_laser_plane(*p, &right_laser_calib, camera_intrinsics.meters_per_px)
            })
            .collect();
        let mut left_projected_points: Vec<glam::Vec3> = left_laser_points
            .iter()
            .map(|p| project_on_laser_plane(*p, &left_laser_calib, camera_intrinsics.meters_per_px))
            .collect();

        let mut points = Vec::<glam::Vec3>::new();
        points.append(&mut right_projected_points);
        points.append(&mut left_projected_points);

        let mut points_3d_world: Vec<glam::Vec3> = points
            .iter()
            .map(|p| camera_intrinsics.meters_per_px * (*p))
            .map(|p| world_2_cam.transform_point3(p))
            .collect();

        rec.log(
            "world/points_3d_cam",
            &rerun::Points3D::new(points_3d_world.clone()),
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

fn log_world_reference_system(rec: &rerun::RecordingStream) -> rerun::RecordingStreamResult<()> {
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
    return rec.log(
        "world/axis",
        &rerun::Arrows3D::from_vectors(camera_axis.clone()).with_colors(colors.clone()),
    );
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
    let denominator = p.z * laser_calib.angle.tan() + p.x;
    p * (laser_baseline_px / denominator)
}
