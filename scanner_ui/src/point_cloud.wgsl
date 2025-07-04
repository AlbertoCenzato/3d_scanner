// point_cloud.wgsl
struct Camera {
    view_proj: mat4x4<f32>
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;

    out.position = camera.view_proj * vec4<f32>(pos, 1.0);
    let r = clamp(pos.x/ 3 , 0, 1);
    let g = clamp(pos.y/ 3 , 0, 1);
    let b = clamp(pos.z/ 3 , 0, 1);
    out.color = vec3<f32>(r, g, b); // white
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
