// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the plugins are looked for (G16 of the design): the folders that a person added, the folders of `LADSPA_PATH`, and the usual ones of the system, in that order. A
//! plugin that is in more than one is the one of the first (what makes the key of a plugin is where the registry of piece 7.5 comes in; the order is here).

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The endings that the files of libraries have on this system, in lower case. (The LADSPA libraries of a Mac are made as `.so` as often as `.dylib`.)
pub fn library_file_extensions() -> &'static [&'static str] {
    if cfg!(windows) {
        &["dll"]
    } else if cfg!(target_os = "macos") {
        &["dylib", "so"]
    } else {
        &["so"]
    }
}

/// The ending a library file made for this system has (the one that its own build gets).
pub fn library_file_extension() -> &'static str {
    library_file_extensions()[0]
}

/// The folders that plugins are installed in on this system if nobody says otherwise: those of `ladspa.h` (`/usr/lib/ladspa` and `/usr/local/lib/ladspa`) and `~/.ladspa`, on
/// the systems that have them. **Windows has none**: nothing installs LADSPA plugins in a place that every program looks in, and a person adds the folder of the program that
/// has them.
pub fn usual_folders(home: Option<&Path>) -> Vec<PathBuf> {
    if cfg!(windows) {
        return Vec::new();
    }
    let mut folders = vec![PathBuf::from("/usr/lib/ladspa"), PathBuf::from("/usr/local/lib/ladspa")];
    if let Some(home) = home {
        folders.push(home.join(".ladspa"));
    }
    folders
}

/// All the folders to look in, in the order of who is to win: the ones a person added, then those of `ladspa_path` (the value of `LADSPA_PATH`, a list of folders in the way
/// the system makes a list of paths), then the `usual` ones. A folder that is in the list more than once is in it once, where it first is; an empty path is not a folder.
pub fn folders_to_scan(added_by_a_person: &[PathBuf], ladspa_path: Option<&OsStr>, usual: &[PathBuf]) -> Vec<PathBuf> {
    let from_the_variable = ladspa_path.map(|value| std::env::split_paths(value).collect::<Vec<_>>());
    let mut folders: Vec<PathBuf> = Vec::new();
    for folder in added_by_a_person
        .iter()
        .chain(from_the_variable.iter().flatten())
        .chain(usual)
    {
        if !folder.as_os_str().is_empty() && !folders.contains(folder) {
            folders.push(folder.clone());
        }
    }
    folders
}

/// The files in `folder` that are libraries of this system (by their endings), in the order of their names (without regard to case). Only the folder itself is looked in, as `LADSPA_PATH` is. A folder
/// that is not there has none.
pub fn libraries_in(folder: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut libraries = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let has_the_ending = path.extension().and_then(OsStr::to_str).is_some_and(|extension| {
            library_file_extensions()
                .iter()
                .any(|ending| extension.eq_ignore_ascii_case(ending))
        });
        if has_the_ending && path.is_file() {
            libraries.push(path);
        }
    }
    // (In the order of the names without regard to case, so that the order is the same on every system; and then, to be a total order, by the names as they are.)
    libraries.sort_by_key(|path| {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        (name.to_lowercase(), name)
    });
    Ok(libraries)
}
