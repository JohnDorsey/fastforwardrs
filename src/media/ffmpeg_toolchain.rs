//! Finding the ffmpeg and ffprobe programs that do all of the work with video files.

use std::env;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::child_process::background_command;

/// Setting this environment variable to a folder makes fastforwardrs look for `ffmpeg` and
/// `ffprobe` in that folder first.
pub const FFMPEG_DIRECTORY_ENVIRONMENT_VARIABLE: &str = "FASTFORWARDRS_FFMPEG_DIRECTORY";

/// The first version of ffmpeg that can read a long filter graph from a file with
/// `-/filter_complex`. Older versions use `-filter_complex_script` instead, and ffmpeg 9 no
/// longer accepts that.
const FIRST_VERSION_WITH_SLASH_PREFIX_OPTIONS: u32 = 7;

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
        "ffmpeg and ffprobe could not be found. Install FFmpeg (https://ffmpeg.org/download.html) so that both programs are on your PATH, or put them in the same folder as fastforwardrs. Looked in:\n{places_searched}"
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

    /// Whether a filter graph kept in a file is passed with `-/filter_complex <file>` (ffmpeg 7
    /// and newer) instead of the older `-filter_complex_script <file>`.
    pub fn reads_filter_graph_files_with_slash_prefix(&self) -> bool {
        self.version
            .major_version
            .is_none_or(|major| major >= FIRST_VERSION_WITH_SLASH_PREFIX_OPTIONS)
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
        let version = run_version_banner(&self.ffmpeg_path)?;
        run_version_banner(&self.ffprobe_path)?;
        Ok(FfmpegToolchain {
            ffmpeg_path: self.ffmpeg_path.clone(),
            ffprobe_path: self.ffprobe_path.clone(),
            version: FfmpegVersion::parse(&version),
        })
    }
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

    fn toolchain_with_major_version(major_version: Option<u32>) -> FfmpegToolchain {
        FfmpegToolchain {
            ffmpeg_path: PathBuf::from("ffmpeg"),
            ffprobe_path: PathBuf::from("ffprobe"),
            version: FfmpegVersion {
                banner: String::new(),
                major_version,
            },
        }
    }

    #[test]
    fn filter_graph_files_use_the_slash_prefix_from_ffmpeg_seven_onwards() {
        assert!(!toolchain_with_major_version(Some(4)).reads_filter_graph_files_with_slash_prefix());
        assert!(!toolchain_with_major_version(Some(6)).reads_filter_graph_files_with_slash_prefix());
        assert!(toolchain_with_major_version(Some(7)).reads_filter_graph_files_with_slash_prefix());
        assert!(toolchain_with_major_version(Some(9)).reads_filter_graph_files_with_slash_prefix());
        assert!(toolchain_with_major_version(None).reads_filter_graph_files_with_slash_prefix());
    }

    #[test]
    fn a_folder_without_the_programs_gives_a_helpful_error_naming_the_folder() {
        let empty_folder = tempfile::tempdir().unwrap();
        let error = FfmpegToolchain::locate_in_directory(empty_folder.path()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("could not be found"), "{message}");
        assert!(
            message.contains(&empty_folder.path().display().to_string()),
            "{message}"
        );
    }
}
