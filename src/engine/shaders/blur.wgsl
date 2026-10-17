// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The effect `engine:blur`: one of its two passes, a Gaussian blur along one line (the other pass is along the other). The picture is premultiplied, so averaging its four
// numbers is the right blur of a picture that is see-through here and there. The pixel at the edge is used for what is beyond it (a picture does not fade to nothing at its
// edge). **It is an effect of the scene-linear domain** (CM-10): the numbers are the light of the working space, as they are, which is what a blur of light is.
//
// The weights are `exp(-offset^2 / (2 sigma^2))` for the offsets from `-half_width` to `half_width`, divided by their sum (so they add up to one, and the cut-off of the
// kernel at `half_width` does not make the picture darker).

struct Blur {
    // one step along the line: (1, 0) for the pass along the rows and (0, 1) for the pass along the columns
    step: vec2<i32>,
    // the standard deviation of the Gaussian, in pixels of this picture
    sigma: f32,
    // how many pixels each side of the pixel are averaged
    half_width: i32,
}

@group(0) @binding(0) var<uniform> blur: Blur;
@group(0) @binding(1) var layer_picture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let last = vec2<i32>(textureDimensions(layer_picture)) - vec2<i32>(1);
    let here = vec2<i32>(position.xy);
    let spread = -0.5 / (blur.sigma * blur.sigma);
    var sum = vec4<f32>(0.0);
    var weights = 0.0;
    for (var offset = -blur.half_width; offset <= blur.half_width; offset++) {
        let weight = exp(spread * f32(offset * offset));
        let at = clamp(here + blur.step * offset, vec2<i32>(0), last);
        sum += textureLoad(layer_picture, at, 0) * weight;
        weights += weight;
    }
    return sum / weights;
}
