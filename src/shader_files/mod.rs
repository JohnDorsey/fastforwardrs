// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shader files (ISF) on the computer: reading one into the kind of effect it is, with the images that lie beside it, and finding the ones that are in folders (phase 6.8: the plugin
//! manager lists them and the menus offer them). A shader is **data**, and nothing here runs one: it is read, and compiled by the host of `isf` to see whether it can be run.

mod file;
mod scan;

pub use file::{
    KindOfAShader, ShaderFile, ShaderFileProblem, images_beside, kind_of_a_shader, read_a_shader_file, shader_file_from,
};
pub use scan::{
    FoundShader, MOST_DEPTH_OF_THE_SEARCH, MOST_FILES_LOOKED_AT, ShaderScan, begins_like_an_isf_file, scan_folders,
    scan_folders_within, usual_folders,
};
