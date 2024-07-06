use std::path::PathBuf;
use std::{io, vec::IntoIter};

pub trait GrayscaleCamera {
    fn get_image(&mut self) -> Option<image::GrayImage>;
}

pub enum CameraType {
    DiskLoader(std::path::PathBuf),
    #[cfg(feature = "camera")]
    RaspberryPi,
}

pub fn get_camera(camera_type: CameraType) -> Result<Box<dyn GrayscaleCamera>, io::Error> {
    let camera: Box<dyn GrayscaleCamera> = match camera_type {
        CameraType::DiskLoader(path) => Box::new(DiskLoaderCamera::from_directory(&path)?),
        #[cfg(feature = "camera")]
        CameraType::RaspberryPi => Box::new(raspberry::PiCamera::new()),
    };
    return Ok(camera);
}

pub struct DiskLoaderCamera {
    iter: IntoIter<PathBuf>,
}

impl DiskLoaderCamera {
    pub fn new(images: Vec<PathBuf>) -> DiskLoaderCamera {
        return DiskLoaderCamera {
            iter: images.into_iter(),
        };
    }

    pub fn from_directory(directory: &std::path::Path) -> Result<DiskLoaderCamera, io::Error> {
        let mut files: Vec<PathBuf> = directory
            .read_dir()?
            .filter_map(|f| match f {
                Ok(entry) => Some(entry.path()),
                Err(_) => None,
            })
            .collect();
        files.sort();
        return Ok(DiskLoaderCamera::new(files));
    }
}

impl GrayscaleCamera for DiskLoaderCamera {
    fn get_image(&mut self) -> Option<image::GrayImage> {
        match self.iter.next() {
            Some(path) => {
                return Some(image::open(path).unwrap().into_luma8());
            }
            None => {
                return None;
            }
        }
    }
}

#[cfg(feature = "camera")]
pub mod raspberry {
    use super::*;
    use libcamera::camera::{ActiveCamera, Camera};
    use libcamera::{camera_manager::CameraManager, logging::LoggingLevel, stream::StreamRole};

    fn test_camera() {
        let mgr = CameraManager::new().unwrap();

        mgr.log_set_level("Camera", LoggingLevel::Error);

        let cameras = mgr.cameras();

        for i in 0..cameras.len() {
            let cam = cameras.get(i).unwrap();
            println!("Camera {}", i);
            println!("ID: {}", cam.id());

            println!("Properties: {:#?}", cam.properties());

            let config = cam
                .generate_configuration(&[StreamRole::ViewFinder])
                .unwrap();
            let view_finder_cfg = config.get(0).unwrap();
            println!("Available formats: {:#?}", view_finder_cfg.formats());
        }
    }

    pub struct PiCamera {
        camera_manager: CameraManager,
        camera: Camera,
        active_camera: ActiveCamera,
    }

    impl PiCamera {
        pub fn new() -> PiCamera {
            let camera_manager = CameraManager::new()?;
            camera_manager.log_set_level("Camera", LoggingLevel::Error);
            let camera = camera_manager.cameras().get(0)?;
            let active_camera = camera.acquire()?;
            return PiCamera {
                camera_manager,
                camera,
                active_camera,
            };
        }
    }

    impl GrayscaleCamera for PiCamera {
        fn get_image(&mut self) -> Option<image::GrayImage> {
            let frame = self.camera.capture().unwrap();
            return Some(
                image::ImageBuffer::from_raw(
                    frame.width as u32,
                    frame.height as u32,
                    frame.buf.to_vec(),
                )
                .unwrap(),
            );
        }
    }
}
