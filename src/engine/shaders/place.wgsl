// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// Puts a layer's picture in its place on the frame: a quadrilateral (a rectangle that may be turned) of two triangles, with the picture stretched over
// it, laid over what is there already (premultiplied, over black to begin with).
//
// The two triangles are the ones the window's own renderer draws for the same layer (corners 0 1 2 and 0 2 3, from the top left corner round to the
// bottom left one), and the positions are worked out with the same arithmetic, so that the same pixels are covered.

struct Placement {
    // Where the corners are, in pixels of the frame: top left (x, y) and top right (x, y) ...
    corners_top: vec4<f32>,
    // ... bottom right (x, y) and bottom left (x, y).
    corners_bottom: vec4<f32>,
    // What every pixel of the picture is multiplied by (premultiplied: a solid layer is a white pixel with its colour here).
    tint: vec4<f32>,
    frame_size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> placement: Placement;
@group(0) @binding(1) var layer_texture: texture_2d<f32>;
@group(0) @binding(2) var layer_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) texture_coordinate: vec2<f32>,
}

fn corner_in_pixels(corner: u32) -> vec2<f32> {
    switch corner {
        case 0u: {
            return placement.corners_top.xy;
        }
        case 1u: {
            return placement.corners_top.zw;
        }
        case 2u: {
            return placement.corners_bottom.xy;
        }
        default: {
            return placement.corners_bottom.zw;
        }
    }
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    var corner_of_each_vertex = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let corner = corner_of_each_vertex[index];
    let pixels = corner_in_pixels(corner);
    var out: VertexOutput;
    out.position = vec4<f32>(
        2.0 * pixels.x / placement.frame_size.x - 1.0,
        1.0 - 2.0 * pixels.y / placement.frame_size.y,
        0.0,
        1.0,
    );
    // The picture's own corners: top left (0, 0), top right (1, 0), bottom right (1, 1), bottom left (0, 1).
    out.texture_coordinate = vec2<f32>(f32(corner == 1u || corner == 2u), f32(corner >= 2u));
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(layer_texture, layer_sampler, in.texture_coordinate) * placement.tint;
}
