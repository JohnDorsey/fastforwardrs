// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The vertex stage of the effects that others wrote (their shaders only have a fragment stage): one triangle that is bigger than the picture it is drawn on
// and covers all of it, so that the fragment stage is run once for every pixel of the picture. It is the same triangle as the one the engine's own
// effects draw.

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
