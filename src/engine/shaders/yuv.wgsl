// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// A video picture in planes (brightness at every pixel, and two colour differences at half the width and half the height) becomes red, green and blue,
// premultiplied (it is opaque, so that is the same), in half-float numbers. Done once, when the picture is uploaded. The same arithmetic as
// `YuvConversion::rgb_of` in Rust, which the tests hold this to.

struct Conversion {
    // rows of the matrix (red, green, blue), each as the numbers for (y, u, v) after what is taken off them is taken off
    to_rgb: mat3x3<f32>,
    offsets: vec3<f32>,
}

@group(0) @binding(0) var<uniform> conversion: Conversion;
@group(0) @binding(1) var brightness_plane: texture_2d<f32>;
@group(0) @binding(2) var blue_difference_plane: texture_2d<f32>;
@group(0) @binding(3) var red_difference_plane: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let brightness = textureLoad(brightness_plane, pixel, 0).r;
    // The colour differences have a pixel for every square of four pixels, and each of them is the colour difference of all four: the pixel of a square
    // is read, and not blended with its neighbours' (which is what ffmpeg does, and so the pictures are what they always were; see
    // `reference_rgba` in Rust).
    let chroma_pixel = pixel / 2;
    let blue_difference = textureLoad(blue_difference_plane, chroma_pixel, 0).r;
    let red_difference = textureLoad(red_difference_plane, chroma_pixel, 0).r;
    let rgb = conversion.to_rgb * (vec3<f32>(brightness, blue_difference, red_difference) - conversion.offsets);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
