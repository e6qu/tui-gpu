struct VertexOut {
    @builtin(position) position: vec4<f32>;
};

@vertex
fn vs_main(@location(0) position: vec2<f32>) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    let color = vec3<f32>(0.12, 0.62, 0.89);
    return vec4<f32>(color, 1.0);
}
