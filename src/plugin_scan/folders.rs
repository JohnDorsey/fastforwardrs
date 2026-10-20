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

/// The folders that are looked in if nobody says otherwise, from the environment of this program, in the order of who is to win: `LADSPA_PATH`, the usual places of LADSPA, then
/// `CLAP_PATH` and the usual places of CLAP. (The folders a person added come before all of these; see [`folders_to_scan`].)
pub fn folders_looked_in_by_default() -> Vec<PathBuf> {
    folders_looked_in_with(&|name| std::env::var_os(name), dirs::home_dir().as_deref())
}

/// The same, with `environment` giving the value of a variable of the environment (or nothing) and `home` the home folder: for the tests, which do not change the environment of
/// the program they run in.
pub fn folders_looked_in_with(
    environment: &dyn Fn(&str) -> Option<std::ffi::OsString>,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    let mut usual = usual_folders(home);
    usual.extend(clap_folders(
        environment("CLAP_PATH").as_deref(),
        home,
        environment("COMMONPROGRAMFILES").map(PathBuf::from).as_deref(),
        environment("LOCALAPPDATA").map(PathBuf::from).as_deref(),
    ));
    folders_to_scan(&[], environment("LADSPA_PATH").as_deref(), &usual)
}

/// Whether `path` is named like a CLAP library: it ends in `.clap` (whatever case). On Windows and Linux a CLAP library is a file with that ending; on macOS it is a bundle, a
/// folder with that ending, and the program that loads it finds the file inside.
pub fn is_a_clap_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("clap"))
}

/// The folders that CLAP plugins are installed in, from the value of `CLAP_PATH` (a list of folders in the way the system makes a list of paths) and then the usual ones of the
/// system, in that order: Windows `%COMMONPROGRAMFILES%\CLAP` and `%LOCALAPPDATA%\Programs\Common\CLAP`; macOS `/Library/Audio/Plug-Ins/CLAP` and
/// `~/Library/Audio/Plug-Ins/CLAP`; elsewhere `/usr/lib/clap` and `~/.clap`. (The three places a system gives are passed in, so that this does not look at the environment.)
pub fn clap_folders(
    clap_path: Option<&OsStr>,
    home: Option<&Path>,
    common_program_files: Option<&Path>,
    local_app_data: Option<&Path>,
) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = clap_path
        .map(|value| std::env::split_paths(value).collect())
        .unwrap_or_default();
    if cfg!(windows) {
        folders.extend(common_program_files.map(|folder| folder.join("CLAP")));
        folders.extend(local_app_data.map(|folder| folder.join("Programs").join("Common").join("CLAP")));
    } else if cfg!(target_os = "macos") {
        folders.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
        folders.extend(home.map(|home| home.join("Library").join("Audio").join("Plug-Ins").join("CLAP")));
    } else {
        folders.push(PathBuf::from("/usr/lib/clap"));
        folders.extend(home.map(|home| home.join(".clap")));
    }
    folders.retain(|folder| !folder.as_os_str().is_empty());
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

/// Where the system's loader looks for the libraries that a library needs, besides the folder of the library itself, besides the system's own folders: the name of the
/// variable of the environment that lists them. (`PATH` on Windows, `LD_LIBRARY_PATH` on Linux, `DYLD_FALLBACK_LIBRARY_PATH` on macOS.)
pub fn library_search_variable() -> &'static str {
    if cfg!(windows) {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_FALLBACK_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    }
}

/// The folders that the libraries a plugin library needs are looked for in, besides its own folder (which the loader looks in first): `extra` (the folders a person said have
/// them), and then, if `parent_folders` says so, the folder the library's folder is in, and the one that is in, and so on up to the top, nearest first. (LMMS keeps `libfftw3f-3.dll` two
/// folders above its LADSPA plugins.) A folder that is not there is left out, and a folder is in the list once.
pub fn dependency_search_folders(library: &Path, extra: &[PathBuf], parent_folders: bool) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    let mut add = |folder: &Path| {
        if !folder.as_os_str().is_empty() && folder.is_dir() && !folders.iter().any(|already| already == folder) {
            folders.push(folder.to_owned());
        }
    };
    for folder in extra {
        add(folder);
    }
    if parent_folders {
        // (The first parent of the file is its own folder, which is looked in anyway.)
        // (Not the top of the file system: a folder that every program can write to is no place to find libraries in.)
        for ancestor in library
            .ancestors()
            .skip(2)
            .filter(|ancestor| ancestor.parent().is_some())
        {
            add(ancestor);
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

/// How many folders down from a folder that is looked in the CLAP libraries are looked for. (CLAP says to look in a folder and all that is in it; a person who adds a very
/// large folder should not have the whole of it walked.)
const MOST_FOLDERS_DOWN_FOR_CLAP: usize = 6;

/// The CLAP libraries in `folder` **and in the folders in it**, down to six folders: the files that end in `.clap` (on macOS, the folders that end in `.clap`, which are not looked
/// into), in the order of their paths without regard to case. A folder that is a link to another is not looked into, so that a link back up cannot make the walk go for ever. A
/// folder that is not there has none; a folder inside that cannot be read is left out, and the folder that was asked for being unreadable is an error.
pub fn clap_libraries_in(folder: &Path) -> io::Result<Vec<PathBuf>> {
    let mut libraries = Vec::new();
    match fs::read_dir(folder) {
        Ok(_) => collect_clap_libraries(folder, MOST_FOLDERS_DOWN_FOR_CLAP, &mut libraries)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    }
    sort_by_path_without_regard_to_case(&mut libraries);
    Ok(libraries)
}

/// Puts `paths` in the order of their text without regard to case (and then, to be a total order, as they are): the same on every system, whatever order the folders give.
pub(super) fn sort_by_path_without_regard_to_case(paths: &mut [PathBuf]) {
    paths.sort_by_key(|path| {
        let text = path.to_string_lossy().into_owned();
        (text.to_lowercase(), text)
    });
}

fn collect_clap_libraries(folder: &Path, folders_down: usize, libraries: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if is_a_clap_file(&path) && (path.is_file() || (cfg!(target_os = "macos") && path.is_dir())) {
            libraries.push(path);
        } else if kind.is_dir() && folders_down > 0 {
            // (A folder that cannot be read is left out: the rest of the folder is still looked in.)
            let _ = collect_clap_libraries(&path, folders_down - 1, libraries);
        }
    }
    Ok(())
}
