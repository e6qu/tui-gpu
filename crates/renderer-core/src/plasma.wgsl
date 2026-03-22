struct Params {
    resolution: vec2<f32>,
    time: f32,
    _pad: f32,
};

@group(0) @binding(0)
var<uniform> params: Params;

@group(0) @binding(1)
var output_tex: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= u32(params.resolution.x) || id.y >= u32(params.resolution.y) {
        return;
    }
    let uv = vec2<f32>(
        f32(id.x) / max(params.resolution.x, 1.0),
        f32(id.y) / max(params.resolution.y, 1.0),
    );
    let value = sin(uv.x * 12.0 + params.time)
        + cos(uv.y * 10.0 - params.time * 0.6)
        + sin(length(uv - 0.5) * 18.0 + params.time * 0.9);
    let normalized = clamp(value * 0.25 + 0.5, 0.0, 1.0);
    let r = normalized;
    let g = normalized * 0.85 + 0.15;
    let b = (1.0 - normalized) * 0.75 + 0.25;
    textureStore(output_tex, vec2<i32>(i32(id.x), i32(id.y)), vec4<f32>(r, g, b, 1.0));
}
