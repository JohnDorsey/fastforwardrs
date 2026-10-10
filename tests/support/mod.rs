//! Shared helpers for the integration tests that run the real ffmpeg.
//!
//! The tests make their own small test videos on the fly (nothing large is stored in the
//! repository). In every test video each frame carries its own frame number as a strip of sixteen
//! black or white blocks along the top of the picture, which survives scaling and compression. So
//! a test can look at a frame it received and know exactly which source frame it was.

#![allow(dead_code)] // Each integration test file uses a different part of this module.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use fastforwardrs::media::{FfmpegToolchain, probe_video};
use fastforwardrs::project::{
    Project, SourceVideo, SourceVideoIdentifier, SourceVideoLink, SpeedFactor, SpeedIndicatorLayout, TimelineFrameIndex,
};

/// Set this to `1` to make the tests fail (instead of skipping themselves) when ffmpeg is missing.
pub const REQUIRE_FFMPEG_ENVIRONMENT_VARIABLE: &str = "FASTFORWARDRS_REQUIRE_FFMPEG";

/// How many blocks the frame number strip is made of, so frame numbers up to 65535 fit.
const FRAME_NUMBER_BLOCK_COUNT: u32 = 16;

/// The strip takes up this much of the top of a picture that is 180 pixels tall (and scales with
/// the picture).
const STRIP_HEIGHT_AT_180_PIXELS_TALL: u32 = 40;

/// Finds ffmpeg for a test. When it is missing the test is skipped with a message, unless
/// [`REQUIRE_FFMPEG_ENVIRONMENT_VARIABLE`] is set to `1`, in which case it fails loudly.
pub fn toolchain() -> Option<FfmpegToolchain> {
    match FfmpegToolchain::locate() {
        Ok(toolchain) => Some(toolchain),
        Err(error) => {
            if std::env::var(REQUIRE_FFMPEG_ENVIRONMENT_VARIABLE).as_deref() == Ok("1") {
                panic!("ffmpeg is required but was not found: {error}");
            }
            eprintln!("SKIPPED: ffmpeg is not installed ({error})");
            None
        }
    }
}

/// Starts a test with the toolchain, or returns from it (skipping it) when ffmpeg is missing.
#[macro_export]
macro_rules! toolchain_or_skip {
    () => {
        match $crate::support::toolchain() {
            Some(toolchain) => toolchain,
            None => return,
        }
    };
}

/// The settings of a test video.
#[derive(Clone, Debug)]
pub struct TestVideoSpecification {
    /// Names the video file, so that different specifications never share a file.
    pub name: &'static str,
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    /// For example `"30"` or `"30000/1001"`.
    pub frame_rate: &'static str,
    pub duration_in_seconds: u32,
    pub with_audio: bool,
    /// What the sound is. `None` is a steady 440 Hz tone. Otherwise an `aevalsrc` expression of the
    /// time `t` in seconds, such as `0.3*sin(2*PI*(300+200*floor(t))*t)`, a tone that is a different
    /// pitch for each second, so that a test can tell from the sound where in the video it is.
    pub audio_expression: Option<&'static str>,
    /// A filter applied to the numbered picture (for example to set a pixel shape, or to drop
    /// frames and so make the frame rate variable).
    pub extra_video_filter: Option<&'static str>,
    /// The file type: `mp4` stores timestamps exactly, `mkv` rounds them to milliseconds.
    pub container: &'static str,
    /// Extra ffmpeg output options, such as `-fps_mode vfr` to keep gaps in a variable frame rate.
    pub extra_output_arguments: &'static [&'static str],
}

impl TestVideoSpecification {
    /// 320x180, 30 frames per second, 20 seconds long (600 frames), with a mono sine-wave track.
    pub const COUNTER: TestVideoSpecification = TestVideoSpecification {
        name: "counter_20_seconds",
        width_in_pixels: 320,
        height_in_pixels: 180,
        frame_rate: "30",
        duration_in_seconds: 20,
        with_audio: true,
        audio_expression: None,
        extra_video_filter: None,
        container: "mp4",
        extra_output_arguments: &[],
    };
}

/// A generated test video file.
#[derive(Clone, Debug)]
pub struct TestVideo {
    pub path: PathBuf,
    pub specification: TestVideoSpecification,
}

fn fixture_directory() -> PathBuf {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test_fixtures");
    fs::create_dir_all(&directory).expect("could not create the test fixture folder");
    directory
}

/// Makes (or reuses) the test video described by `specification`.
pub fn test_video(toolchain: &FfmpegToolchain, specification: &TestVideoSpecification) -> TestVideo {
    // Tests run in parallel threads: only one may be generating fixtures at a time.
    static GENERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = GENERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let path = fixture_directory().join(format!("{}.{}", specification.name, specification.container));
    if !path.exists() {
        let unfinished_path =
            fixture_directory().join(format!("{}.unfinished.{}", specification.name, specification.container));
        generate_video(toolchain, specification, &unfinished_path);
        fs::rename(&unfinished_path, &path).expect("could not move the finished test video into place");
    }
    TestVideo {
        path,
        specification: specification.clone(),
    }
}

/// The standard 20 second, 30 frames per second test video.
pub fn counter_video(toolchain: &FfmpegToolchain) -> TestVideo {
    test_video(toolchain, &TestVideoSpecification::COUNTER)
}

fn generate_video(toolchain: &FfmpegToolchain, specification: &TestVideoSpecification, destination: &Path) {
    let picture = format!(
        "nullsrc=s={}x{}:r={}:d={},geq=lum='if(lt(Y,{}),255*mod(floor(N/pow(2,floor(X/{}))),2),90)':cb=128:cr=128",
        specification.width_in_pixels,
        specification.height_in_pixels,
        specification.frame_rate,
        specification.duration_in_seconds,
        specification.height_in_pixels * STRIP_HEIGHT_AT_180_PIXELS_TALL / 180,
        specification.width_in_pixels / FRAME_NUMBER_BLOCK_COUNT,
    );
    let picture = match specification.extra_video_filter {
        Some(filter) => format!("{picture},{filter}"),
        None => picture,
    };

    let mut command = Command::new(toolchain.ffmpeg_path());
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg(picture);
    if specification.with_audio {
        let sound_source = match specification.audio_expression {
            Some(expression) => format!(
                "aevalsrc=exprs={expression}:sample_rate=48000:duration={}",
                specification.duration_in_seconds
            ),
            None => format!(
                "sine=frequency=440:sample_rate=48000:duration={}",
                specification.duration_in_seconds
            ),
        };
        command.args(["-f", "lavfi", "-i"]).arg(sound_source);
    }
    // Lossless H.264 with a keyframe every second: exact pixels, and cheap seeking.
    command.args([
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-qp",
        "0",
        "-pix_fmt",
        "yuv420p",
        "-g",
        "30",
    ]);
    if specification.with_audio {
        command.args(["-c:a", "aac", "-b:a", "128k", "-shortest"]);
    }
    command.args(specification.extra_output_arguments);
    command.arg(destination);

    let output = command.output().expect("could not run ffmpeg to make a test video");
    assert!(
        output.status.success(),
        "making test video {:?} failed: {}",
        specification.name,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Reads the frame number out of a picture made by the test videos. `pixels` holds the picture
/// row by row, `bytes_per_pixel` bytes each (3 for RGB, 4 for RGBA); the first byte of each pixel
/// is used, which is enough because the strip is grey.
pub fn frame_number_in_picture(
    pixels: &[u8],
    width_in_pixels: u32,
    height_in_pixels: u32,
    bytes_per_pixel: u32,
) -> u64 {
    let block_width = f64::from(width_in_pixels) / f64::from(FRAME_NUMBER_BLOCK_COUNT);
    let strip_height = (f64::from(height_in_pixels) * f64::from(STRIP_HEIGHT_AT_180_PIXELS_TALL) / 180.0).max(2.0);
    let sample_row = (strip_height / 2.0) as u32;
    let mut frame_number = 0u64;
    for block in 0..FRAME_NUMBER_BLOCK_COUNT {
        let sample_column = ((f64::from(block) + 0.5) * block_width) as u32;
        let offset = ((sample_row * width_in_pixels + sample_column) * bytes_per_pixel) as usize;
        if pixels[offset] > 127 {
            frame_number |= 1 << block;
        }
    }
    frame_number
}

/// A project made from `video`, as the editor would make it when the video is imported.
pub fn project_for(toolchain: &FfmpegToolchain, video: &TestVideo) -> Project {
    let mut project = Project::empty();
    project
        .import_source_video(SourceVideo {
            identifier: SourceVideoIdentifier::new(1),
            link: SourceVideoLink::from_absolute_path(video.path.clone()),
            properties: probe_video(toolchain, &video.path).expect("the test video can be probed"),
        })
        .unwrap();
    // Keep the symbol clear of the frame number strip along the top of the test pictures.
    project.set_speed_indicator_layout(SpeedIndicatorLayout::new(0.05, 0.6, 0.2));
    project
}

/// Splits the project at each timeline frame (counted on the timeline as it is when the split
/// is made) and sets each clip's speed, as a user would.
pub fn edited(mut project: Project, split_frames: &[u64], speeds: &[u32]) -> Project {
    for &frame in split_frames {
        project.split_at(TimelineFrameIndex::new(frame)).unwrap();
    }
    for (clip_index, &speed) in speeds.iter().enumerate() {
        project
            .set_clip_speed(clip_index, SpeedFactor::new(speed).unwrap())
            .unwrap();
    }
    project
}

/// What the finished video must show, straight from the project: the source frame number of each
/// finished picture. This is deliberately computed without any of the export code.
pub fn expected_source_frames(project: &Project) -> Vec<u64> {
    project
        .video_track()
        .placements()
        .flat_map(|placement| {
            (0..placement.clip.timeline_frame_count())
                .map(move |offset| placement.clip.source_frame_at_timeline_offset(offset).unwrap().value())
        })
        .collect()
}

/// A scratch folder that is removed when dropped.
pub fn scratch_directory() -> tempfile::TempDir {
    tempfile::tempdir().expect("could not make a scratch folder")
}

/// Every picture of a video file, decoded to RGB (three bytes per pixel), at the file's own size.
pub struct DecodedVideo {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    pub pictures: Vec<Vec<u8>>,
}

impl DecodedVideo {
    /// The frame number carried by each picture (see the module documentation).
    pub fn frame_numbers(&self) -> Vec<u64> {
        self.pictures
            .iter()
            .map(|picture| frame_number_in_picture(picture, self.width_in_pixels, self.height_in_pixels, 3))
            .collect()
    }
}

/// Decodes every picture of the video at `path`, which must be `width` by `height` pixels.
pub fn decode_video(
    toolchain: &FfmpegToolchain,
    path: &Path,
    width_in_pixels: u32,
    height_in_pixels: u32,
) -> DecodedVideo {
    let output = Command::new(toolchain.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-an", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("could not run ffmpeg to decode a video");
    assert!(
        output.status.success(),
        "decoding {path:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let picture_size = (width_in_pixels * height_in_pixels * 3) as usize;
    assert_eq!(
        output.stdout.len() % picture_size,
        0,
        "the decoded data is not a whole number of {width_in_pixels}x{height_in_pixels} pictures"
    );
    DecodedVideo {
        width_in_pixels,
        height_in_pixels,
        pictures: output.stdout.chunks(picture_size).map(<[u8]>::to_vec).collect(),
    }
}

/// The sound of the file at `path` as 32-bit floating point samples, `channels` interleaved
/// channels at `sample_rate` samples per second.
pub fn decoded_sound(toolchain: &FfmpegToolchain, path: &Path, channels: u32, sample_rate: u32) -> Vec<f32> {
    let output = Command::new(toolchain.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-vn", "-f", "f32le", "-ac"])
        .arg(channels.to_string())
        .args(["-ar"])
        .arg(sample_rate.to_string())
        .arg("-")
        .output()
        .expect("could not run ffmpeg to decode sound");
    assert!(
        output.status.success(),
        "decoding the sound of {path:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    floating_point_samples(&output.stdout)
}

/// Reads bytes of little-endian 32-bit floating point samples (as ffmpeg's `f32le` writes them).
pub fn floating_point_samples(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|sample| f32::from_le_bytes(*sample))
        .collect()
}

/// How loud `samples` are on the whole: their root mean square. A silent stretch is 0, and the test
/// videos' sine wave is about 0.35.
pub fn loudness(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
}

/// How long each stream of a media file is, in seconds: `(video, audio)`.
pub fn stream_durations(toolchain: &FfmpegToolchain, path: &Path) -> (Option<f64>, Option<f64>) {
    let json = probe_json(toolchain, path, &["-show_streams"]);
    let duration_of = |codec_type: &str| {
        json["streams"]
            .as_array()
            .unwrap()
            .iter()
            .find(|stream| stream["codec_type"] == codec_type)
            .and_then(|stream| stream["duration"].as_str())
            .and_then(|text| text.parse::<f64>().ok())
    };
    (duration_of("video"), duration_of("audio"))
}

/// A tiny deterministic random number generator for building random edits. (The library has its
/// own for its unit tests, but unit test helpers are not visible to integration tests.)
pub struct RandomNumbers(u64);

impl RandomNumbers {
    pub fn with_seed(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03 | 1)
    }

    pub fn next_below(&mut self, upper_bound: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % upper_bound
    }
}

/// Runs ffprobe on `path` and returns what it printed, parsed as JSON.
pub fn probe_json(toolchain: &FfmpegToolchain, path: &Path, extra_arguments: &[&str]) -> serde_json::Value {
    let output = Command::new(toolchain.ffprobe_path())
        .args(["-v", "error", "-print_format", "json"])
        .args(extra_arguments)
        .arg(path)
        .output()
        .expect("could not run ffprobe");
    assert!(
        output.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("ffprobe printed something that is not JSON")
}
