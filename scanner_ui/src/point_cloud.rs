use glam::Vec3;

fn ply_header(num_points: usize) -> String {
    let mut header = String::new();
    header.push_str("ply\n");
    header.push_str("format binary_little_endian 1.0\n");
    header.push_str(&format!("element vertex {}\n", num_points));
    header.push_str("property float x\n");
    header.push_str("property float y\n");
    header.push_str("property float z\n");
    header.push_str("end_header\n");
    header
}

pub fn ply_encode(points: &[Vec3]) -> Vec<u8> {
    let header = ply_header(points.len());
    let header_bytes = header.as_bytes();
    let mut ply_data = Vec::with_capacity(header_bytes.len() + points.len() * 12); // 12 bytes per point (3 * 4 bytes)

    // Add header
    ply_data.extend_from_slice(header_bytes);

    // Add point data
    for point in points {
        ply_data.extend_from_slice(&point.x.to_le_bytes());
        ply_data.extend_from_slice(&point.y.to_le_bytes());
        ply_data.extend_from_slice(&point.z.to_le_bytes());
    }

    ply_data
}
