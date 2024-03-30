pub mod cameras {
    use std::path::PathBuf;
    use std::{io, vec::IntoIter};

    pub trait GrayscaleCamera {
        fn get_image(&mut self) -> Option<image::GrayImage>;
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
}
