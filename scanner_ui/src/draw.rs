use glam::Vec3;

pub fn circumference(r: f32, center: Vec3, v1: Vec3, v2: Vec3, points: &mut Vec<Vec3>) {
    for t in 0..360 {
        let alpha = (t as f32).to_radians();
        let p = center + r * alpha.cos() * v1 + r * alpha.sin() * v2;
        points.push(p);
    }
}

pub fn circle(r: f32, center: Vec3, v1: Vec3, v2: Vec3, points: &mut Vec<Vec3>) {
    const STEPS: u32 = 20;
    for s in 0..STEPS {
        let radius = (s as f32) * r / (STEPS as f32);
        circumference(radius, center, v1, v2, points);
    }
}

pub fn segment(p1: Vec3, p2: Vec3) -> Vec<Vec3> {
    const STEPS: u32 = 20;
    (0..STEPS)
        .map(|t| {
            let s = (t as f32) / (STEPS as f32);
            p1.lerp(p2, s)
        })
        .collect()
}

pub fn cylinder(r: f32, origin: Vec3, axis: Vec3, steps: u32, points: &mut Vec<Vec3>) {
    let top_face = origin + axis;
    let (v1, v2) = axis.normalize().any_orthonormal_pair();
    circle(r, origin, v1, v2, points);
    for step in 0..steps {
        let s = (step as f32) / (steps as f32);
        let center = origin.lerp(top_face, s);
        circumference(r, center, v1, v2, points);
    }
    circle(r, origin + axis, v1, v2, points);
}

fn lerp(a: f32, b: f32, s: f32) -> f32 {
    a + (b - a) * s
}

pub fn cone(base_radius: f32, base_center: Vec3, axis: Vec3, steps: u32, points: &mut Vec<Vec3>) {
    let vertex_pos = base_center + axis;
    let (v1, v2) = axis.normalize().any_orthonormal_pair();
    circle(base_radius, base_center, v1, v2, points);
    for step in 0..steps {
        let s = (step as f32) / (steps as f32);
        let center = base_center.lerp(vertex_pos, s);
        let radius = lerp(0_f32, base_radius, 1_f32 - s);
        circumference(radius, center, v1, v2, points);
    }
}

fn ax(v: Vec3, radius: f32, points: &mut Vec<Vec3>) {
    cylinder(radius, Vec3::ZERO, v, 64, points);
    cone(2.0 * radius, v, 0.2 * v, 64, points);
}

pub fn axis(points: &mut Vec<Vec3>) {
    const RADIUS: f32 = 0.05;
    ax(Vec3::X, RADIUS, points);
    ax(Vec3::Y, RADIUS, points);
    ax(Vec3::Z, RADIUS, points);
}
