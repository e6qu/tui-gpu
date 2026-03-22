struct Params {
    resolution: vec2<f32>,
    time: f32,
    _pad: f32,
};

@group(0) @binding(0)
var<uniform> params: Params;

@group(0) @binding(1)
var output_tex: texture_storage_2d<rgba8unorm, write>;

fn dot3(a: vec3<f32>, b: vec3<f32>) -> f32 {
    return a.x * b.x + a.y * b.y + a.z * b.z;
}

fn normalize3(v: vec3<f32>) -> vec3<f32> {
    let len = sqrt(max(dot3(v, v), 1e-5));
    return v / len;
}

fn reflect3(v: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    return v - 2.0 * dot3(v, n) * n;
}

fn intersect_sphere(origin: vec3<f32>, dir: vec3<f32>, center: vec3<f32>, radius: f32) -> f32 {
    let oc = origin - center;
    let a = dot3(dir, dir);
    let b = 2.0 * dot3(oc, dir);
    let c = dot3(oc, oc) - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return -1.0;
    }
    let sq = sqrt(disc);
    var t = (-b - sq) / (2.0 * a);
    if t < 0.0 {
        t = (-b + sq) / (2.0 * a);
    }
    if t < 0.0 {
        return -1.0;
    }
    return t;
}

fn intersect_plane(origin: vec3<f32>, dir: vec3<f32>, point: vec3<f32>, normal: vec3<f32>) -> f32 {
    let denom = dot3(normal, dir);
    if abs(denom) < 1e-4 {
        return -1.0;
    }
    let t = dot3(point - origin, normal) / denom;
    if t < 0.0 {
        return -1.0;
    }
    return t;
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = u32(params.resolution.x);
    let height = u32(params.resolution.y);
    if id.x >= width || id.y >= height {
        return;
    }
    let aspect = params.resolution.x / max(params.resolution.y, 1.0);
    let uv = vec2<f32>(
        (f32(id.x) + 0.5) / params.resolution.x * 2.0 - 1.0,
        1.0 - (f32(id.y) + 0.5) / params.resolution.y * 2.0,
    );
    let dir = normalize3(vec3<f32>(uv.x * aspect, uv.y, -1.2));
    let origin = vec3<f32>(0.0, 0.0, 0.0);
    let light_dir = normalize3(vec3<f32>(0.4, 0.8, -0.5));
    let sphere_center = vec3<f32>(
        sin(params.time) * 1.5,
        cos(params.time) * 0.5,
        -4.0 - cos(params.time),
    );
    var color = vec3<f32>(0.0, 0.0, 0.0);
    var hit_any = false;
    let sphere_t = intersect_sphere(origin, dir, sphere_center, 1.2);
    if sphere_t > 0.0 {
        let hit = origin + dir * sphere_t;
        let normal = normalize3(hit - sphere_center);
        let diffuse = max(dot3(normal, light_dir), 0.0);
        let spec_dir = reflect3(-light_dir, normal);
        let spec = pow(max(dot3(spec_dir, -dir), 0.0), 32.0);
        let base = vec3<f32>(0.8, 0.2, 0.3);
        color = base * diffuse + vec3<f32>(spec, spec, spec) + vec3<f32>(0.1, 0.1, 0.15);
        hit_any = true;
    } else {
        let plane_t = intersect_plane(origin, dir, vec3<f32>(0.0, -1.2, 0.0), vec3<f32>(0.0, 1.0, 0.0));
        if plane_t > 0.0 {
        let hit = origin + dir * plane_t;
        let checker = mod(floor(hit.x * 1.5) + floor(hit.z * 1.5), 2.0);
            let base = mix(vec3<f32>(0.2, 0.2, 0.25), vec3<f32>(0.9, 0.9, 0.95), checker);
            let diffuse = max(dot3(vec3<f32>(0.0, 1.0, 0.0), light_dir), 0.0);
            color = base * (diffuse * 0.8 + 0.2);
            hit_any = true;
        }
    }
    if !hit_any {
        let t = 0.5 * (dir.y + 1.0);
        color = mix(vec3<f32>(0.2, 0.3, 0.6), vec3<f32>(0.05, 0.05, 0.1), t);
    }
    textureStore(
        output_tex,
        vec2<i32>(i32(id.x), i32(id.y)),
        vec4<f32>(color, 1.0),
    );
}
