// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gives the program a **build identity**: a number made of the contents of the code it is built from (`src`, `crates`, `Cargo.toml`, `Cargo.lock`), so that two programs built from
//! the same code have the same one, and a program built from other code has another. The editor and the program that scans plugin libraries (`fastforwardrs-plugin-scan`) are built
//! from one library, so they have one identity when they were built together, and the editor can tell when it has been given a scan program that was built from older code (a
//! `cargo run` builds the editor and not the program beside it). See `plugin_scan::BUILD_ID`.
//!
//! (Contents, not times: a program that is copied or unpacked has the identity it was built with.)

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Every file under `folder`, except the folders that are build products or tools' files.
fn files_under(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target" || name == ".git") {
                continue;
            }
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}

fn main() {
    let mut files = vec![
        PathBuf::from("Cargo.toml"),
        PathBuf::from("Cargo.lock"),
        PathBuf::from("build.rs"),
    ];
    files_under(Path::new("src"), &mut files);
    files_under(Path::new("crates"), &mut files);
    files.sort();
    let mut hasher = DefaultHasher::new();
    for file in &files {
        file.hash(&mut hasher);
        // (Line endings are not part of the code: the same text with the endings of another system is the same code.)
        let text = std::fs::read(file).unwrap_or_default();
        let text: Vec<u8> = text.into_iter().filter(|byte| *byte != b'\r').collect();
        text.hash(&mut hasher);
    }
    println!("cargo:rustc-env=FFRS_BUILD_ID={:016x}", hasher.finish());
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=crates");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");
}
