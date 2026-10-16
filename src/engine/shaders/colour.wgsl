// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// The curves between codes and light (CM-08), the same arithmetic as `colour_space::TransferFunction` in Rust, which the tests hold this to. This file has no entry points: it is
// put in front of the passes that need it (`shaders::with_colour`), and which curve is used is a number in the pass's uniforms, so that one shader serves every curve and the
// number of pipelines does not grow with the number of curves. The numbers are the ones of `TransferFunction::number_for_the_shaders`.
//
// Every curve is applied to a code that is cut off at 0 and 1, and to a light that is cut off at 0 below, as the Rust version is.

const CURVE_SRGB: u32 = 0u;
const CURVE_GAMMA_22: u32 = 1u;
const CURVE_GAMMA_24: u32 = 2u;
const CURVE_GAMMA_26: u32 = 3u;
const CURVE_REC709: u32 = 4u;
const CURVE_LINEAR: u32 = 5u;

// The constants of the camera curve of BT.709 that BT.2020 gives (the rounded ones of BT.709 do not make its two pieces meet).
const REC709_ALPHA: f32 = 1.09929682680944;
const REC709_BETA: f32 = 0.018053968510807;

// The light that codes stand for.
fn decode_to_light(curve: u32, codes: vec3<f32>) -> vec3<f32> {
    let code = clamp(codes, vec3<f32>(0.0), vec3<f32>(1.0));
    switch curve {
        case 0u: {
            let straight = code / 12.92;
            let powered = pow((code + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
            return select(powered, straight, code <= vec3<f32>(0.04045));
        }
        case 1u: {
            return pow(code, vec3<f32>(2.2));
        }
        case 2u: {
            return pow(code, vec3<f32>(2.4));
        }
        case 3u: {
            return pow(code, vec3<f32>(2.6));
        }
        case 4u: {
            let straight = code / 4.5;
            let powered = pow((code + vec3<f32>(REC709_ALPHA - 1.0)) / REC709_ALPHA, vec3<f32>(1.0 / 0.45));
            return select(powered, straight, code < vec3<f32>(4.5 * REC709_BETA));
        }
        default: {
            return code;
        }
    }
}

// The codes that stand for light. A light below 0 is the code 0; a light above 1 gives a code above 1, for whoever makes the picture to cut off.
fn encode_from_light(curve: u32, lights: vec3<f32>) -> vec3<f32> {
    let light = max(lights, vec3<f32>(0.0));
    switch curve {
        case 0u: {
            let straight = 12.92 * light;
            let powered = 1.055 * pow(light, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
            return select(powered, straight, light <= vec3<f32>(0.0031308));
        }
        case 1u: {
            return pow(light, vec3<f32>(1.0 / 2.2));
        }
        case 2u: {
            return pow(light, vec3<f32>(1.0 / 2.4));
        }
        case 3u: {
            return pow(light, vec3<f32>(1.0 / 2.6));
        }
        case 4u: {
            let straight = 4.5 * light;
            let powered = REC709_ALPHA * pow(light, vec3<f32>(0.45)) - vec3<f32>(REC709_ALPHA - 1.0);
            return select(powered, straight, light < vec3<f32>(REC709_BETA));
        }
        default: {
            return light;
        }
    }
}
