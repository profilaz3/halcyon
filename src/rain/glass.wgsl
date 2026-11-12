// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) motixo and Cosmic Forge contributors
// Copyright (c) 2026 profilaz3 and Halcyon contributors
//
// Water lens outline and lighting adapted from Cosmic Forge's rain-glass
// shader: https://github.com/motixo/cosmic-forge-shaders,
// shaders/rain-glass.frag (`lens_shape` and `lens_light`), licensed
// GPL-3.0-only. Ported from GLSL to WGSL for Halcyon. The outline is
// evaluated per drop in Halcyon's reduced-resolution water field; the
// lighting runs at full resolution from the decoded lens coordinates, and the
// bend is expressed in drop radii so Halcyon's Refraction control scales it.
// This file is concatenated ahead of rain.wgsl when the shader is built.

// Irregular, slowly varying bead outline. `taper` narrows the top of a moving
// drop into a teardrop; `splash` roughens a fresh impact.
fn lens_shape(q_in: vec2<f32>, traits: vec3<f32>, splash: f32, taper: f32) -> vec2<f32> {
    var q = q_in;
    q.x /= 1. + taper * clamp(q.y, -1., 1.);
    let r2 = dot(q, q);
    let direction = q * inverseSqrt(max(r2, 0.0001));
    var tilt = normalize(vec2(2. * traits.z - 1., 0.58 + traits.y));
    if splash > 0. {
        let angle = splash * (fract(traits.x * 7.13 + traits.z * 3.37) - 0.5) * 6.283185;
        let axis = vec2(cos(angle), sin(angle));
        tilt = tilt.x * axis + tilt.y * vec2(-axis.y, axis.x);
    }
    let x = dot(direction, tilt);
    let y = dot(direction, vec2(-tilt.y, tilt.x));
    let lobe = x * (4. * x * x - 3.);
    let ripple = 4. * x * y * (x * x - y * y);
    let lobes = 0.13 + 0.10 * traits.y + splash * mix(0.07, 0.23, traits.z);
    let ripples = 0.17 * traits.x - 0.085 + splash * (0.38 * traits.x - 0.19);
    let contour = clamp(
        0.98 + lobes * lobe + (0.12 + 0.12 * traits.z) * y + ripples * ripple,
        0.68 - 0.12 * splash,
        1.18 + 0.22 * splash,
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
// +y down), `cover` its coverage. The bend is in drop radii and points to the
// opposite side, so the drop shows a flipped, minified view of the scene.
fn lens_light(q: vec2<f32>, cover: f32) -> Water {
    let r2 = min(dot(q, q), 1.);
    let dome = sqrt(1. - r2);
    let lip = smoothstep(0.36, 0.94, r2);
    let low = max(0., q.y);
    var glint = clamp(dot(vec3(q, dome), vec3(-0.39, -0.57, 0.72)), 0., 1.);
    glint *= glint;
    glint *= glint;
    glint *= glint;
    glint *= glint;
    return Water(
        -q * (1.08 + 1.38 * dome) * cover,
        cover,
        (lip * low * 0.47 + glint * 0.24) * cover,
        lip * (0.10 + 0.48 * max(0., -q.y)) * cover,
    );
}
