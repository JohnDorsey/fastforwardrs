// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The export's first pass over a finished frame: the brightness plane of the `yuv420p` picture that the encoder is given. The frame is the 8-bit one that the
// preview shows (the `output` pass made it), read pixel by pixel; the brightness of a pixel is a row of the matrix of its standard and range, and the
// 8-bit texture it is written to rounds it. The same arithmetic as `RgbToYuvConversion` in Rust, which the tests hold this to.

struct Conversion {
    // for brightness, the blue and the red colour difference: the numbers that multiply red, green and blue, and what is added after
    to_brightness: vec4<f32>,
    to_blue_difference: vec4<f32>,
    to_red_difference: vec4<f32>,
}

@group(0) @binding(0) var<uniform> conversion: Conversion;
@group(0) @binding(1) var frame: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let colour = textureLoad(frame, vec2<i32>(position.xy), 0).rgb;
    let brightness = dot(conversion.to_brightness.xyz, colour) + conversion.to_brightness.w;
    return vec4<f32>(brightness, 0.0, 0.0, 1.0);
}
