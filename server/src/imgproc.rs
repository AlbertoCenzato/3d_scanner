use crate::calibration;
use crate::calibration::LaserCalib;
use crate::logging;
use crate::motor::StepperMotor;
use anyhow::Result;

const LOW_THRESHOLD: u8 = 30;

pub fn process_image(
    image: &image::GrayImage,
    i: i64,
    rec: &dyn logging::Logger,
    angle_per_step: f32,
    calib: &calibration::Calibration,
    motor: &mut dyn StepperMotor,
) -> Vec<glam::Vec3> {
    rec.set_time_sequence("timeline", i as i64);
    motor.step(1);
    let res = rec.log_image(
        "world/image",
        image::DynamicImage::ImageLuma8(image.clone()),
    );
    if let Err(e) = res {
        println!("Failed to log image to logger: {e}");
    }

    //let transform = glam::Affine3A::from_rotation_z(angle_per_step);
    //for point in &mut *point_cloud {
    //    *point = transform.transform_point3(*point);
    //}

    let new_points = triangulate(&image, &calib);
    //rec.log_points("world/points_3d_cam", &new_points)?;
    //point_cloud.append(&mut new_points);
    //rec.log_points("world/points_3d_world", &point_cloud)?;
    //Ok(())
    return new_points;
}

fn triangulate(image: &image::GrayImage, calib: &calibration::Calibration) -> Vec<glam::Vec3> {
    println!("Image info: dimensions {:?}", image.dimensions(),);

    let width = image.width() as f32;
    let height = image.height() as f32;
    let img_2_img_center =
        glam::Affine3A::from_translation(-glam::vec3(width / 2_f32, height / 2_f32, 0_f32));

    let focal_length_px = calib.camera.intrinsics.focal_length_px();
    let points: Vec<glam::Vec3> = detect_laser_points(&image)
        .iter()
        .map(|p| glam::vec3(p.x, p.y, focal_length_px))
        .map(|p| img_2_img_center.transform_point3(p))
        .collect();

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

    let img_plane_2_world = calib.camera.extrinsics.as_affine() * calib.camera.img_plane_2_cam();
    return points
        .iter()
        .map(|p| meters_per_px * (*p))
        .map(|p| img_plane_2_world.transform_point3(p))
        .collect();
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
