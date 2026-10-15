// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The last pass of a frame: the picture that was put together in half-float precision becomes the 8-bit picture that is given back.
// (This is where a display transform goes when there is one: in compatibility mode the numbers already are what the screen is shown.)

@group(0) @binding(0) var accumulation: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let colour = textureLoad(accumulation, vec2<i32>(position.xy), 0);
    return vec4<f32>(clamp(colour.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
