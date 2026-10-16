// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// A picture that was sent as bytes becomes the picture that the passes of a frame work on: made into the numbers of the working space (in light, in the primaries of the
// working space, in phase 3's linear light; in the compatibility mode the numbers as they are: the identity and the curve that is none), then premultiplied (each colour
// multiplied by how opaque the pixel is, which is how pictures are filtered and laid over each other correctly), in half-float numbers. **Premultiplied in light**, and
// never in codes. This is done once, when the picture is uploaded, and not for every frame. (The curves are in `colour.wgsl`, which is in front of this text.)

struct Ingest {
    // the light of the picture's primaries to the working space's
    to_working: mat3x3<f32>,
    // the number of the curve that the picture's codes are decoded with (`TransferFunction::number_for_the_shaders`)
    curve: u32,
}

@group(0) @binding(0) var<uniform> ingest: Ingest;
@group(0) @binding(1) var source: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let colour = textureLoad(source, vec2<i32>(position.xy), 0);
    let light = ingest.to_working * decode_to_light(ingest.curve, colour.rgb);
    return vec4<f32>(light * colour.a, colour.a);
}
