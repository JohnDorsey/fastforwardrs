// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fetching the plugins of the manifest for a person who asked: each release file is downloaded **from its authors' own address**, **its checksum is checked before anything is taken
//! out of it**, only the files the manifest names are taken out (the plugins, and the licence texts that the manifest gives with their own checksums), and a `NOTICE.txt` says who
//! wrote the set, under which licence and where its source is. A set is built in a folder of its own and put in place only when all of it is there: **a download that fails or is
//! cancelled leaves what was there before as it was.**
//!
//! The download is done by the system's `curl` program, which the editor runs as it runs ffmpeg (so no library for the network, and for the secure connection the computer's own, is
//! in the editor); and a `.tar` archive (the builds for Linux) is opened by the system's `tar`. A `.zip` is opened by the editor.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use super::manifest::{BundleSet, Manifest};
use crate::digest::Sha256Digest;
use crate::media::background_command;

/// How far a download has got, for the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    /// What is being done, for a person.
    pub step: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// Something that can put the contents of an address in a file. (The system's `curl` does it; the tests give files made on the spot.)
pub trait Fetcher: Send + Sync {
    /// Downloads `url` to the file `to`, telling `on_bytes` how many bytes there are as it goes, and stopping if `cancelled` says so.
    fn fetch(
        &self,
        url: &str,
        to: &Path,
        on_bytes: &mut dyn FnMut(u64),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), String>;
}

/// The system's `curl`.
pub struct Curl {
    protocols: &'static str,
}

impl Curl {
    /// **Only https**: an address that is not (or that sends the download to one that is not) is refused.
    pub const fn https_only() -> Self {
        Self { protocols: "=https" }
    }

    /// For a test that serves from the computer it is run on, which is not https.
    #[cfg(test)]
    pub const fn for_a_test_on_this_computer() -> Self {
        Self { protocols: "=http" }
    }
}

impl Fetcher for Curl {
    fn fetch(
        &self,
        url: &str,
        to: &Path,
        on_bytes: &mut dyn FnMut(u64),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        let mut command = background_command(Path::new("curl"));
        command
            .args(["--fail", "--location", "--silent", "--show-error"])
            .args(["--proto", self.protocols, "--proto-redir", self.protocols])
            // (A connection that is reset is tried again too, which `--retry` alone does not do: found by a download that was reset on its second file.)
            .args([
                "--retry",
                "5",
                "--retry-delay",
                "2",
                "--retry-connrefused",
                "--retry-all-errors",
                "--connect-timeout",
                "20",
            ])
            // (A download that has gone quiet for a minute is not going on.)
            .args(["--speed-limit", "1000", "--speed-time", "60"])
            .arg("--output")
            .arg(to)
            .arg("--")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                "the program `curl` was not found, and it is what downloads the plugins (it comes with Windows 10 and later and with macOS, and most Linux systems have it): install it, or fetch the plugins with tools/bundled_plugins/fetch_bundled_plugins.py".to_owned()
            } else {
                format!("`curl` could not be started: {error}")
            }
        })?;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut said = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        let _ = stderr.read_to_string(&mut said);
                    }
                    on_bytes(fs::metadata(to).map_or(0, |metadata| metadata.len()));
                    if status.success() {
                        return Ok(());
                    }
                    let _ = fs::remove_file(to);
                    let said = said.trim();
                    return Err(format!(
                        "{url} could not be downloaded: {}",
                        if said.is_empty() { "curl said no" } else { said }
                    ));
                }
                Ok(None) => {
                    on_bytes(fs::metadata(to).map_or(0, |metadata| metadata.len()));
                    if cancelled() {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = fs::remove_file(to);
                        return Err(CANCELLED.to_owned());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => return Err(format!("`curl` could not be waited for: {error}")),
            }
        }
    }
}

/// What a download that a person stopped says.
pub const CANCELLED: &str = "cancelled";

/// What a download made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The names of the sets that were fetched.
    pub sets: Vec<String>,
    /// The plugin files, each in the folder of its set.
    pub plugin_files: Vec<PathBuf>,
}

/// The folder that the plugins that a person fetched are put in: `fastforwardrs/plugins` in the folder of the programs' data of the person (`%LOCALAPPDATA%` on Windows).
pub fn fetched_plugins_folder() -> Option<PathBuf> {
    dirs::data_local_dir().map(|folder| folder.join("fastforwardrs").join("plugins"))
}

/// Whether every plugin of `set` for `platform` is in its folder in `folder` (a file, or on macOS a bundle, which is a folder).
pub fn is_fetched(set: &BundleSet, platform: &str, folder: &Path) -> bool {
    set.platforms.get(platform).is_some_and(|build| {
        build
            .members
            .iter()
            .all(|member| folder.join(&set.id).join(&member.to).exists())
    })
}

/// How many bytes would be downloaded for `platform`: each release file that is needed, once (two sets can be in one file).
pub fn bytes_to_download(manifest: &Manifest, platform: &str) -> u64 {
    let mut seen = BTreeSet::new();
    manifest
        .sets
        .iter()
        .filter_map(|set| set.platforms.get(platform))
        .filter(|build| seen.insert(build.sha256.clone()))
        .map(|build| build.size)
        .sum()
}

/// Fetches the sets of `manifest` that have a build for `platform` into `folder`, one folder for each set.
///
/// # Errors
/// What went wrong, in words for a person: a release file that could not be downloaded or whose checksum is not the one the manifest says, a file that is not in it, the folder
/// that cannot be written, or that the download was cancelled ([`CANCELLED`]). **A set that was in `folder` before is left as it was if its new version could not be made.**
pub fn fetch_plugins(
    manifest: &Manifest,
    platform: &str,
    folder: &Path,
    fetcher: &dyn Fetcher,
    report: &mut dyn FnMut(&Progress),
    cancelled: &dyn Fn() -> bool,
) -> Result<Outcome, String> {
    let problems = manifest.problems();
    if !problems.is_empty() {
        return Err(format!(
            "the list of plugins is not fit to be used: {}",
            problems.join("; ")
        ));
    }
    let wanted: Vec<&BundleSet> = manifest
        .sets
        .iter()
        .filter(|set| set.platforms.contains_key(platform))
        .collect();
    if wanted.is_empty() {
        return Err(format!("there are no plugins to fetch for {platform}"));
    }
    let temporary = folder.join(".downloading");
    fs::create_dir_all(&temporary).map_err(|error| format!("{} could not be made: {error}", temporary.display()))?;
    let result = fetch_the_sets(
        manifest, &wanted, platform, folder, &temporary, fetcher, report, cancelled,
    );
    let _ = fs::remove_dir_all(&temporary);
    result
}

#[allow(clippy::too_many_arguments)]
fn fetch_the_sets(
    manifest: &Manifest,
    wanted: &[&BundleSet],
    platform: &str,
    folder: &Path,
    temporary: &Path,
    fetcher: &dyn Fetcher,
    report: &mut dyn FnMut(&Progress),
    cancelled: &dyn Fn() -> bool,
) -> Result<Outcome, String> {
    let total = bytes_to_download(manifest, platform);
    let mut finished = 0_u64;
    let mut downloaded: Vec<(String, PathBuf)> = Vec::new();
    let mut outcome = Outcome {
        sets: Vec::new(),
        plugin_files: Vec::new(),
    };
    for (number, set) in wanted.iter().enumerate() {
        let build = &set.platforms[platform];
        let step = format!("{} ({} of {})", set.name, number + 1, wanted.len());
        if cancelled() {
            return Err(CANCELLED.to_owned());
        }
        // (A release file that two sets are in is downloaded once.)
        let archive = if let Some((_, path)) = downloaded.iter().find(|(sha, _)| *sha == build.sha256) {
            path.clone()
        } else {
            let path = temporary.join(format!("{}-{}", build.sha256, file_name_of(&build.url)));
            let before = finished;
            report(&Progress {
                step: format!("Downloading {step}"),
                bytes_done: before,
                bytes_total: total,
            });
            fetcher.fetch(
                &build.url,
                &path,
                &mut |bytes| {
                    report(&Progress {
                        step: format!("Downloading {step}"),
                        bytes_done: before + bytes,
                        bytes_total: total,
                    });
                },
                cancelled,
            )?;
            report(&Progress {
                step: format!("Checking {step}"),
                bytes_done: before + build.size,
                bytes_total: total,
            });
            check_the_checksum(&path, &build.sha256, &build.url)?;
            finished += build.size;
            downloaded.push((build.sha256.clone(), path.clone()));
            path
        };
        report(&Progress {
            step: format!("Unpacking {step}"),
            bytes_done: finished,
            bytes_total: total,
        });
        let building = folder.join(format!("{}.building", set.id));
        let _ = fs::remove_dir_all(&building);
        let made = build_the_set(set, build, &archive, &building, temporary, fetcher, cancelled);
        if let Err(error) = made {
            let _ = fs::remove_dir_all(&building);
            return Err(error);
        }
        let place = folder.join(&set.id);
        let _ = fs::remove_dir_all(&place);
        fs::rename(&building, &place)
            .map_err(|error| format!("{} could not be put in place: {error}", place.display()))?;
        outcome.sets.push(set.name.clone());
        outcome
            .plugin_files
            .extend(build.members.iter().map(|member| place.join(&member.to)));
    }
    report(&Progress {
        step: "Done".to_owned(),
        bytes_done: total,
        bytes_total: total,
    });
    Ok(outcome)
}

fn build_the_set(
    set: &BundleSet,
    build: &super::manifest::Build,
    archive: &Path,
    building: &Path,
    temporary: &Path,
    fetcher: &dyn Fetcher,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    fs::create_dir_all(building).map_err(|error| format!("{} could not be made: {error}", building.display()))?;
    {
        let mut opened = super::archive::open(archive, build.installer.as_deref(), temporary)?;
        for member in &build.members {
            opened.take(&member.from, &building.join(&member.to))?;
        }
    }
    for text in &set.licence_texts {
        let path = temporary.join(format!("{}-{}", text.sha256, text.name));
        fetcher.fetch(&text.url, &path, &mut |_| {}, cancelled)?;
        check_the_checksum(&path, &text.sha256, &text.url)?;
        fs::copy(&path, building.join(&text.name))
            .map_err(|error| format!("the licence text {} could not be written: {error}", text.name))?;
    }
    fs::write(building.join("NOTICE.txt"), notice_of(set))
        .map_err(|error| format!("NOTICE.txt could not be written: {error}"))
}

fn check_the_checksum(path: &Path, wanted: &str, url: &str) -> Result<(), String> {
    let found = Sha256Digest::of_file(path)
        .map_err(|error| format!("{} could not be read to be checked: {error}", path.display()))?;
    if found.to_hex() == wanted {
        Ok(())
    } else {
        let _ = fs::remove_file(path);
        Err(format!(
            "{url} is not the file that was expected (its checksum is different: the file at that address changed, or was damaged on the way), so nothing of it was used"
        ))
    }
}

fn file_name_of(url: &str) -> &str {
    url.rsplit('/').next().unwrap_or("download")
}

/// What `NOTICE.txt` in the folder of a set says.
pub fn notice_of(set: &BundleSet) -> String {
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
        "These files are the plugins as their authors released them, unaltered, fetched by fastforwardrs from the address of the release, and they are used under their own licence (the text is in this folder).".to_owned(),
        String::new(),
        format!("The corresponding source of this version is at: {}", set.source.url),
    ];
    if let Some(note) = &set.source.note {
        lines.push(note.clone());
    }
    lines.join("\n") + "\n"
}
