// screensaver.wgsl
// Full-screen triangle vertex shader + fragment shader animating with time.

struct TimeUniform {
    time: f32,
};

@group(0) @binding(0)
var<uniform> u_time: TimeUniform;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>
};

// Generate a full-screen triangle without vertex buffers.
@vertex
fn vs_main(@builtin(vertex_index) vtx_idx: u32) -> VertexOut {
    var verts = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0)
    );
    var out: VertexOut;
    let pos = verts[vtx_idx];
    out.pos = vec4<f32>(pos, 0.0, 1.0);
    // Map from clip space (-1..1) to 0..1 UV
    out.uv = pos * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u_time.time;

    // Simple moving color bands using sin and uv
    let r = 0.5 + 0.5 * sin((uv.x * 10.0 + t) * 2.0);
    let g = 0.5 + 0.5 * sin((uv.y * 10.0 + t * 0.8) * 2.0 + 1.0);
    let b = 0.5 + 0.5 * sin(((uv.x + uv.y) * 8.0 - t * 1.2) * 2.0 + 2.0);

    // vignette
    let d = distance(uv, vec2<f32>(0.5, 0.5));
    let vignette = smoothstep(0.8, 0.4, d);

    let col = vec3<f32>(r, g, b) * vignette;

    return vec4<f32>(col, 1.0);
}
