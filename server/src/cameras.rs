use crate::acquisition_loop::{AcquisitionLoop, Camera, OpenCamera};
use crate::calibration;
use anyhow::Result;
use log::info;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc};
use std::{io, vec::IntoIter};

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

pub fn make_camera(camera_type: CameraType) -> Result<Box<dyn Camera + Send>> {
    match camera_type {
        CameraType::DiskLoader(path) => {
            let camera: Box<dyn Camera + Send> = Box::new(DiskCamera::from_directory(&path)?);
            return Ok(camera);
        }
        #[cfg(feature = "camera")]
        CameraType::RaspberryPi => {
            let camera: Box<dyn Camera + Send> = Box::new(real_camera::PiCamera { num_buffers: 5 });
            return Ok(camera);
        }
    }
}

fn is_img_path(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        return ext == "png" || ext == "jpg" || ext == "jpeg" || ext == "bmp";
    }
    return false;
}

pub struct DiskCamera {
    images_paths: IntoIter<PathBuf>,
    calibration: calibration::Calibration,
}

impl DiskCamera {
    fn from_directory(path: &Path) -> Result<DiskCamera, io::Error> {
        let images: Vec<PathBuf> = path
            .read_dir()?
            .filter_map(|f| match f {
                Ok(entry) => Some(entry.path()),
                Err(_) => None,
            })
            .filter(|p| is_img_path(p))
            .collect();
        let calibration = calibration::load_calibration(&path.join("calibration.json"))
            .map_err(|e| io::Error::new(io::ErrorKind::NotFound, format!("{e}")))?;
        Ok(DiskCamera {
            images_paths: images.into_iter(),
            calibration,
        })
    }
}

impl OpenCamera for DiskCamera {
    fn get_image(&mut self) -> Result<image::GrayImage> {
        match self.images_paths.next() {
            Some(path) => {
                let image = image::open(path)?;
                let vertical_image = image.rotate90(); // rotate 90 degrees to match camera orientation
                let grayscale_image = vertical_image.to_luma8(); // convert to grayscale
                Ok(grayscale_image)
            }
            None => {
                let error = io::Error::new(io::ErrorKind::NotFound, "No more images");
                Err(error.into())
            }
        }
    }
}

impl Camera for DiskCamera {
    fn acquire_from_camera(
        &mut self,
        stop_token: Arc<AtomicBool>,
        acquisition_loop: &mut AcquisitionLoop,
    ) -> anyhow::Result<()> {
        return acquisition_loop.run(stop_token, self);
    }

    //fn calibration(&self) -> &calibration::Calibration {
    //    return &self.calibration;
    //}
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
        request::{Request, ReuseFlag},
        stream::StreamRole,
    };
    use std::time::Duration;

    // drm-fourcc does not have MJPEG type yet, construct it from raw fourcc identifier
    //const MJPEG: PixelFormat = PixelFormat::new(u32::from_le_bytes([b'M', b'J', b'P', b'G']), 0);

    const YUV420: PixelFormat = PixelFormat::new(DrmFourcc::Yuv420 as u32, 0);

    struct RealOpenCamera<'d> {
        camera: ActiveCamera<'d>,
        stream: libcamera::stream::Stream,
        frame_size: libcamera::geometry::Size,
        requests: Vec<Request>,
        rx: std::sync::mpsc::Receiver<Request>,
    }

    impl OpenCamera for RealOpenCamera<'_> {
        fn get_image(&mut self) -> Result<image::GrayImage> {
            let req = self.requests.pop().ok_or(CameraError::InvalidRequest)?;
            self.camera.queue_request(req).unwrap();

            info!("Waiting for camera request execution");
            let mut req = self.rx.recv_timeout(Duration::from_secs(2))?;
            info!("Camera request {:?} completed!", req);
            info!("Metadata: {:#?}", req.metadata());
            // Get framebuffer for our stream
            let framebuffer: &MemoryMappedFrameBuffer<FrameBuffer> = req
                .buffer(&self.stream)
                .ok_or(CameraError::InvalidRequest)?;
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
            self.requests.push(req);

            let image = image::GrayImage::from_raw(
                self.frame_size.width,
                self.frame_size.height,
                buffer_data,
            )
            .ok_or(CameraError::InvalidRequest)?;
            return Ok(image);
        }
    }

    pub struct PiCamera {
        pub num_buffers: u32,
    }

    impl Camera for PiCamera {
        fn acquire_from_camera(
            &mut self,
            stop_token: Arc<AtomicBool>,
            acquisition_loop: &mut AcquisitionLoop,
        ) -> Result<()> {
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
            let reqs = buffers
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

            let mut open_camera = RealOpenCamera {
                camera: cam,
                stream: stream,
                frame_size: frame_size,
                requests: reqs,
                rx: rx,
            };

            return acquisition_loop.run(stop_token, &mut open_camera);
        }
    }
}
