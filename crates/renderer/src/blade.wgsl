struct BackgroundParams {
    color: vec4<f32>,
}

struct CameraParams {
    position: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    forward: vec4<f32>,
    clip: vec4<f32>,
    viewport: vec4<f32>,
}

struct TriShaderParams {
    values: vec4<f32>,
}

struct LineShaderParams {
    viewport_and_line_width: vec4<f32>,
    depth_bias: vec4<f32>,
}

struct DotShaderParams {
    viewport_and_radius: vec4<f32>,
    depth_bias: vec4<f32>,
}

struct TriVertexPod {
    pos: vec4<f32>,
    norm: vec4<f32>,
    col: vec4<f32>,
    uv: vec4<f32>,
}

struct LineVertexPod {
    pos: vec4<f32>,
    col: vec4<f32>,
    tangent: vec4<f32>,
    prev_tangent: vec4<f32>,
    extrude: vec4<f32>,
}

struct DotInstancePod {
    pos: vec4<f32>,
    col: vec4<f32>,
}

struct ProjectedPoint {
    clip: vec4<f32>,
    ndc: vec2<f32>,
}

struct TriOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) model: vec3<f32>,
    @location(3) normal: vec3<f32>,
}

struct ColorOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
}

struct DotOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
}

var<uniform> background: BackgroundParams;

var<uniform> tri_camera: CameraParams;
var<uniform> tri_params: TriShaderParams;
var t_color: texture_2d<f32>;
var s_color: sampler;
var<storage, read> tri_vertices: array<TriVertexPod>;

var<uniform> line_camera: CameraParams;
var<uniform> line_params: LineShaderParams;
var<storage, read> line_vertices: array<LineVertexPod>;

var<uniform> dot_camera: CameraParams;
var<uniform> dot_params: DotShaderParams;
var<storage, read> dot_instances: array<DotInstancePod>;

const QUAD_POSITIONS: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(-1.0, 1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(1.0, 1.0),
);

const LIGHT_SRC: vec3<f32> = vec3<f32>(1.0, 1.0, 0.0);
const GAMMA: f32 = 3.0;
const ALPHA_CUTOFF: f32 = 1.0 / 255.0;

fn world_to_camera(world: vec3<f32>, camera: CameraParams) -> vec3<f32> {
    let relative = world - camera.position.xyz;
    return vec3<f32>(
        dot(relative, camera.right.xyz),
        dot(relative, camera.up.xyz),
        dot(relative, camera.forward.xyz),
    );
}

fn normal_to_camera(normal: vec3<f32>, camera: CameraParams) -> vec3<f32> {
    return vec3<f32>(
        dot(normal, camera.right.xyz),
        dot(normal, camera.up.xyz),
        dot(normal, camera.forward.xyz),
    );
}

fn vector_to_camera(vector: vec3<f32>, camera: CameraParams) -> vec3<f32> {
    return vec3<f32>(
        dot(vector, camera.right.xyz),
        dot(vector, camera.up.xyz),
        dot(vector, camera.forward.xyz),
    );
}

fn project_camera(model: vec3<f32>, camera: CameraParams, depth_bias: f32) -> ProjectedPoint {
    let camera_x = model.x;
    let camera_y = model.y;
    let camera_z = model.z;

    let tan_half_fov = max(camera.clip.z, 0.05);
    let aspect = max(camera.clip.w, 0.1);
    let viewport_scale = max(camera.viewport.xy, vec2<f32>(1e-6));
    let near = camera.clip.x;
    let far = max(camera.clip.y, near + 0.0001);

    let clip_w = camera_z;
    let clip_x = camera_x / (tan_half_fov * aspect) * viewport_scale.x;
    let clip_y = camera_y / tan_half_fov * viewport_scale.y;
    let clip_z = far * camera_z / (far - near) - (far * near) / (far - near);

    var clip = vec4<f32>(clip_x, clip_y, clip_z, clip_w);
    clip.z -= depth_bias * clip.w;
    let inv_w = 1.0 / max(abs(clip.w), 1e-6);

    return ProjectedPoint(clip, clip.xy * inv_w);
}

fn project(world: vec3<f32>, camera: CameraParams, depth_bias: f32) -> ProjectedPoint {
    return project_camera(world_to_camera(world, camera), camera, depth_bias);
}

fn safe_normalize3(v: vec3<f32>) -> vec3<f32> {
    if (dot(v, v) > 1e-6) {
        return normalize(v);
    }
    return vec3<f32>(0.0);
}

@vertex
fn vs_background(@builtin(vertex_index) vertex_index: u32) -> ColorOut {
    var out: ColorOut;
    out.pos = vec4<f32>(QUAD_POSITIONS[vertex_index], 0.0, 1.0);
    out.color = background.color;
    return out;
}

@fragment
fn fs_background(in: ColorOut) -> @location(0) vec4<f32> {
    return in.color;
}

@vertex
fn vs_triangle(@builtin(vertex_index) vertex_index: u32) -> TriOut {
    let vertex = tri_vertices[vertex_index];
    let projected = project(vertex.pos.xyz, tri_camera, tri_params.values.y);
    let model = world_to_camera(vertex.pos.xyz, tri_camera);

    var out: TriOut;
    out.pos = projected.clip;
    out.color = vertex.col;
    out.uv = vertex.uv.xy;
    out.model = model;
    out.normal = normal_to_camera(vertex.norm.xyz, tri_camera);
    return out;
}

@fragment
fn fs_triangle(in: TriOut) -> @location(0) vec4<f32> {
    let sampled = textureSample(t_color, s_color, in.uv);
    let normal = normalize(in.normal);
    let light_dir = normalize(in.model - LIGHT_SRC);
    let gloss = max(tri_params.values.z, 0.0);
    let specular = gloss * pow(max(dot(light_dir, normal), 0.0), GAMMA);
    let lit_rgb = in.color.rgb + (vec3<f32>(1.0) - in.color.rgb) * specular;
    let alpha = in.color.a * sampled.a * tri_params.values.x;
    if (alpha <= ALPHA_CUTOFF) {
        discard;
    }
    return vec4<f32>(lit_rgb * sampled.rgb, alpha);
}

@vertex
fn vs_line(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> ColorOut {
    let vertex = line_vertices[instance_index * 6u + vertex_index];
    let model = world_to_camera(vertex.pos.xyz, line_camera);

    let viewport = max(line_params.viewport_and_line_width.xy, vec2<f32>(1.0));
    let radius_px = max(line_params.viewport_and_line_width.z, 0.0);
    let tangent = vector_to_camera(vertex.tangent.xyz, line_camera);
    let prev_tangent = vector_to_camera(vertex.prev_tangent.xyz, line_camera);
    let extrude = vertex.extrude.x;

    let used_normal = safe_normalize3(cross(tangent, vec3<f32>(0.0, 0.0, 1.0)));
    let prev_normal = safe_normalize3(cross(prev_tangent, vec3<f32>(0.0, 0.0, 1.0)));
    let miter_clip = 0.5 * (prev_normal + used_normal);
    let miter_dot = dot(miter_clip, used_normal);
    var unclipped = vec3<f32>(0.0);
    if (abs(miter_dot) > 1e-6) {
        unclipped = miter_clip / miter_dot;
    }
    let tan_half_fov = max(line_camera.clip.z, 0.05);
    let aspect = max(line_camera.clip.w, 0.1);
    let scale = 2.0 * radius_px * model.z * tan_half_fov * aspect / viewport.x * extrude;
    let max_miter_scale = max(line_params.depth_bias.y, 0.0);
    let unclipped_length_sq = dot(unclipped, unclipped);
    var full_normal = unclipped * scale;
    if (dot(miter_clip, miter_clip) <= 1e-6 || unclipped_length_sq > max_miter_scale * max_miter_scale) {
        full_normal = miter_clip * scale;
    }
    let projected = project_camera(model + full_normal, line_camera, line_params.depth_bias.x);

    var out: ColorOut;
    out.pos = projected.clip;
    out.color = vec4<f32>(vertex.col.rgb, vertex.col.a * line_params.viewport_and_line_width.w);
    return out;
}

@fragment
fn fs_line(in: ColorOut) -> @location(0) vec4<f32> {
    if (in.color.a <= ALPHA_CUTOFF) {
        discard;
    }
    return in.color;
}

@vertex
fn vs_dot(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> DotOut {
    let instance = dot_instances[instance_index];
    let projected = project(instance.pos.xyz, dot_camera, dot_params.depth_bias.x);
    let viewport = max(dot_params.viewport_and_radius.xy, vec2<f32>(1.0));
    let vertex_count = max(u32(dot_params.depth_bias.y), 3u);
    let angle = 2.0 * 3.141592653589793 * f32(vertex_index) / f32(vertex_count);
    let local = vec2<f32>(cos(angle), sin(angle));
    let radius_px = max(dot_params.viewport_and_radius.z, 0.0);
    let offset_ndc = local * radius_px * vec2<f32>(2.0 / viewport.x, 2.0 / viewport.y);

    var out: DotOut;
    let position_xy = (projected.ndc + offset_ndc) * projected.clip.w;
    out.pos = vec4<f32>(position_xy, projected.clip.z, projected.clip.w);
    out.color = vec4<f32>(instance.col.rgb, instance.col.a * dot_params.viewport_and_radius.w);
    return out;
}

@fragment
fn fs_dot(in: DotOut) -> @location(0) vec4<f32> {
    if (in.color.a <= ALPHA_CUTOFF) {
        discard;
    }
    return in.color;
}
