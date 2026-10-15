// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The effect `engine:colour_matrix`: the colours of a layer's picture are mixed with a 3 by 3 matrix, the same for every pixel, and cut off at 0 and 1; how
// clear each pixel is does not change. It is what `ColourMatrix::apply_to_rgba_pixels` does to the window's pictures, but in half-float numbers, with no
// 8-bit step in the middle. The picture is premultiplied (its colours are already multiplied by how opaque it is), so it is un-premultiplied for the mixing
// and premultiplied again after it.

struct Mixing {
    // rows of the matrix: how much of the red, green and blue the new red (the first row), the new green and the new blue are made of
    to_new: mat3x3<f32>,
}

@group(0) @binding(0) var<uniform> mixing: Mixing;
@group(0) @binding(1) var layer_picture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let premultiplied = textureLoad(layer_picture, vec2<i32>(position.xy), 0);
    let opacity = premultiplied.a;
    // (A pixel that is all clear has no colour to mix, and stays all clear.)
    var colour = vec3<f32>(0.0);
    if opacity > 0.0 {
        colour = premultiplied.rgb / opacity;
    }
    let mixed = clamp(mixing.to_new * colour, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(mixed * opacity, opacity);
}
