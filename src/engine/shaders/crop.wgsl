// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The effect `engine:crop`: what is outside the rectangle that is left becomes clear. The picture is premultiplied, so its four numbers are all multiplied by how much of
// the pixel is inside. Each edge of the rectangle is a smooth step across `soft_width` pixels, half of it inside the edge and half outside, so the pixels whose centres are
// a half pixel or more inside are all inside and those a half pixel or more outside are all outside: with a soft width of one pixel (the least), an edge on a boundary
// between two pixels is exact, and one that is not is a pixel that is partly clear. **It is an effect of the scene-linear domain** (CM-10): the numbers are the light of
// the working space, as they are.

struct Crop {
    // the rectangle that is left, in pixels of this picture: the x of its left edge, the y of its top edge, the x of its right edge and the y of its bottom edge
    edges: vec4<f32>,
    // how many pixels wide the step across each edge is (one or more)
    soft_width: f32,
}

@group(0) @binding(0) var<uniform> crop: Crop;
@group(0) @binding(1) var layer_picture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that is bigger than the picture and covers all of it.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let half_width = crop.soft_width * 0.5;
    // how far inside each edge the centre of the pixel is
    let inside = vec4<f32>(
        position.x - crop.edges.x,
        position.y - crop.edges.y,
        crop.edges.z - position.x,
        crop.edges.w - position.y,
    );
    let covered = smoothstep(vec4<f32>(-half_width), vec4<f32>(half_width), inside);
    let how_much = covered.x * covered.y * covered.z * covered.w;
    return textureLoad(layer_picture, vec2<i32>(position.xy), 0) * how_much;
}
