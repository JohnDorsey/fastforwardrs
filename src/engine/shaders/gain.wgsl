// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The effects `engine:exposure` and `engine:opacity`: the picture of a layer, which is premultiplied (its colours are already multiplied by how opaque it is), is
// multiplied by four numbers, one for each of the red, the green, the blue and the opacity. Exposure is a gain of the three colours by two to the stops (the opacity is
// not changed); opacity is the same number for all four, which keeps the picture premultiplied. **They are effects of the scene-linear domain** (CM-10): the numbers
// are the light of the working space, as they are, and nothing is converted.

struct Gain {
    // the factors of the red, green, blue and opacity of the premultiplied picture
    factors: vec4<f32>,
}

@group(0) @binding(0) var<uniform> gain: Gain;
@group(0) @binding(1) var layer_picture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(layer_picture, vec2<i32>(position.xy), 0) * gain.factors;
}
