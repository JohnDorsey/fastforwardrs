// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The effect `engine:colour_matrix`: the colours of a layer's picture are mixed with a 3 by 3 matrix, the same for every pixel, and cut off at 0 and 1; how
// clear each pixel is does not change. It is what `ColourMatrix::apply_to_rgba_pixels` does to the window's pictures, but in half-float numbers, with no
// 8-bit step in the middle. The picture is premultiplied (its colours are already multiplied by how opaque it is), so it is un-premultiplied for the mixing
// and premultiplied again after it. **It is an effect of the display-referred domain** (CM-10): the colours are mixed as the codes of a screen (sRGB codes, Rec.709
// primaries), which is what its matrices are made for, so the light of the working space is converted to them first and back after; in the compatibility mode the numbers
// already are such codes, the matrices are the identity and the curve is none. (The curves are in `colour.wgsl`, which is in front of this text.)
//
// Three numbers (`offsets`) are added to the mixed colour before it is cut off (brightness and contrast need them, phase 4); and `parts` can leave out, for a pass that
// is one of several in a row in the display-referred domain, the conversion in (bit 0), the conversion out (bit 1) or the mixing (bit 2: a pass that only converts, and
// does not cut off). **Zeros mean all three are done, as they were before the struct had these two**, which is what a call that is given to the engine as a foreign effect
// with the numbers of the old struct gets.

struct Mixing {
    // rows of the matrix: how much of the red, green and blue the new red (the first row), the new green and the new blue are made of
    to_new: mat3x3<f32>,
    // the light of the working space to the primaries of the display-referred domain, and back
    into_domain: mat3x3<f32>,
    out_of_domain: mat3x3<f32>,
    // the number of the curve of the domain (`TransferFunction::number_for_the_shaders`)
    curve: u32,
    // added to the mixed colour, in the codes of the domain, before it is cut off at 0 and 1
    offsets: vec3<f32>,
    // bit 0: the conversion into the domain is left out; bit 1: the conversion out of it is left out; bit 2: the mixing (and the cutting off) is left out
    parts: u32,
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
    var codes = colour;
    if (mixing.parts & 1u) == 0u {
        codes = encode_from_light(mixing.curve, mixing.into_domain * colour);
    }
    if (mixing.parts & 4u) == 0u {
        codes = clamp(mixing.to_new * codes + mixing.offsets, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    var light = codes;
    if (mixing.parts & 2u) == 0u {
        light = mixing.out_of_domain * decode_to_light(mixing.curve, codes);
    }
    return vec4<f32>(light * opacity, opacity);
}
