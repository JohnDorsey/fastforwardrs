// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shaders that come with the program: ISF files that were written for it (`assets/shaders`), kept **inside the program** (`include_str!`), so that they are there on every
//! computer it is installed on, need no download and cannot be lost or changed. They are generators (the fractals, the patterns) and filters (the effects that turn a picture into
//! something stranger); every one of them has inputs to change, and every input of each is a value like any other (it has a slider, the stopwatch and keyframes).
//!
//! They are **installed like the shaders that are found in folders** (see `scan`): each is read and compiled, and is listed in the plugin manager with what became of it, and the
//! ones that can be used are in the menus. A project that uses one has **the text of it in the project file**, as it has the text of any shader, so it runs on a computer that has
//! another version of the program. A shader that is changed in a later version is another kind of effect (its key has the digest of its text), and the projects that used the old one go on
//! running it.
//!
//! The licence of the files is CC0-1.0 (made for the program, nothing of anyone else's: see the SPDX lines at the top of each, and `docs/distribution-notices.md`).

use std::path::PathBuf;

use super::file::kind_of_a_shader;
use super::scan::{FoundShader, ShaderScan};

/// One of the shaders that come with the program.
#[derive(Clone, Copy, Debug)]
pub struct ShaderOfTheProgram {
    /// The folder of `assets/shaders` that the file is in (`fractals`).
    pub folder: &'static str,
    /// The name of the file, `Mandelbrot Explorer.fs`: what the name and the key of its kind are made from.
    pub file_name: &'static str,
    pub text: &'static str,
}

/// Where the files are, in the tree of the source (a test looks there to see that every file that is there is in [`SHADERS_OF_THE_PROGRAM`], and the other way round).
pub const FOLDER_OF_THE_FILES_IN_THE_SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders");

macro_rules! shader_of_the_program {
    ($folder:literal, $name:literal) => {
        ShaderOfTheProgram {
            folder: $folder,
            file_name: concat!($name, ".fs"),
            text: include_str!(concat!("../../assets/shaders/", $folder, "/", $name, ".fs")),
        }
    };
}

/// The shaders that come with the program: the fractals, then the other trippy ones (the generators, then the filters).
pub const SHADERS_OF_THE_PROGRAM: &[ShaderOfTheProgram] = &[
    shader_of_the_program!("fractals", "Mandelbrot Explorer"),
    shader_of_the_program!("fractals", "Julia Dance"),
    shader_of_the_program!("fractals", "Newton Basins"),
    shader_of_the_program!("fractals", "Apollonian Gasket"),
    shader_of_the_program!("fractals", "Mandelbulb"),
    shader_of_the_program!("fractals", "Folded Sponge"),
    shader_of_the_program!("fractals", "Mandelbox"),
    shader_of_the_program!("trippy", "Plasma Storm"),
    shader_of_the_program!("trippy", "Quasicrystal"),
    shader_of_the_program!("trippy", "Liquid Marble"),
    shader_of_the_program!("trippy", "Hyperbolic Tiling"),
    shader_of_the_program!("trippy", "Kaleidoscope Mandala"),
    shader_of_the_program!("trippy", "Droste Spiral"),
    shader_of_the_program!("trippy", "Tunnel Warp"),
    shader_of_the_program!("trippy", "Liquid Warp"),
    shader_of_the_program!("trippy", "Prism Split"),
    shader_of_the_program!("trippy", "Acid Palette"),
    shader_of_the_program!("trippy", "Neon Edges"),
    shader_of_the_program!("trippy", "Video Feedback"),
    shader_of_the_program!("trippy", "Ripple Rings"),
];

/// What a scan would make of the shaders of the program: each read and compiled like a shader that was found in a folder, and each said to be **built into the program** (it is in no file
/// that can be looked at on the computer).
pub fn scan_of_the_shaders_of_the_program() -> ShaderScan {
    let shaders = SHADERS_OF_THE_PROGRAM
        .iter()
        .map(|shader| {
            let found = |shape, kind, why_not_usable| FoundShader {
                path: PathBuf::from(shader.file_name),
                file_name: shader.file_name.to_owned(),
                shape,
                kind,
                why_not_usable,
                built_into_the_program: true,
            };
            match kind_of_a_shader(shader.file_name, shader.text, Vec::new()) {
                Ok(made) => found(Some(made.shape), Some(made.kind), made.why_not_usable),
                Err(why) => found(None, None, Some(why)),
            }
        })
        .collect();
    ShaderScan {
        shaders,
        problems: Vec::new(),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod world_tests;
