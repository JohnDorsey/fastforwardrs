// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The last pass of a frame: the picture that was put together in half-float precision becomes the 8-bit picture that is given back. **This is where the display
// transform is**: the light of the working space is converted to the primaries of the output, encoded with its curve, and cut off at 0 and 1; in the compatibility mode the
// numbers already are what the screen is shown, so the matrix is the identity and the curve is none, and the pass only cuts off. (The curves are in `colour.wgsl`, which is
// in front of this text.)

struct Output {
    // the light of the working space to the output's primaries
    to_output: mat3x3<f32>,
    // the number of the curve that the output's codes are made with (`TransferFunction::number_for_the_shaders`)
    curve: u32,
}

@group(0) @binding(0) var<uniform> conversion: Output;
@group(0) @binding(1) var accumulation: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let colour = textureLoad(accumulation, vec2<i32>(position.xy), 0);
    let codes = encode_from_light(conversion.curve, conversion.to_output * colour.rgb);
    return vec4<f32>(clamp(codes, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
