// Procedural decoration only: the native card and its content draw above this.
struct Params { size_radius_phase: vec4<f32>, accent: vec4<f32>, album: vec4<f32> }
@group(0) @binding(0) var<uniform> params: Params;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vertex(@builtin(vertex_index) id: u32) -> Vertex {
    let xy = array<vec2<f32>, 3>(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
    var out: Vertex;
    out.position = vec4(xy[id], 0., 1.);
    out.uv = vec2(xy[id].x * 0.5 + 0.5, 0.5 - xy[id].y * 0.5);
    return out;
}
@fragment fn fragment(in: Vertex) -> @location(0) vec4<f32> {
    let size = max(params.size_radius_phase.xy, vec2(1.));
    let radius = min(params.size_radius_phase.z, min(size.x, size.y) * 0.5);
    let p = in.uv * size;
    let q = abs(p - size * 0.5) - (size * 0.5 - radius);
    let distance = length(max(q, vec2(0.))) + min(max(q.x, q.y), 0.) - radius;
    let inside = 1. - smoothstep(-0.8, 0.2, distance);
    if inside <= 0. { discard; }
    if params.album.w > 0. {
        let delta = (p - params.album.xy) / max(params.album.z, 1.);
        let falloff = exp(-dot(delta, delta) * 1.65);
        let alpha = inside * falloff * params.accent.a;
        return vec4(params.accent.rgb * alpha, alpha);
    }
    let rim = exp(-abs(distance + 1.) * 0.65);
    let grain = fract(sin(dot(floor(p), vec2(12.9898, 78.233))) * 43758.5453);
    let wash = pow(max(0., 1. - in.uv.y), 3.) * 0.055;
    let phase = params.size_radius_phase.w;
    var glint = 0.;
    if phase >= 0. {
        let center = phase * 1.8 - 0.4;
        glint = exp(-pow((in.uv.x + in.uv.y * 0.3 - center) / 0.12, 2.))
            * sin(phase * 3.14159265) * 0.32;
    }
    let alpha = inside * (rim * (0.22 + glint) + wash + grain * 0.014);
    let color = mix(params.accent.rgb, vec3(1.), 0.65);
    return vec4(color * alpha, alpha);
}
