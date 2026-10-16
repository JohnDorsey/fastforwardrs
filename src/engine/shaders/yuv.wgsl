// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// A video picture in planes (brightness at every pixel, and two colour differences at half the width and half the height) becomes red, green and blue (the codes
// that the matrix and the range make), then the numbers of the working space (decoded to light with the picture's curve and converted to the working space's primaries;
// in the compatibility mode the codes as they are), premultiplied (it is opaque, so that is the same), in half-float numbers. Done once, when the picture is uploaded.
// The same arithmetic as `YuvConversion::rgb_of` in Rust, which the tests hold this to. (The curves are in `colour.wgsl`, which is in front of this text.)
//
// **The colour differences** are copied to the four pixels of a square (the compatibility mode: what ffmpeg makes, which the tests against the old drawing need), or blended
// from the four squares round the pixel (linear light), by where the file says they sit; `smooth` says which. The arithmetic of the blend is `reference_rgba_smooth` in Rust.

struct Conversion {
    // rows of the matrix (red, green, blue), each as the numbers for (y, u, v) after what is taken off them is taken off
    to_rgb: mat3x3<f32>,
    offsets: vec3<f32>,
    // the number of the curve that the picture's codes are decoded with (`TransferFunction::number_for_the_shaders`): in the four bytes that the offsets leave over
    curve: u32,
    // the light of the picture's primaries to the working space's
    to_working: mat3x3<f32>,
    // (1 or 0: whether the colour differences are blended, and then how far across and down from the middle of the first pixel of a square they sit, in pixels)
    chroma: vec4<f32>,
}

@group(0) @binding(0) var<uniform> conversion: Conversion;
@group(0) @binding(1) var brightness_plane: texture_2d<f32>;
@group(0) @binding(2) var blue_difference_plane: texture_2d<f32>;
@group(0) @binding(3) var red_difference_plane: texture_2d<f32>;

// The four colour differences at the columns `a.x` and `b.x` and the rows `a.y` and `b.y`, blended by `weight` (across, down) of the way from `a` to `b`.
fn blend_of_four(plane: texture_2d<f32>, a: vec2<i32>, b: vec2<i32>, weight: vec2<f32>) -> f32 {
    let top = textureLoad(plane, vec2<i32>(a.x, a.y), 0).r * (1.0 - weight.x) + textureLoad(plane, vec2<i32>(b.x, a.y), 0).r * weight.x;
    let bottom = textureLoad(plane, vec2<i32>(a.x, b.y), 0).r * (1.0 - weight.x) + textureLoad(plane, vec2<i32>(b.x, b.y), 0).r * weight.x;
    return top * (1.0 - weight.y) + bottom * weight.y;
}

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
    var blue_difference: f32;
    var red_difference: f32;
    if conversion.chroma.x < 0.5 {
        // The colour differences have a pixel for every square of four pixels, and each of them is the colour difference of all four: the pixel of a square
        // is read, and not blended with its neighbours' (which is what ffmpeg does, and so the pictures are what they always were; see `reference_rgba` in Rust).
        let chroma_pixel = pixel / 2;
        blue_difference = textureLoad(blue_difference_plane, chroma_pixel, 0).r;
        red_difference = textureLoad(red_difference_plane, chroma_pixel, 0).r;
    } else {
        // The colour difference of a square sits at (2i + across, 2j + down) among the pixels, so a pixel (x, y) is (x - across) / 2 of the way along the colour
        // differences: the two nearest across and the two nearest down are blended, and past the outermost the nearest is used (see `reference_rgba_smooth` in Rust).
        let last = vec2<i32>(textureDimensions(blue_difference_plane)) - vec2<i32>(1, 1);
        let along = (vec2<f32>(pixel) - conversion.chroma.yz) / 2.0;
        let first = floor(along);
        let weight = along - first;
        let a = clamp(vec2<i32>(first), vec2<i32>(0, 0), last);
        let b = clamp(vec2<i32>(first) + vec2<i32>(1, 1), vec2<i32>(0, 0), last);
        blue_difference = blend_of_four(blue_difference_plane, a, b, weight);
        red_difference = blend_of_four(red_difference_plane, a, b, weight);
    }
    let rgb = conversion.to_rgb * (vec3<f32>(brightness, blue_difference, red_difference) - conversion.offsets);
    let codes = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(conversion.to_working * decode_to_light(conversion.curve, codes), 1.0);
}
