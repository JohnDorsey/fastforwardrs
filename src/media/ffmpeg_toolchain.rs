// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding the ffmpeg and ffprobe programs that do all of the work with video files.

use std::env;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::child_process::background_command;

/// Setting this environment variable to a folder makes fastforwardrs look for `ffmpeg` and
/// `ffprobe` in that folder first.
pub const FFMPEG_DIRECTORY_ENVIRONMENT_VARIABLE: &str = "FASTFORWARDRS_FFMPEG_DIRECTORY";

/// The oldest ffmpeg the program works with. It is developed and tested against ffmpeg 9.0.2, and an older one does not give the same
/// pictures: ffmpeg 8.0.1 moves the pictures of a file whose sound starts before its picture by one frame after `-ss` (found on Linux,
/// 2026-10-15, with `framemd5` on one file under both). A toolchain that is older is not used, and the error says why.
pub const OLDEST_FFMPEG_MAJOR_VERSION_THAT_WORKS: u32 = 9;

/// Which version of ffmpeg this is, as far as we can tell from its `-version` banner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfmpegVersion {
    /// The first line of `ffmpeg -version`, for showing in diagnostics.
    pub banner: String,
    /// The major version number (the 9 in 9.0.2). `None` for development builds that only carry
    /// a commit number; those are treated as being very recent.
    pub major_version: Option<u32>,
}

impl FfmpegVersion {
    /// Reads the version out of the first line of `ffmpeg -version`, which looks like
    /// `ffmpeg version 9.0.2-full_build-www.gyan.dev Copyright (c) 2000-2026 the FFmpeg developers`.
    pub fn parse(banner: &str) -> Self {
        let banner = banner.lines().next().unwrap_or("").trim();
        let major_version = banner
            .split_whitespace()
            .skip_while(|word| *word != "version")
            .nth(1)
            .and_then(|version_word| {
                let version_word = version_word.trim_start_matches(['n', 'N', 'v']);
                let leading_digits: String = version_word.chars().take_while(char::is_ascii_digit).collect();
                // "N-115823-gabcdef" (a development build) has no digits before the dash.
                leading_digits.parse().ok()
            });
        Self {
            banner: banner.to_owned(),
            major_version,
        }
    }

    /// Whether this is a version the program works with (see `OLDEST_FFMPEG_MAJOR_VERSION_THAT_WORKS`): not an older release. A development
    /// build with no version number is taken to be recent.
    pub fn is_new_enough(&self) -> bool {
        self.major_version
            .is_none_or(|major| major >= OLDEST_FFMPEG_MAJOR_VERSION_THAT_WORKS)
    }
}

/// Where ffmpeg and ffprobe are, and which version they are.
#[derive(Clone, Debug)]
pub struct FfmpegToolchain {
    ffmpeg_path: PathBuf,
    ffprobe_path: PathBuf,
    version: FfmpegVersion,
}

#[derive(Debug, Error)]
pub enum ToolchainError {
    #[error(
        "ffmpeg and ffprobe, version 9 or newer, could not be found. Install FFmpeg 9 (https://ffmpeg.org/download.html) so that both programs are on your PATH, or put them in the same folder as fastforwardrs. Looked in:\n{places_searched}"
    )]
    NotFound { places_searched: String },
}

impl FfmpegToolchain {
    /// Looks for working copies of ffmpeg and ffprobe: first in the folder named by
    /// [`FFMPEG_DIRECTORY_ENVIRONMENT_VARIABLE`], then next to the running program, then on the
    /// `PATH`.
    pub fn locate() -> Result<Self, ToolchainError> {
        Self::locate_among(&default_candidates())
    }

    /// Like [`Self::locate`], but using one specific folder.
    pub fn locate_in_directory(directory: &Path) -> Result<Self, ToolchainError> {
        Self::locate_among(&[Candidate::in_directory(directory)])
    }

    fn locate_among(candidates: &[Candidate]) -> Result<Self, ToolchainError> {
        let mut places_searched = Vec::new();
        for candidate in candidates {
            match candidate.try_out() {
                Ok(toolchain) => return Ok(toolchain),
                Err(reason) => places_searched.push(format!("  - {}: {reason}", candidate.description)),
            }
        }
        Err(ToolchainError::NotFound {
            places_searched: places_searched.join("\n"),
        })
    }

    /// A toolchain made of paths that were never checked, for unit tests that must not depend on
    /// ffmpeg being installed.
    #[cfg(test)]
    pub(crate) fn unchecked_for_tests(ffmpeg_path: PathBuf, ffprobe_path: PathBuf, major_version: Option<u32>) -> Self {
        Self {
            ffmpeg_path,
            ffprobe_path,
            version: FfmpegVersion {
                banner: String::new(),
                major_version,
            },
        }
    }

    pub fn ffmpeg_path(&self) -> &Path {
        &self.ffmpeg_path
    }

    pub fn ffprobe_path(&self) -> &Path {
        &self.ffprobe_path
    }

    pub fn version(&self) -> &FfmpegVersion {
        &self.version
    }
}

struct Candidate {
    description: String,
    ffmpeg_path: PathBuf,
    ffprobe_path: PathBuf,
}

impl Candidate {
    fn in_directory(directory: &Path) -> Self {
        Self {
            description: directory.display().to_string(),
            ffmpeg_path: directory.join(program_file_name("ffmpeg")),
            ffprobe_path: directory.join(program_file_name("ffprobe")),
        }
    }

    /// Whatever the operating system finds when asked for plain `ffmpeg` and `ffprobe`.
    fn on_path() -> Self {
        Self {
            description: "the PATH".to_owned(),
            ffmpeg_path: PathBuf::from(program_file_name("ffmpeg")),
            ffprobe_path: PathBuf::from(program_file_name("ffprobe")),
        }
    }

    fn try_out(&self) -> Result<FfmpegToolchain, String> {
        self.try_out_asking_for_banners_with(run_version_banner)
    }

    /// [`Self::try_out`], where `banner_of` says what `program -version` prints (so that a test need not have an ffmpeg of each version).
    fn try_out_asking_for_banners_with(
        &self,
        banner_of: impl Fn(&Path) -> Result<String, String>,
    ) -> Result<FfmpegToolchain, String> {
        let version = FfmpegVersion::parse(&banner_of(&self.ffmpeg_path)?);
        if !version.is_new_enough() {
            return Err(too_old_message(&version));
        }
        banner_of(&self.ffprobe_path)?;
        Ok(FfmpegToolchain {
            ffmpeg_path: self.ffmpeg_path.clone(),
            ffprobe_path: self.ffprobe_path.clone(),
            version,
        })
    }
}

/// What is said of an ffmpeg that is too old to be used.
fn too_old_message(version: &FfmpegVersion) -> String {
    format!(
        "this is not ffmpeg {OLDEST_FFMPEG_MAJOR_VERSION_THAT_WORKS} or newer, which the program needs (\"{}\")",
        version.banner
    )
}

fn default_candidates() -> Vec<Candidate> {
    let mut candidates = Vec::new();
    if let Some(directory) = env::var_os(FFMPEG_DIRECTORY_ENVIRONMENT_VARIABLE) {
        candidates.push(Candidate::in_directory(Path::new(&directory)));
    }
    if let Some(directory) = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_owned))
    {
        candidates.push(Candidate::in_directory(&directory));
    }
    candidates.push(Candidate::on_path());
    candidates
}

fn program_file_name(program: &str) -> String {
    if cfg!(windows) {
        format!("{program}.exe")
    } else {
        program.to_owned()
    }
}

/// Runs `program -version` and returns what it printed, or the reason it could not be run.
fn run_version_banner(program: &Path) -> Result<String, String> {
    let output = background_command(program)
        .arg("-version")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!("{} exited with {}", program.display(), output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_major_version_from_release_banners() {
        let cases = [
            (
                "ffmpeg version 9.0.2-full_build-www.gyan.dev Copyright (c) 2000-2026 the FFmpeg developers",
                Some(9),
            ),
            (
                "ffmpeg version 7.0 Copyright (c) 2000-2024 the FFmpeg developers",
                Some(7),
            ),
            (
                "ffmpeg version 6.1.1-3ubuntu5 Copyright (c) 2000-2023 the FFmpeg developers",
                Some(6),
            ),
            (
                "ffmpeg version 4.4.2-0ubuntu0.22.04.1 Copyright (c) 2000-2021 the FFmpeg developers",
                Some(4),
            ),
            (
                "ffmpeg version n7.1 Copyright (c) 2000-2024 the FFmpeg developers",
                Some(7),
            ),
            (
                "ffprobe version 10.1.0 Copyright (c) 2007-2027 the FFmpeg developers",
                Some(10),
            ),
        ];
        for (banner, expected) in cases {
            assert_eq!(FfmpegVersion::parse(banner).major_version, expected, "{banner}");
        }
    }

    #[test]
    fn development_builds_without_a_version_number_have_no_major_version() {
        for banner in [
            "ffmpeg version N-115823-gabcdef012 Copyright (c) 2000-2024 the FFmpeg developers",
            "ffmpeg version git-2024-05-01-abcdef0 Copyright (c) 2000-2024 the FFmpeg developers",
            "",
            "something unrelated",
        ] {
            assert_eq!(FfmpegVersion::parse(banner).major_version, None, "{banner:?}");
        }
    }

    #[test]
    fn only_the_first_banner_line_is_kept() {
        let version = FfmpegVersion::parse("ffmpeg version 9.0.2 Copyright\nbuilt with gcc\nconfiguration: ...");
        assert_eq!(version.banner, "ffmpeg version 9.0.2 Copyright");
    }

    #[test]
    fn only_ffmpeg_nine_and_newer_is_new_enough_and_a_development_build_is_taken_to_be_recent() {
        for (banner, expected) in [
            (
                "ffmpeg version 9.0.2-full_build-www.gyan.dev Copyright (c) 2000-2026 the FFmpeg developers",
                true,
            ),
            (
                "ffmpeg version n9.0.2-27-g7e93014c5b-20261014 Copyright (c) 2000-2026 the FFmpeg developers",
                true,
            ),
            (
                "ffmpeg version 10.1.0 Copyright (c) 2007-2027 the FFmpeg developers",
                true,
            ),
            (
                "ffmpeg version N-115823-gabcdef012 Copyright (c) 2000-2024 the FFmpeg developers",
                true,
            ),
            (
                "ffmpeg version 8.0.1-3ubuntu2 Copyright (c) 2000-2025 the FFmpeg developers",
                false,
            ),
            (
                "ffmpeg version 7.1 Copyright (c) 2000-2024 the FFmpeg developers",
                false,
            ),
            (
                "ffmpeg version 4.4.2-0ubuntu0.22.04.1 Copyright (c) 2000-2021 the FFmpeg developers",
                false,
            ),
        ] {
            assert_eq!(FfmpegVersion::parse(banner).is_new_enough(), expected, "{banner}");
        }
    }

    #[test]
    fn an_ffmpeg_that_is_too_old_is_refused_with_its_banner_and_the_version_that_is_needed() {
        let message = too_old_message(&FfmpegVersion::parse("ffmpeg version 8.0.1-3ubuntu2 Copyright"));
        assert!(message.contains("ffmpeg 9 or newer"), "{message}");
        assert!(message.contains("ffmpeg version 8.0.1-3ubuntu2"), "{message}");
    }

    #[test]
    fn a_candidate_whose_ffmpeg_is_too_old_is_not_used_and_one_that_is_new_enough_is() {
        let candidate = Candidate::in_directory(Path::new("somewhere"));
        let banner = |text: &'static str| move |_: &Path| Ok(text.to_owned());

        let old =
            candidate.try_out_asking_for_banners_with(banner("ffmpeg version 8.0.1-3ubuntu2 Copyright (c) 2000-2025"));
        assert!(old.unwrap_err().contains("ffmpeg 9 or newer"));

        let current = candidate
            .try_out_asking_for_banners_with(banner("ffmpeg version 9.0.2-full_build Copyright (c) 2000-2026"))
            .unwrap();
        assert_eq!(current.version().major_version, Some(9));
        assert_eq!(
            current.ffmpeg_path(),
            Path::new("somewhere").join(program_file_name("ffmpeg"))
        );

        // (A good ffmpeg is no use without its ffprobe.)
        let without_ffprobe = candidate.try_out_asking_for_banners_with(|program: &Path| {
            if program.ends_with(program_file_name("ffprobe")) {
                Err("no ffprobe".to_owned())
            } else {
                Ok("ffmpeg version 9.0.2 Copyright".to_owned())
            }
        });
        assert_eq!(without_ffprobe.unwrap_err(), "no ffprobe");

        let missing =
            candidate.try_out_asking_for_banners_with(|program: &Path| Err(format!("no {}", program.display())));
        assert!(missing.unwrap_err().starts_with("no "));
    }

    #[test]
    fn a_folder_without_the_programs_gives_a_helpful_error_naming_the_folder() {
        let empty_folder = tempfile::tempdir().unwrap();
        let error = FfmpegToolchain::locate_in_directory(empty_folder.path()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("could not be found"), "{message}");
        assert!(message.contains("version 9 or newer"), "{message}");
        assert!(
            message.contains(&empty_folder.path().display().to_string()),
            "{message}"
        );
    }
}
