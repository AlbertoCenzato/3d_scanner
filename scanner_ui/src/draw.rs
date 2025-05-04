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

pub fn cylinder(r: f32, origin: Vec3, axis: Vec3, points: &mut Vec<Vec3>) {
    const STEPS: u32 = 20;

    let top_face = origin + axis;
    let v1 = axis.any_orthonormal_vector();
    let v2 = axis.cross(v1);
    circle(r, origin, v1, v2, points);
    for step in (0..STEPS) {
        let s = (step as f32) / (STEPS as f32);
        let center = origin.lerp(top_face, s);
        circumference(r, center, v1, v2, points);
    }
    circle(r, origin + axis, v1, v2, points);
}
