// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The host of ISF (Interactive Shader Format) shaders: **how the program gets generators, transitions and most new effects without growing** (see
//! `docs/engine-phase-6-design.md`). This module has no graphics in it: it reads an ISF file ([`read`]), says what it asks for and is (its inputs, its passes, whether it is a filter, a
//! generator or a transition), and turns its GLSL into a WGSL module that the picture engine can run. Whatever is wrong with a file is told as an [`IsfProblem`], with the line and
//! the column of the file.

mod compile;
mod descriptor;
mod reader;
mod registry;
mod rewrite;

pub use compile::{CompiledShader, ENTRY_POINT, compile, compile_in_a_worker};

pub use descriptor::{descriptor_of, key_is_of_the_text, key_of, numbers_of_the_values};
pub use registry::{compiled_shader_of, is_usable, why_not_usable};

pub use reader::{
    IsfFile, IsfImportedImage, IsfInput, IsfInputKind, IsfPass, IsfProblem, MOST_NUMBERS_OF_THE_INPUTS, Shape, read,
};
pub use rewrite::{MOST_NESTING, NUMBERS_BEFORE_THE_INPUTS, VEC4S_OF_THE_UNIFORM_BLOCK};
