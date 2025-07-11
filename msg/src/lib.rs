pub static DEFAULT_SERVER_PORT: &str = "12345";

pub mod command {
    use serde;

    #[derive(serde::Deserialize, serde::Serialize, Debug)]
    pub enum Command {
        Status,
        Replay,
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
