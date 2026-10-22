// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! One shader file: its text, the images that lie beside it, and the kind of effect it is.
//!
//! The rules are the program's one set of rules about what a shader file may be (phase 6.3 and 6.4.4, and now the scan of the plugin manager): a file of at most
//! [`MOST_BYTES_OF_A_SHADER`] that is text, that is read as ISF with the line and the column of what is wrong, and whose images (`IMPORTED`) are **files in the folder of the shader or
//! below it** (a path that is absolute, goes up, or says nothing is refused: a shader is not a way to read the files of a computer), each a PNG or a JPEG that the program can use, at most
//! [`MOST_IMAGES_A_SHADER_BRINGS`] of them.

use std::path::Path;

use crate::isf::{self, Shape};
use crate::project::{EffectKind, MOST_BYTES_OF_A_SHADER, MOST_IMAGES_A_SHADER_BRINGS, ShaderImage};
use crate::project_file::read_a_resource_file;

/// A shader file that was read.
#[derive(Clone, Debug)]
pub struct ShaderFile {
    /// The name of the file (`Random Shape.fs`), which the name and the key of its kind are made from.
    pub file_name: String,
    pub text: String,
    /// The images it brings, read from beside it, in the order its header lists them.
    pub images: Vec<ShaderImage>,
    pub kind: EffectKind,
    pub shape: Shape,
    /// What stops this version from running it, in words, if something does. (It is read all the same: it is a shader, and the project may keep it.)
    pub why_not_usable: Option<String>,
}

/// What was wrong with a file that was meant to be a shader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShaderFileProblem {
    /// The file could not be read (it is not there, it is not a file, it is too big to be read).
    CouldNotBeRead(String),
    /// It was read and is not a shader that this program can make an effect of. (What is said does not name the file: whoever shows it does.)
    NotAShader(String),
}

impl ShaderFileProblem {
    /// What is wrong, in words.
    pub fn words(&self) -> &str {
        match self {
            Self::CouldNotBeRead(words) | Self::NotAShader(words) => words,
        }
    }
}

/// What is made of the text of a shader (and of the images it brings): its kind, what it is for, and why it cannot be run, if it cannot.
#[derive(Clone, Debug)]
pub struct KindOfAShader {
    pub kind: EffectKind,
    pub shape: Shape,
    pub why_not_usable: Option<String>,
}

/// The images that the shader `text`, the file `path`, brings with it (an ISF file's `IMPORTED`), read from beside it. What is wrong, in words, with the first that is wrong, if anything is.
/// (A file that cannot be read as ISF has no images to read: it says what is wrong with it when it is made into a kind.)
pub fn images_beside(path: &Path, text: &str) -> Result<Vec<ShaderImage>, String> {
    let Ok(file) = isf::read(text) else {
        return Ok(Vec::new());
    };
    if file.imported.len() > MOST_IMAGES_A_SHADER_BRINGS {
        return Err(format!(
            "the shader brings {} images, and {MOST_IMAGES_A_SHADER_BRINGS} are the most",
            file.imported.len()
        ));
    }
    let folder = path.parent().unwrap_or_else(|| Path::new("."));
    file.imported
        .iter()
        .map(|imported| {
            let relative = Path::new(&imported.path);
            // (The parts of the path are all names or the current folder: a part that is a drive, the root of a disk or `..` is one that leaves the folder, and there is no need to ask
            // whether the path is absolute besides: every absolute path has such a part.)
            let stays_below = !imported.path.trim().is_empty()
                && relative
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_) | std::path::Component::CurDir));
            if !stays_below {
                return Err(format!(
                    "the image {} is at {:?}, which is not a file in the folder of the shader or below it",
                    imported.name, imported.path
                ));
            }
            let bytes = read_a_resource_file(&folder.join(relative))
                .map_err(|problem| format!("the image {} ({}): {problem}", imported.name, imported.path))?;
            let image = ShaderImage::new(imported.name.clone(), bytes);
            image
                .pixels()
                .map_err(|why| format!("the image {} ({}): {why}", imported.name, imported.path))?;
            Ok(image)
        })
        .collect()
}

/// The kind of effect that the shader `text` of the file `file_name`, which brings `images`, is, with what it is for and whether this version runs it. **What is wrong with it, in words, if
/// it is no shader at all** (too long, no ISF header, a header that is wrong, a description that cannot be used).
pub fn kind_of_a_shader(file_name: &str, text: &str, images: Vec<ShaderImage>) -> Result<KindOfAShader, String> {
    if text.len() > MOST_BYTES_OF_A_SHADER {
        return Err(format!(
            "the file is {} bytes, and {MOST_BYTES_OF_A_SHADER} are the most a shader can be",
            text.len()
        ));
    }
    let file = isf::read(text).map_err(|problem| problem.to_string())?;
    let descriptor = isf::descriptor_with_images_of(&file, file_name, text, images);
    let kind = EffectKind::from_snapshot(descriptor)?;
    Ok(KindOfAShader {
        kind,
        shape: file.shape(),
        why_not_usable: isf::why_not_usable(kind.descriptor()),
    })
}

/// The shader whose text is `text`, which is in the file `path` (the images that lie beside it are read now).
pub fn shader_file_from(path: &Path, text: String) -> Result<ShaderFile, ShaderFileProblem> {
    let file_name = path
        .file_name()
        .map_or_else(|| "shader.fs".to_owned(), |name| name.to_string_lossy().into_owned());
    let images = images_beside(path, &text).map_err(ShaderFileProblem::NotAShader)?;
    let made = kind_of_a_shader(&file_name, &text, images.clone()).map_err(ShaderFileProblem::NotAShader)?;
    Ok(ShaderFile {
        file_name,
        text,
        images,
        kind: made.kind,
        shape: made.shape,
        why_not_usable: made.why_not_usable,
    })
}

/// Reads the file at `path` as a shader.
pub fn read_a_shader_file(path: &Path) -> Result<ShaderFile, ShaderFileProblem> {
    let bytes = read_a_resource_file(path).map_err(|problem| ShaderFileProblem::CouldNotBeRead(problem.to_string()))?;
    let Ok(text) = String::from_utf8(bytes.to_vec()) else {
        return Err(ShaderFileProblem::NotAShader("the file is not text".to_owned()));
    };
    shader_file_from(path, text)
}

#[cfg(test)]
mod tests;
