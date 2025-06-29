use log;

pub static DEFAULT_SERVER_PORT: &str = "12345";

pub mod command {
    use serde;

    #[derive(serde::Deserialize, serde::Serialize, Debug)]
    pub enum Command {
        Status,
        Replay,
    }

    impl Command {
        pub fn from_text(text: &str) -> Result<Command, serde_json::Error> {
            serde_json::from_str(text)
        }

        pub fn to_text(&self) -> String {
            match serde_json::to_string(self) {
                Ok(text) => text,
                Err(e) => {
                    log::error!("Failed to serialize command: {}", e);
                    String::new()
                }
            }
        }
    }
}

pub mod response {
    use serde;

    #[derive(serde::Deserialize, serde::Serialize)]
    pub enum Response {
        Ok,
        Error(String),
        Close,
        Status(Status),
        PointCloud(PointCloud),
    }

    impl Response {
        pub fn from_text(text: &str) -> Result<Response, serde_json::Error> {
            serde_json::from_str(text)
        }

        pub fn to_text(&self) -> String {
            match serde_json::to_string(self) {
                Ok(text) => text,
                Err(e) => {
                    log::error!("Failed to serialize response: {}", e);
                    String::new()
                }
            }
        }
    }

    #[derive(serde::Deserialize, serde::Serialize)]
    pub struct Status {
        pub lasers: LasersData,
        pub motor_speed: f32,
    }

    #[derive(serde::Deserialize, serde::Serialize)]
    pub struct LasersData {
        pub laser_1: bool,
        pub laser_2: bool,
    }

    #[derive(serde::Deserialize, serde::Serialize)]
    pub struct PointCloud {
        pub points: Vec<glam::Vec3>,
    }
}
