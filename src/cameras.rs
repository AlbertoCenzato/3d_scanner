use crate::calibration;
use crate::img_processing;
use crate::logging;
use crate::motor;
use anyhow::Result;
use std::cfg;
use std::path::{Path, PathBuf};
use std::{f32::consts::PI, time::Duration};
use std::{io, vec::IntoIter};

pub enum CameraType {
    DiskLoader(std::path::PathBuf),
    #[cfg(feature = "camera")]
    RaspberryPi,
}

pub trait Camera {
    fn acquire_from_camera(
        &self,
        rec: &dyn logging::Logger,
        calib: &calibration::Calibration,
        motor: &mut dyn motor::StepperMotor,
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
            let camera: Box<dyn Camera> = Box::new(real_camera::PiCamera {});
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

    fn get_image(&mut self) -> Option<image::GrayImage> {
        match self.iter.next() {
            Some(path) => Some(image::open(path).unwrap().into_luma8()),
            None => None,
        }
    }
}

impl Camera for DiskCamera {
    fn acquire_from_camera(
        &self,
        rec: &dyn logging::Logger,
        calib: &calibration::Calibration,
        motor: &mut dyn motor::StepperMotor,
    ) -> Result<Vec<glam::Vec3>> {
        let mut camera = DiskCamera::from_directory(Path::new("images"))?;

        let mut point_cloud = Vec::<glam::Vec3>::new();
        let angle_per_step = 5_f32.to_radians();
        let steps = (2_f32 * PI / angle_per_step).ceil() as i32;
        for i in 0..steps {
            let image = camera.get_image().unwrap();
            img_processing::process_image(
                &image,
                i as i64,
                rec,
                angle_per_step,
                &calib,
                motor,
                &mut point_cloud,
            )?;
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
        framebuffer_map::MemoryMappedFrameBuffer,
        pixel_format::PixelFormat,
        properties,
        request::Request,
        stream::StreamRole,
    };

    // drm-fourcc does not have MJPEG type yet, construct it from raw fourcc identifier
    //const MJPEG: PixelFormat = PixelFormat::new(u32::from_le_bytes([b'M', b'J', b'P', b'G']), 0);

    const YUV420: PixelFormat = PixelFormat::new(DrmFourcc::Yuv420 as u32, 0);

    pub struct PiCamera {}

    impl Camera for PiCamera {
        fn acquire_from_camera(
            &self,
            rec: &dyn logging::Logger,
            calib: &calibration::Calibration,
            motor: &mut dyn motor::StepperMotor,
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
                CameraConfigurationStatus::Invalid => {
                    panic!("Error validating camera configuration")
                }
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
                img_processing::process_image(
                    &image,
                    i as i64,
                    rec,
                    angle_per_step,
                    &calib,
                    motor,
                    &mut point_cloud,
                )?;
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
}
