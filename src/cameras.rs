use std::path::{Path, PathBuf};
use std::{io, vec::IntoIter};

pub enum CameraType {
    DiskLoader(std::path::PathBuf),
    #[cfg(feature = "camera")]
    RaspberryPi,
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
