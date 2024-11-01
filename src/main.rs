mod calibration;
mod cameras;
mod logging;
mod motor;

use calibration::{load_calibration, LaserCalib};
use motor::StepperMotor;

use std::{f32::consts::PI, time::Duration};

use libcamera::{
    camera::{ActiveCamera, CameraConfigurationStatus},
    camera_manager::CameraManager,
    framebuffer::AsFrameBuffer,
    framebuffer_allocator::{FrameBuffer, FrameBufferAllocator},
    framebuffer_map::MemoryMappedFrameBuffer,
    pixel_format::PixelFormat,
    properties,
    request::Request,
    stream::StreamRole,
};

use drm_fourcc::DrmFourcc;

// drm-fourcc does not have MJPEG type yet, construct it from raw fourcc identifier
//const MJPEG: PixelFormat = PixelFormat::new(u32::from_le_bytes([b'M', b'J', b'P', b'G']), 0);

const YUV420: PixelFormat = PixelFormat::new(DrmFourcc::Yuv420 as u32, 0);

use logging::make_logger;

use anyhow::Result;
use clap::Parser;
use image;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    image_dir: PathBuf,
    output_dir: PathBuf,
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
    let rec = make_logger("3d_scanner", reurn_server_address)?;
    rec.log_camera("world/camera", &calib.camera)?;

    println!("Processing files from {}", args.image_dir.display());

    let mut motor = StepperMotor::new()?;
    let _point_cloud = acquire_from_camera(&rec, &calib, &mut motor)?;

    Ok(())
}

fn acquire_from_camera(
    rec: &Box<dyn logging::Logger>,
    calib: &calibration::Calibration,
    motor: &mut StepperMotor,
) -> Result<Vec<glam::Vec3>> {
    let mngr = CameraManager::new()?;
    let cameras = mngr.cameras();
    let cam = cameras.get(0).expect("No cameras found");

    let camera_model = cam.properties().get::<properties::Model>()?;
    println!("Using camera: {}", *camera_model);

    let mut cam = cam.acquire()?;

    // This will generate default configuration for each specified role
    let mut cfgs = cam
        .generate_configuration(&[StreamRole::StillCapture])
        .unwrap();

    println!("Generated config: {:#?}", cfgs);

    match cfgs.validate() {
        CameraConfigurationStatus::Valid => println!("Camera configuration valid!"),
        CameraConfigurationStatus::Adjusted => {
            println!("Camera configuration was adjusted: {:#?}", cfgs)
        }
        CameraConfigurationStatus::Invalid => panic!("Error validating camera configuration"),
    }

    cam.configure(&mut cfgs)?;

    let mut alloc = FrameBufferAllocator::new(&cam);

    // Allocate frame buffers for the stream
    let mut cfg = cfgs.get_mut(0).unwrap();
    cfg.set_pixel_format(YUV420);
    let pixel_format = cfg.get_pixel_format();
    println!("Pixel format: {:?}", pixel_format);

    let frame_size = cfg.get_size();
    let stream = cfg.stream().unwrap();
    let buffers = alloc.alloc(&stream).unwrap();
    println!("Allocated {} buffers", buffers.len());

    // Convert FrameBuffer to MemoryMappedFrameBuffer, which allows reading &[u8]
    let buffers = buffers
        .into_iter()
        .map(|buf| MemoryMappedFrameBuffer::new(buf).unwrap())
        .collect::<Vec<_>>();

    // Create capture requests and attach buffers
    let mut reqs = buffers
        .into_iter()
        .map(|buf| {
            let mut req = cam.create_request(None).unwrap();
            req.add_buffer(&stream, buf).unwrap();
            req
        })
        .collect::<Vec<_>>();

    // Completed capture requests are returned as a callback
    let (tx, rx) = std::sync::mpsc::channel();
    cam.on_request_completed(move |req| {
        tx.send(req).unwrap();
    });

    cam.start(None).unwrap();

    let mut point_cloud = Vec::<glam::Vec3>::new();
    let angle_per_step = 5_f32.to_radians();
    let steps = (2_f32 * PI / angle_per_step).ceil() as i32;
    for i in 0..steps {
        let image = get_image(&cam, &stream, &frame_size, &mut reqs, &rx).unwrap();
        acquisition_step(
            &image,
            i as i64,
            &rec,
            angle_per_step,
            &calib,
            motor,
            &mut point_cloud,
        )?;
    }

    Ok(point_cloud)
}

fn acquisition_step(
    image: &image::GrayImage,
    i: i64,
    rec: &Box<dyn logging::Logger>,
    angle_per_step: f32,
    calib: &calibration::Calibration,
    motor: &mut StepperMotor,
    point_cloud: &mut Vec<glam::Vec3>,
) -> Result<()> {
    rec.set_time_sequence("timeline", i as i64);
    motor.step(1);
    let res = rec.log_image(
        "world/image",
        image::DynamicImage::ImageLuma8(image.clone()),
    );
    if let Err(e) = res {
        println!("Failed to log image to logger: {e}");
    }

    let transform = glam::Affine3A::from_rotation_z(angle_per_step);
    for point in &mut *point_cloud {
        *point = transform.transform_point3(*point);
    }

    let mut new_points = triangulate(&image, &calib);
    rec.log_points("world/points_3d_cam", &new_points)?;
    point_cloud.append(&mut new_points);
    rec.log_points("world/points_3d_world", &point_cloud)?;
    Ok(())
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

fn get_image(
    camera: &ActiveCamera,
    stream: &libcamera::stream::Stream,
    frame_size: &libcamera::geometry::Size,
    requests: &mut Vec<Request>,
    rx: &std::sync::mpsc::Receiver<Request>,
) -> Option<image::GrayImage> {
    camera.queue_request(requests.pop().unwrap()).unwrap();

    println!("Waiting for camera request execution");
    let req = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("Camera request failed");
    println!("Camera request {:?} completed!", req);
    println!("Metadata: {:#?}", req.metadata());
    // Get framebuffer for our stream
    let framebuffer: &MemoryMappedFrameBuffer<FrameBuffer> = req.buffer(&stream).unwrap();
    println!("FrameBuffer metadata: {:#?}", framebuffer.metadata());

    // grayscale image encoded in first image plane
    let planes = framebuffer.data();
    let image_data = planes.get(0).unwrap();
    let data_length = framebuffer
        .metadata()
        .unwrap()
        .planes()
        .get(0)
        .unwrap()
        .bytes_used as usize;

    // copy buffer data to Vec<u8>
    let buffer_data = image_data[..data_length].to_vec();

    // recycle request
    requests.push(req);

    let image = image::GrayImage::from_raw(frame_size.width, frame_size.height, buffer_data)
        .expect("Failed to create image from raw buffer");
    return Some(image);
}
