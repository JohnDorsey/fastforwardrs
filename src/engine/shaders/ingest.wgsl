// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// A picture that was sent as bytes becomes the picture that the passes of a frame work on: premultiplied (each colour multiplied by how opaque the
// pixel is, which is how pictures are filtered and laid over each other correctly), in half-float numbers. This is done once, when the picture is
// uploaded, and not for every frame.

@group(0) @binding(0) var source: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let colour = textureLoad(source, vec2<i32>(position.xy), 0);
    return vec4<f32>(colour.rgb * colour.a, colour.a);
}
