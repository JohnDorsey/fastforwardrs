// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The export's second pass over a finished frame: the two colour difference planes of the `yuv420p` picture that the encoder is given, which have a pixel for
// every square of four pixels of the frame and are written side by side (blue in the red channel of the texture, red in the green one: the processor takes them
// apart). Each is the average of the colour differences of the four pixels of its square, and a square that sticks out past an odd edge counts the pixel that is
// there again for the one that is not. The same arithmetic as `reference_yuv420p` in Rust, which the tests hold this to.

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
    let first = vec2<i32>(position.xy) * 2;
    let last = vec2<i32>(textureDimensions(frame)) - vec2<i32>(1);
    var blue_sum = 0.0;
    var red_sum = 0.0;
    for (var square = 0; square < 4; square++) {
        let place = min(first + vec2<i32>(square & 1, square >> 1), last);
        let colour = textureLoad(frame, place, 0).rgb;
        blue_sum += dot(conversion.to_blue_difference.xyz, colour);
        red_sum += dot(conversion.to_red_difference.xyz, colour);
    }
    let blue_difference = blue_sum / 4.0 + conversion.to_blue_difference.w;
    let red_difference = red_sum / 4.0 + conversion.to_red_difference.w;
    return vec4<f32>(blue_difference, red_difference, 0.0, 1.0);
}
