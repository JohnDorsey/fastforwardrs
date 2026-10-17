// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The effect `engine:lut`: the colours of a layer's picture go through a look-up table (a `.cube` file). The picture is premultiplied, so it is un-premultiplied for the
// look-up and premultiplied again after it.
//
// **The spaces** (CM-11). The colours go into the table in the space the call says and come out of it in the space the call says: `to_input` and `from_output` are the matrices
// of the primaries (the working space to the table's, and back) and the curves are numbers (see `colour.wgsl`, which is in front of this text); a space that is linear light has
// the identity for a matrix and the linear curve. **The conversion into the table's space is left out, or the one out of it, when `parts` says so**: that is for a table of the
// codes of a screen when the colour is in them already, because the effect before it or after it works in them (the engine converts once between neighbours of one domain).
// The colour that comes out of a table of codes is cut off at 0 and 1 (as the engine's other effects of codes are); the colour that comes out of a table of linear light is not
// (`clamp_output` says which).
//
// **The look-up.** A 3-D table is a cube of `size` points a side, in a 3-D texture, red changing fastest as in the file; a 1-D table is a strip of `size` points in a texture
// 256 wide (the point `n` at the column `n % 256` of the row `n / 256`). The input is mapped from the table's domain to the cube (cut off at its ends), and looked up
// between the points by the tetrahedron the colour is in (the four corners that the order of the three fractions picks) or by all eight corners, or, for a 1-D table, by
// linear interpolation in each channel's own column. The points are read as they are (`textureLoad`), so what comes out does not depend on how a card filters.
// `look_up` in `cube_table.rs` is the same arithmetic on the processor, which this is held to, and both to ffmpeg's `lut3d`.
//
// **Intensity** mixes the colour that went in and the colour that came out, as light (a call that wants less than all of the table is one of the scene-linear domain, so
// the colours are light on both sides).

struct Look {
    // the light of the working space to the primaries of the space the colours go into the table in, and the primaries of the space they come out in to the working space
    to_input: mat3x3<f32>,
    from_output: mat3x3<f32>,
    // the domain of the table
    domain_min: vec3<f32>,
    // how many points a side a 3-D table has, or how many points a 1-D table has
    size: u32,
    domain_max: vec3<f32>,
    // how much of the colour that comes out of the table is in the result
    intensity: f32,
    // the curves (numbers: see `colour.wgsl`) of the space going in and of the space coming out
    input_curve: u32,
    output_curve: u32,
    // bit 0: the conversion into the table's space is left out; bit 1: the conversion out of it is left out
    parts: u32,
    // 0: tetrahedral, 1: trilinear (of a 3-D table)
    interpolation: u32,
    // 1 for a 1-D table, 0 for a 3-D table
    one_dimensional: u32,
    // 1 if the colour that comes out of the table is cut off at 0 and 1 (it is codes), 0 if it is not (it is linear light)
    clamp_output: u32,
}

@group(0) @binding(0) var<uniform> look: Look;
@group(0) @binding(1) var layer_picture: texture_2d<f32>;
@group(0) @binding(2) var table_3d: texture_3d<f32>;
@group(0) @binding(3) var table_1d: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

fn row_3d(red: u32, green: u32, blue: u32) -> vec3<f32> {
    return textureLoad(table_3d, vec3<i32>(i32(red), i32(green), i32(blue)), 0).rgb;
}

fn row_1d(point: u32) -> vec3<f32> {
    return textureLoad(table_1d, vec2<i32>(i32(point % 256u), i32(point / 256u)), 0).rgb;
}

// The point of the table that a colour is at (from 0 to size - 1 in each channel), after it is mapped from the domain.
fn place_in_the_table(colour: vec3<f32>) -> vec3<f32> {
    let inside = clamp(colour, look.domain_min, look.domain_max);
    return (inside - look.domain_min) / (look.domain_max - look.domain_min) * f32(look.size - 1u);
}

fn look_up_1d(colour: vec3<f32>) -> vec3<f32> {
    let at = place_in_the_table(colour);
    let last = look.size - 1u;
    let below = min(vec3<u32>(floor(at)), vec3<u32>(last));
    let above = min(below + vec3<u32>(1u), vec3<u32>(last));
    let fraction = at - vec3<f32>(below);
    return vec3<f32>(
        mix(row_1d(below.r).r, row_1d(above.r).r, fraction.r),
        mix(row_1d(below.g).g, row_1d(above.g).g, fraction.g),
        mix(row_1d(below.b).b, row_1d(above.b).b, fraction.b),
    );
}

fn look_up_trilinear(below: vec3<u32>, above: vec3<u32>, d: vec3<f32>) -> vec3<f32> {
    let c00 = mix(row_3d(below.r, below.g, below.b), row_3d(above.r, below.g, below.b), d.r);
    let c10 = mix(row_3d(below.r, above.g, below.b), row_3d(above.r, above.g, below.b), d.r);
    let c01 = mix(row_3d(below.r, below.g, above.b), row_3d(above.r, below.g, above.b), d.r);
    let c11 = mix(row_3d(below.r, above.g, above.b), row_3d(above.r, above.g, above.b), d.r);
    return mix(mix(c00, c10, d.g), mix(c01, c11, d.g), d.b);
}

// The colour between the corners of the little cube by the four corners of the tetrahedron it is in: the first and the last corner of the walk from the corner `below` to the
// corner `above` are always there, and the two between are the ones that the largest and then the second largest fraction step to.
fn look_up_tetrahedral(below: vec3<u32>, above: vec3<u32>, d: vec3<f32>) -> vec3<f32> {
    let c000 = row_3d(below.r, below.g, below.b);
    let c111 = row_3d(above.r, above.g, above.b);
    var first: vec3<f32>;
    var second: vec3<f32>;
    var weights: vec4<f32>;
    if d.r > d.g {
        if d.g > d.b {
            first = row_3d(above.r, below.g, below.b);
            second = row_3d(above.r, above.g, below.b);
            weights = vec4<f32>(1.0 - d.r, d.r - d.g, d.g - d.b, d.b);
        } else if d.r > d.b {
            first = row_3d(above.r, below.g, below.b);
            second = row_3d(above.r, below.g, above.b);
            weights = vec4<f32>(1.0 - d.r, d.r - d.b, d.b - d.g, d.g);
        } else {
            first = row_3d(below.r, below.g, above.b);
            second = row_3d(above.r, below.g, above.b);
            weights = vec4<f32>(1.0 - d.b, d.b - d.r, d.r - d.g, d.g);
        }
    } else if d.b > d.g {
        first = row_3d(below.r, below.g, above.b);
        second = row_3d(below.r, above.g, above.b);
        weights = vec4<f32>(1.0 - d.b, d.b - d.g, d.g - d.r, d.r);
    } else if d.b > d.r {
        first = row_3d(below.r, above.g, below.b);
        second = row_3d(below.r, above.g, above.b);
        weights = vec4<f32>(1.0 - d.g, d.g - d.b, d.b - d.r, d.r);
    } else {
        first = row_3d(below.r, above.g, below.b);
        second = row_3d(above.r, above.g, below.b);
        weights = vec4<f32>(1.0 - d.g, d.g - d.r, d.r - d.b, d.b);
    }
    return weights.x * c000 + weights.y * first + weights.z * second + weights.w * c111;
}

fn look_up_3d(colour: vec3<f32>) -> vec3<f32> {
    let at = place_in_the_table(colour);
    let last = look.size - 1u;
    let below = min(vec3<u32>(floor(at)), vec3<u32>(last));
    let above = min(below + vec3<u32>(1u), vec3<u32>(last));
    let d = at - vec3<f32>(below);
    if look.interpolation == 1u {
        return look_up_trilinear(below, above, d);
    }
    return look_up_tetrahedral(below, above, d);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let premultiplied = textureLoad(layer_picture, vec2<i32>(position.xy), 0);
    let opacity = premultiplied.a;
    // (A pixel that is all clear has no colour to look up, and stays all clear.)
    var colour = vec3<f32>(0.0);
    if opacity > 0.0 {
        colour = premultiplied.rgb / opacity;
    }
    var going_in = colour;
    if (look.parts & 1u) == 0u {
        going_in = encode_from_light(look.input_curve, look.to_input * colour);
    }
    var coming_out: vec3<f32>;
    if look.one_dimensional == 1u {
        coming_out = look_up_1d(going_in);
    } else {
        coming_out = look_up_3d(going_in);
    }
    var codes = coming_out;
    if look.clamp_output == 1u {
        codes = clamp(codes, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    var result = codes;
    if (look.parts & 2u) == 0u {
        if look.output_curve == CURVE_LINEAR {
            // (Linear light comes out as it is: it is not cut off at 1, which `decode_to_light` would do.)
            result = look.from_output * codes;
        } else {
            result = look.from_output * decode_to_light(look.output_curve, codes);
        }
    }
    let mixed = mix(colour, result, look.intensity);
    return vec4<f32>(mixed * opacity, opacity);
}
