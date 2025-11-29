use glam::Vec3;

pub fn ply_encode(points: &[Vec3]) -> String {
    let mut ply_data = String::new();
    ply_data.push_str("ply\n");
    ply_data.push_str("format ascii 1.0\n");
    ply_data.push_str(&format!("element vertex {}\n", points.len()));
    ply_data.push_str("property float x\n");
    ply_data.push_str("property float y\n");
    ply_data.push_str("property float z\n");
    ply_data.push_str("end_header\n");
    for p in points {
        ply_data.push_str(&format!("{} {} {}\n", p.x, p.y, p.z));
    }
    ply_data
}
