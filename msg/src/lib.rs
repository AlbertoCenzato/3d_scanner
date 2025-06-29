pub static DEFAULT_SERVER_PORT: &str = "12345";

pub mod command {
    use serde;

    #[derive(serde::Deserialize, serde::Serialize, Debug)]
    pub enum Command {
        Status,
        Replay,
    }

    impl Command {
        pub fn from_bytes(bytes: &[u8]) -> Result<Command, rmp_serde::decode::Error> {
            rmp_serde::from_slice(bytes)
        }

        pub fn to_bytes(&self) -> Vec<u8> {
            rmp_serde::to_vec(self).expect("Failed to serialize command")
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
        pub fn from_bytes(bytes: &[u8]) -> Result<Response, rmp_serde::decode::Error> {
            rmp_serde::from_slice(bytes)
        }

        pub fn to_bytes(&self) -> Vec<u8> {
            rmp_serde::to_vec(self).expect("Failed to serialize response")
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
