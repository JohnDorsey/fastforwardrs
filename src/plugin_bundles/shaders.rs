// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shaders that the program can fetch for a person who asks (phase 6.9; the *Bundles* tab of the plugin manager): a manifest kept in the program, with the same licence policy as the
//! one for plugins, and the download of **each file at a fixed version of its author's repository**, checked against its checksum before it is used, with the licence texts and a
//! `NOTICE.txt` beside them (`bundled-plugins/shaders.json`; the record of what was left out and why is `docs/bundleable-isf-shaders.md`).
//!
//! A shader is data, so there is no trust to ask for, and nothing is unpacked: the files are put in a folder of their own for each set (`fastforwardrs/shaders/<set>` in the data folder of
//! the person), where the plugin manager looks for shaders, and the files that were fetched are those that the manifest names, each of the bytes whose checksum it holds.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::download::{CANCELLED, Fetcher, Progress, check_the_checksum};
use super::manifest::{
    LicenceText, Source, is_a_plain_file_name, is_a_plain_id, is_a_sha256, is_https, licence_is_allowed,
};
use crate::project::MOST_BYTES_OF_A_SHADER;

/// The manifest of shaders as it was when the program was built.
pub const EMBEDDED_SHADER_MANIFEST: &str = include_str!("../../bundled-plugins/shaders.json");

/// What a shader is for, as the manifest says it.
const SHAPES: [&str; 3] = ["filter", "generator", "transition"];

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ShaderManifest {
    pub format: u32,
    pub sets: Vec<ShaderSet>,
}

/// One set of shaders from one repository, at one version: one folder when it is fetched.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ShaderSet {
    pub id: String,
    pub name: String,
    /// What the version is called (the first characters of the commit).
    pub version: String,
    pub authors: Vec<String>,
    /// An SPDX expression: the licences of the files, joined by `AND`.
    pub licence: String,
    #[serde(default)]
    pub licence_note: Option<String>,
    pub homepage: String,
    pub licence_texts: Vec<LicenceText>,
    pub source: Source,
    pub shaders: Vec<ShaderEntry>,
}

/// One shader file.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ShaderEntry {
    /// What a person calls it.
    pub name: String,
    /// The name it is given in the folder of its set.
    pub file: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// Its own licence (the one its own text says, or the repository's): an SPDX name.
    pub licence: String,
    /// `filter`, `generator` or `transition`.
    pub shape: String,
}

impl ShaderManifest {
    /// The manifest that is kept in the program.
    ///
    /// # Panics
    /// If it cannot be read (a test says that it can).
    pub fn embedded() -> Self {
        serde_json::from_str(EMBEDDED_SHADER_MANIFEST).expect("the manifest of shaders in the program is read")
    }

    /// What is wrong with the manifest, one line each (nothing if it is fit).
    pub fn problems(&self) -> Vec<String> {
        if self.format != 1 {
            return vec![format!(
                "the format of the manifest of shaders is {}, and this program reads 1",
                self.format
            )];
        }
        if self.sets.is_empty() {
            return vec!["the manifest of shaders has no sets".to_owned()];
        }
        let mut problems = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for set in &self.sets {
            let name = &set.id;
            if !is_a_plain_id(&set.id) {
                problems.push(format!(
                    "set {name}: the id is not lower case letters, digits and dashes"
                ));
            }
            if seen.contains(&set.id.as_str()) {
                problems.push(format!("set {name}: the id is used twice"));
            }
            seen.push(&set.id);
            if set.name.is_empty() || set.version.is_empty() || set.authors.is_empty() || set.shaders.is_empty() {
                problems.push(format!("set {name}: a name, a version, authors and shaders are needed"));
            }
            if !is_https(&set.homepage) {
                problems.push(format!("set {name}: the homepage is not an https address"));
            }
            if !licence_is_allowed(&set.licence) {
                problems.push(format!(
                    "set {name}: the licence {:?} is not one that may ship",
                    set.licence
                ));
            }
            if set.licence_texts.is_empty() {
                problems.push(format!("set {name}: no licence text"));
            }
            for text in &set.licence_texts {
                if !(is_https(&text.url) && is_a_sha256(&text.sha256) && is_a_plain_file_name(&text.name)) {
                    problems.push(format!(
                        "set {name}: a licence text needs a plain file name, an https address and a sha256"
                    ));
                }
                if text.name == "NOTICE.txt" || set.shaders.iter().any(|shader| shader.file == text.name) {
                    problems.push(format!(
                        "set {name}: the licence text {} has the name of a file that is made or fetched with it",
                        text.name
                    ));
                }
            }
            if set.licence_texts.iter().enumerate().any(|(number, text)| {
                set.licence_texts[..number]
                    .iter()
                    .any(|earlier| earlier.name == text.name)
            }) {
                problems.push(format!("set {name}: a licence text name is used twice"));
            }
            if !is_https(&set.source.url) {
                problems.push(format!("set {name}: the source has no https address"));
            }
            let licences_of_the_set: Vec<&str> = set.licence.split(" AND ").map(str::trim).collect();
            let mut files: Vec<&str> = Vec::new();
            for shader in &set.shaders {
                let there = format!("set {name}, {}", shader.file);
                if shader.name.is_empty() {
                    problems.push(format!("{there}: no name"));
                }
                let ending_is_right = {
                    let lower = shader.file.to_ascii_lowercase();
                    lower.ends_with(".fs") || lower.ends_with(".isf")
                };
                if !(is_a_plain_file_name(&shader.file) && ending_is_right) {
                    problems.push(format!("{there}: not a plain name of a .fs or .isf file"));
                }
                if files.contains(&shader.file.as_str()) {
                    problems.push(format!("{there}: the name is used twice"));
                }
                files.push(&shader.file);
                if !is_https(&shader.url) || !is_a_sha256(&shader.sha256) {
                    problems.push(format!("{there}: needs an https address and a sha256"));
                }
                if shader.size == 0 || shader.size > MOST_BYTES_OF_A_SHADER as u64 {
                    problems.push(format!(
                        "{there}: {} bytes is not a size that a shader can be (1 to {MOST_BYTES_OF_A_SHADER})",
                        shader.size
                    ));
                }
                if !licence_is_allowed(&shader.licence) {
                    problems.push(format!(
                        "{there}: the licence {:?} is not one that may ship",
                        shader.licence
                    ));
                } else if !licences_of_the_set.contains(&shader.licence.as_str()) {
                    problems.push(format!(
                        "{there}: the licence {:?} is not among those the set says it has ({})",
                        shader.licence, set.licence
                    ));
                }
                if !SHAPES.contains(&shader.shape.as_str()) {
                    problems.push(format!(
                        "{there}: {:?} is not a filter, a generator or a transition",
                        shader.shape
                    ));
                }
            }
        }
        problems
    }
}

/// The folder of a set in `folder` (the folder that shaders are fetched into).
pub fn folder_of_a_shader_set(set: &ShaderSet, folder: &Path) -> PathBuf {
    folder.join(&set.id)
}

/// Whether every shader of `set` is in its folder in `folder`.
pub fn shaders_are_fetched(set: &ShaderSet, folder: &Path) -> bool {
    let place = folder_of_a_shader_set(set, folder);
    set.shaders.iter().all(|shader| place.join(&shader.file).is_file())
}

/// How many bytes would be downloaded for all the sets: the shaders (the licence texts are a few kilobytes more, and are not counted).
pub fn shader_bytes_to_download(manifest: &ShaderManifest) -> u64 {
    manifest
        .sets
        .iter()
        .flat_map(|set| &set.shaders)
        .map(|shader| shader.size)
        .sum()
}

/// What a download of shaders made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderOutcome {
    /// The names of the sets that were fetched.
    pub sets: Vec<String>,
    /// How many shader files were put in place.
    pub files: usize,
}

/// What `NOTICE.txt` in the folder of a set says.
pub fn notice_of_a_shader_set(set: &ShaderSet) -> String {
    let mut lines = vec![
        format!("{} {}", set.name, set.version),
        format!("By {}.", set.authors.join(", ")),
        format!(
            "Licence: {}{}",
            set.licence,
            set.licence_note.as_ref().map_or_else(String::new, |note| format!(" ({note})"))
        ),
        format!("Home page: {}", set.homepage),
        String::new(),
        "These files are the shaders as their authors released them, unaltered, fetched by fastforwardrs from the address of the version named above, and they are used under their own licence: each file says whose it is and under which licence in its own header, and the texts of the licences are in this folder.".to_owned(),
        String::new(),
        format!("The source of this version is at: {}", set.source.url),
    ];
    if let Some(note) = &set.source.note {
        lines.push(note.clone());
    }
    lines.join("\n") + "\n"
}

/// Fetches the shaders of every set of `manifest` into `folder`, one folder for each set, each file checked against its checksum **before** it is used.
///
/// # Errors
/// What went wrong, in words for a person: the manifest is not fit, an address that could not be downloaded, a file whose checksum is not the one the manifest says, a folder that cannot
/// be written, or that the download was cancelled ([`CANCELLED`]). **A set that was in `folder` before is left as it was if its new version could not be made.**
pub fn fetch_shaders(
    manifest: &ShaderManifest,
    folder: &Path,
    fetcher: &dyn Fetcher,
    report: &mut dyn FnMut(&Progress),
    cancelled: &dyn Fn() -> bool,
) -> Result<ShaderOutcome, String> {
    let problems = manifest.problems();
    if !problems.is_empty() {
        return Err(format!(
            "the list of shaders is not fit to be used: {}",
            problems.join("; ")
        ));
    }
    let temporary = folder.join(".downloading");
    fs::create_dir_all(&temporary).map_err(|error| format!("{} could not be made: {error}", temporary.display()))?;
    let result = fetch_the_sets(manifest, folder, &temporary, fetcher, report, cancelled);
    let _ = fs::remove_dir_all(&temporary);
    result
}

fn fetch_the_sets(
    manifest: &ShaderManifest,
    folder: &Path,
    temporary: &Path,
    fetcher: &dyn Fetcher,
    report: &mut dyn FnMut(&Progress),
    cancelled: &dyn Fn() -> bool,
) -> Result<ShaderOutcome, String> {
    let total = shader_bytes_to_download(manifest);
    let mut finished = 0_u64;
    let mut outcome = ShaderOutcome {
        sets: Vec::new(),
        files: 0,
    };
    for (number, set) in manifest.sets.iter().enumerate() {
        if cancelled() {
            return Err(CANCELLED.to_owned());
        }
        let step = format!("{} ({} of {})", set.name, number + 1, manifest.sets.len());
        let building = folder.join(format!("{}.building", set.id));
        let _ = fs::remove_dir_all(&building);
        let made = build_the_set(
            set,
            &building,
            temporary,
            fetcher,
            &step,
            (finished, total),
            report,
            cancelled,
        );
        if let Err(error) = made {
            let _ = fs::remove_dir_all(&building);
            return Err(error);
        }
        let place = folder_of_a_shader_set(set, folder);
        let _ = fs::remove_dir_all(&place);
        fs::rename(&building, &place)
            .map_err(|error| format!("{} could not be put in place: {error}", place.display()))?;
        finished += set.shaders.iter().map(|shader| shader.size).sum::<u64>();
        outcome.sets.push(set.name.clone());
        outcome.files += set.shaders.len();
    }
    report(&Progress {
        step: "Done".to_owned(),
        bytes_done: total,
        bytes_total: total,
    });
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
fn build_the_set(
    set: &ShaderSet,
    building: &Path,
    temporary: &Path,
    fetcher: &dyn Fetcher,
    step: &str,
    (finished, total): (u64, u64),
    report: &mut dyn FnMut(&Progress),
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    fs::create_dir_all(building).map_err(|error| format!("{} could not be made: {error}", building.display()))?;
    let downloads = temporary.join(&set.id);
    fs::create_dir_all(&downloads).map_err(|error| format!("{} could not be made: {error}", downloads.display()))?;
    // (Every file gets a name of its own here that is made of a number, so that nothing a manifest names can climb out of the folder.)
    let shader_files: Vec<PathBuf> = (0..set.shaders.len())
        .map(|number| downloads.join(format!("shader-{number}")))
        .collect();
    let text_files: Vec<PathBuf> = (0..set.licence_texts.len())
        .map(|number| downloads.join(format!("licence-{number}")))
        .collect();
    let mut items: Vec<(&str, &Path)> = Vec::new();
    for (shader, file) in set.shaders.iter().zip(&shader_files) {
        items.push((shader.url.as_str(), file.as_path()));
    }
    for (text, file) in set.licence_texts.iter().zip(&text_files) {
        items.push((text.url.as_str(), file.as_path()));
    }
    report(&Progress {
        step: format!("Downloading {step}"),
        bytes_done: finished,
        bytes_total: total,
    });
    // (The progress is counted in the bytes of the shaders, which the manifest has; the few of a licence text, that come after them, are not counted, so that it never goes back.)
    let bytes_of_the_set: u64 = set.shaders.iter().map(|shader| shader.size).sum();
    fetcher.fetch_many(
        &items,
        &mut |bytes| {
            report(&Progress {
                step: format!("Downloading {step}"),
                bytes_done: finished + bytes.min(bytes_of_the_set),
                bytes_total: total,
            });
        },
        cancelled,
    )?;
    report(&Progress {
        step: format!("Checking {step}"),
        bytes_done: finished + bytes_of_the_set,
        bytes_total: total,
    });
    for (shader, file) in set.shaders.iter().zip(&shader_files) {
        check_the_checksum(file, &shader.sha256, &shader.url)?;
        fs::copy(file, building.join(&shader.file))
            .map_err(|error| format!("the shader {} could not be written: {error}", shader.file))?;
    }
    for (text, file) in set.licence_texts.iter().zip(&text_files) {
        check_the_checksum(file, &text.sha256, &text.url)?;
        fs::copy(file, building.join(&text.name))
            .map_err(|error| format!("the licence text {} could not be written: {error}", text.name))?;
    }
    fs::write(building.join("NOTICE.txt"), notice_of_a_shader_set(set))
        .map_err(|error| format!("NOTICE.txt could not be written: {error}"))
}

#[cfg(test)]
mod tests;
