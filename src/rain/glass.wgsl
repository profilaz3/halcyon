// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) motixo and Cosmic Forge contributors
// Copyright (c) 2026 profilaz3 and Halcyon contributors
//
// Water lens outline and lighting adapted from Cosmic Forge's rain-glass
// shader: https://github.com/motixo/cosmic-forge-shaders,
// shaders/rain-glass.frag (`lens_shape` and `lens_light`), licensed
// GPL-3.0-only. Ported from GLSL to WGSL for Halcyon. The outline is
// evaluated per drop in Halcyon's reduced-resolution water field; the
// lighting runs at full resolution from the decoded lens coordinates.
// Halcyon's changes: the outline's lobes scale with a roughness and an
// oscillation that changes over time, so beads stay round and sliding drops
// wobble; the lens shows a flipped, slightly magnified view with barrel
// compression at the rim, scaled by Halcyon's Refraction control; and the
// lighting is a soft highlight, a light caustic and a thin dark rim.
// This file is concatenated ahead of rain.wgsl when the shader is built.

// Slowly varying water outline. `rough` scales the lobes (0 is a smooth
// round bead, 1 a fresh splash); `taper` narrows the top of a sliding drop
// into a teardrop; `wobble` is this moment's oscillation: x stretches the
// outline into an oval, y turns the lobes.
fn lens_shape(q_in: vec2<f32>, traits: vec3<f32>, rough: f32, taper: f32, wobble: vec2<f32>) -> vec2<f32> {
    var q = q_in;
    q.x /= 1. + taper * clamp(q.y, -1., 1.);
    let r2 = dot(q, q);
    let direction = q * inverseSqrt(max(r2, 0.0001));
    var tilt = normalize(vec2(2. * traits.z - 1., 0.58 + traits.y));
    let angle = wobble.y + rough * (fract(traits.x * 7.13 + traits.z * 3.37) - 0.5) * 6.283185;
    let axis = vec2(cos(angle), sin(angle));
    tilt = tilt.x * axis + tilt.y * vec2(-axis.y, axis.x);
    let x = dot(direction, tilt);
    let y = dot(direction, vec2(-tilt.y, tilt.x));
    let lobe = x * (4. * x * x - 3.);
    let ripple = 4. * x * y * (x * x - y * y);
    let oval = direction.y * direction.y * 2. - 1.;
    let lobes = rough * (0.05 + 0.05 * traits.y + 0.14 * rough * traits.z);
    let ripples = rough * (0.08 * traits.x - 0.04) * (1. + rough);
    let contour = clamp(
        1. + lobes * lobe + 0.06 * rough * (traits.z - 0.5) * y + ripples * ripple + wobble.x * oval,
        0.7,
        1.3,
    );
    return q / contour;
}

struct Water {
    bend: vec2<f32>,
    clear: f32,
    shine: f32,
    shade: f32,
};

// Light through one lens. `q` is the shaped lens coordinate (1 at the rim,
// +y down), `cover` its coverage, `refraction` Halcyon's control (0 to 1).
// The bend is in drop radii from the pixel: a drop shows the scene behind
// its centre, upside down and slightly magnified, compressed toward the rim
// like a real bead of water.
fn lens_light(q: vec2<f32>, cover: f32, refraction: f32) -> Water {
    let r2 = min(dot(q, q), 1.);
    let flip = clamp(refraction * 2.5, 0., 1.);
    let scale = 0.55 + 0.7 * refraction;
    let bend = -q * flip * (1. + scale * (1. + 0.4 * r2));
    // A thin dark rim where the steep edge reflects the dark room, deeper
    // on the upper side.
    let rim = smoothstep(0.7, 0.97, r2) * (0.42 + 0.22 * max(0., -q.y));
    // A soft highlight high on the dome, and light gathered into the lower
    // inside edge.
    let h = q - vec2(-0.3, -0.42);
    let spot = exp(-dot(h, h) * 16.) * 0.62;
    let caustic = smoothstep(0.3, 0.8, r2) * (1. - smoothstep(0.82, 0.97, r2))
        * smoothstep(0.15, 0.75, q.y) * 0.3;
    return Water(bend * cover, cover, (spot + caustic) * cover, rim * cover);
}
