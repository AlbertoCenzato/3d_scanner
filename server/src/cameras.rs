use crate::calibration;
use crate::imgproc;
use crate::logging;
use crate::motor;
use anyhow::Result;
use log::{error, info, warn};
use msg::response;
use msg::response::PointCloud;
use msg::response::Response;
use std::f32::consts::PI;
use std::path::{Path, PathBuf};
use std::{io, sync::mpsc, vec::IntoIter};

pub enum CameraType {
    DiskLoader(std::path::PathBuf),
    #[cfg(feature = "camera")]
    RaspberryPi,
}

#[derive(Debug)]
pub enum CameraError {
    CameraNotFound,
    WrongCameraConfig,
    InvalidRequest,
}

impl std::error::Error for CameraError {}

impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CameraError::CameraNotFound => write!(f, "Camera not found"),
            CameraError::WrongCameraConfig => write!(f, "Wrong camera configuration"),
            CameraError::InvalidRequest => write!(f, "Invalid request"),
        }
    }
}

pub trait Camera {
    fn acquire_from_camera(
        &mut self,
        rec: &dyn logging::Logger,
        calib: &calibration::Calibration,
        motor: &mut dyn motor::StepperMotor,
        scanned_data_queue: mpsc::Sender<Response>,
    ) -> Result<Vec<glam::Vec3>>;
}

pub fn make_camera(camera_type: CameraType) -> Result<Box<dyn Camera>> {
    match camera_type {
        CameraType::DiskLoader(path) => {
            let camera: Box<dyn Camera> = Box::new(DiskCamera::from_directory(&path)?);
            return Ok(camera);
        }
        #[cfg(feature = "camera")]
        CameraType::RaspberryPi => {
            let camera: Box<dyn Camera> = Box::new(real_camera::PiCamera { num_buffers: 5 });
            return Ok(camera);
        }
    }
}

pub struct DiskCamera {
    iter: IntoIter<PathBuf>,
}

impl DiskCamera {
    fn from_directory(path: &Path) -> Result<DiskCamera, io::Error> {
        let images: Vec<PathBuf> = path
            .read_dir()?
            .filter_map(|f| match f {
                Ok(entry) => Some(entry.path()),
                Err(_) => None,
            })
            .collect();
        Ok(DiskCamera {
            iter: images.into_iter(),
        })
    }

    fn get_image(&mut self) -> Result<image::GrayImage> {
        match self.iter.next() {
            Some(path) => Ok(image::open(path).unwrap().into_luma8()),
            None => Err(io::Error::new(io::ErrorKind::NotFound, "No more images").into()),
        }
    }
}

impl Camera for DiskCamera {
    fn acquire_from_camera(
        &mut self,
        rec: &dyn logging::Logger,
        calib: &calibration::Calibration,
        motor: &mut dyn motor::StepperMotor,
        scanned_data_queue: mpsc::Sender<Response>,
    ) -> Result<Vec<glam::Vec3>> {
        let mut point_cloud = Vec::<glam::Vec3>::new();
        let angle_per_step = 5_f32.to_radians();
        let steps = (2_f32 * PI / angle_per_step).ceil() as i32;
        for i in 0..steps {
            let image = self.get_image()?;
            let new_points =
                imgproc::process_image(&image, i as i64, rec, angle_per_step, &calib, motor);

            let response = PointCloud { points: new_points };
            scanned_data_queue.send(Response::PointCloud(response))?;
        }

        Ok(point_cloud)
    }
}

#[cfg(feature = "camera")]
pub mod real_camera {
    use super::*;
    use drm_fourcc::DrmFourcc;
    use libcamera::{
        camera::{ActiveCamera, CameraConfigurationStatus},
        camera_manager::CameraManager,
        framebuffer::AsFrameBuffer,
        framebuffer_allocator::{FrameBuffer, FrameBufferAllocator},
        framebuffer_map::{MemoryMappedFrameBuffer, MemoryMappedFrameBufferError},
        geometry::Size,
        pixel_format::PixelFormat,
        properties,
        request::{Request, ReuseFlag},
        stream::StreamRole,
    };
    use msg::response::PointCloud;
    use std::time::Duration;

    // drm-fourcc does not have MJPEG type yet, construct it from raw fourcc identifier
    //const MJPEG: PixelFormat = PixelFormat::new(u32::from_le_bytes([b'M', b'J', b'P', b'G']), 0);

    const YUV420: PixelFormat = PixelFormat::new(DrmFourcc::Yuv420 as u32, 0);

    pub struct PiCamera {
        pub num_buffers: u32,
    }

    impl Camera for PiCamera {
        fn acquire_from_camera(
            &mut self,
            rec: &dyn logging::Logger,
            calib: &calibration::Calibration,
            motor: &mut dyn motor::StepperMotor,
            scanned_data_queue: mpsc::Sender<Response>,
        ) -> Result<Vec<glam::Vec3>> {
            let mngr = CameraManager::new()?;
            let cameras = mngr.cameras();
            let cam = cameras.get(0).ok_or(CameraError::CameraNotFound)?;

            let camera_model = cam.properties().get::<properties::Model>()?;
            info!("Using camera: {}", *camera_model);

            let mut cam = cam.acquire()?;

            // This will generate default configuration for each specified role
            let mut cfgs = cam
                .generate_configuration(&[StreamRole::StillCapture])
                .ok_or(CameraError::WrongCameraConfig)?;

            info!("Generated config: {:#?}", cfgs);

            match cfgs.validate() {
                CameraConfigurationStatus::Valid => info!("Camera configuration valid!"),
                CameraConfigurationStatus::Adjusted => {
                    info!("Camera configuration was adjusted: {:#?}", cfgs)
                }
                CameraConfigurationStatus::Invalid => {
                    return Err(CameraError::WrongCameraConfig.into());
                }
            }

            cam.configure(&mut cfgs)?;

            let mut alloc = FrameBufferAllocator::new(&cam);

            // Allocate frame buffers for the stream
            let mut cfg = cfgs.get_mut(0).ok_or(CameraError::WrongCameraConfig)?;
            cfg.set_pixel_format(YUV420);
            //cfg.set_size(Size {
            //    width: 640,
            //    height: 480,
            //});
            cfg.set_buffer_count(self.num_buffers);
            let pixel_format = cfg.get_pixel_format();
            info!("Pixel format: {:?}", pixel_format);

            let frame_size = cfg.get_size();
            let stream = cfg.stream().ok_or(CameraError::WrongCameraConfig)?;
            let buffers = alloc.alloc(&stream)?;
            info!("Allocated {} buffers", buffers.len());

            // Convert FrameBuffer to MemoryMappedFrameBuffer, which allows reading &[u8]
            let buffers = buffers
                .into_iter()
                .map(|buf| MemoryMappedFrameBuffer::new(buf))
                .collect::<Result<Vec<_>, _>>()?;

            // Create capture requests and attach buffers
            let mut reqs = buffers
                .into_iter()
                .map(|buf| {
                    let mut req = cam
                        .create_request(None)
                        .ok_or(CameraError::InvalidRequest)?;
                    req.add_buffer(&stream, buf)?;
                    Ok::<_, anyhow::Error>(req)
                })
                .collect::<Result<Vec<_>, _>>()?;

            // Completed capture requests are returned as a callback
            let (tx, rx) = std::sync::mpsc::channel();
            cam.on_request_completed(move |req| {
                tx.send(req).unwrap();
            });

            cam.start(None)?;

            let mut point_cloud = Vec::<glam::Vec3>::new();
            let angle_per_step = 5_f32.to_radians();
            let steps = (2_f32 * PI / angle_per_step).ceil() as i32;
            for i in 0..steps {
                info!("Acquiring image {}", i);
                let image = get_image(&cam, &stream, &frame_size, &mut reqs, &rx)?;
                info!("Processing image {}", i);
                imgproc::process_image(
                    &image,
                    i as i64,
                    rec,
                    angle_per_step,
                    &calib,
                    motor,
                    &mut point_cloud,
                )?;

                let fake_data = i as f32;
                let response = PointCloud {
                    points: vec![glam::Vec3::new(fake_data, fake_data, fake_data)],
                };
                scanned_data_queue.send(Response::PointCloud(response))?;

                motor.step(1);
                std::thread::sleep(Duration::from_millis(100));
            }

            Ok(point_cloud)
        }
    }

    fn get_image(
        camera: &ActiveCamera,
        stream: &libcamera::stream::Stream,
        frame_size: &libcamera::geometry::Size,
        requests: &mut Vec<Request>,
        rx: &std::sync::mpsc::Receiver<Request>,
    ) -> Result<image::GrayImage> {
        let req = requests.pop().ok_or(CameraError::InvalidRequest)?;
        camera.queue_request(req).unwrap();

        info!("Waiting for camera request execution");
        let mut req = rx.recv_timeout(Duration::from_secs(2))?;
        info!("Camera request {:?} completed!", req);
        info!("Metadata: {:#?}", req.metadata());
        // Get framebuffer for our stream
        let framebuffer: &MemoryMappedFrameBuffer<FrameBuffer> =
            req.buffer(&stream).ok_or(CameraError::InvalidRequest)?;
        info!("FrameBuffer metadata: {:#?}", framebuffer.metadata());

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
        req.reuse(ReuseFlag::REUSE_BUFFERS);
        requests.push(req);

        let image = image::GrayImage::from_raw(frame_size.width, frame_size.height, buffer_data)
            .ok_or(CameraError::InvalidRequest)?;
        return Ok(image);
    }
}
