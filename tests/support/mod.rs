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
    Clip, ClipLocation, MediaClip, Project, SourceAudioProperties, SourceFrameIndex, SourceMedia,
    SourceMediaIdentifier, SourceMediaLink, SpeedFactor, SpeedIndicatorLayout, TimelineFrameIndex, Track, TrackId,
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
        .import_source(
            SourceMedia::video(
                SourceMediaIdentifier::new(1),
                SourceMediaLink::from_absolute_path(video.path.clone()),
                probe_video(toolchain, &video.path).expect("the test video can be probed"),
            ),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    // Keep the symbol clear of the frame number strip along the top of the test pictures.
    project.set_speed_indicator_layout(SpeedIndicatorLayout::new(0.05, 0.6, 0.2));
    project
}

/// A clip of a track laid out by hand: the empty frames before it, and the source frames
/// `first_source_frame` up to `end_source_frame` of the video it is made from, at `speed`.
#[derive(Clone, Copy, Debug)]
pub struct ClipOfTheVideo {
    pub gap_before_frames: u64,
    pub first_source_frame: u64,
    pub end_source_frame: u64,
    pub speed: u32,
}

/// A project that has the given test videos as its source files 1, 2, ... (the first of them is
/// the one that gives the finished picture its size and shape) and the given tracks, the one in
/// front first. Each track is the number of the video it uses (counting from 0) and its clips.
/// (The same video can be given several times, to have several source files without making more files.)
pub fn project_with_tracks_of_videos(
    toolchain: &FfmpegToolchain,
    videos: &[&TestVideo],
    tracks: &[(usize, &[ClipOfTheVideo])],
) -> Project {
    let sources: Vec<SourceMedia> = videos
        .iter()
        .enumerate()
        .map(|(number, video)| {
            SourceMedia::video(
                SourceMediaIdentifier::new(number as u32 + 1),
                SourceMediaLink::from_absolute_path(video.path.clone()),
                probe_video(toolchain, &video.path).expect("the test video can be probed"),
            )
        })
        .collect();
    let tracks = tracks
        .iter()
        .enumerate()
        .map(|(number, (video_number, clips))| {
            let clips = clips.iter().map(|clip| {
                let media = MediaClip::new(
                    SourceMediaIdentifier::new(*video_number as u32 + 1),
                    SourceFrameIndex::new(clip.first_source_frame),
                    SourceFrameIndex::new(clip.end_source_frame),
                    SpeedFactor::new(clip.speed).unwrap(),
                )
                .unwrap();
                (clip.gap_before_frames, Clip::Video(media))
            });
            Track::new(TrackId::new(number as u32), format!("track {number}")).with_clips(clips)
        })
        .collect();
    // Keep the symbol clear of the frame number strip along the top of the test pictures.
    Project::from_parts(sources, tracks, SpeedIndicatorLayout::new(0.05, 0.6, 0.2)).unwrap()
}

/// A sound file of a steady tone: `frequency` hertz for `seconds`, at 48000 moments a second, mono.
pub fn tone_file(toolchain: &FfmpegToolchain, frequency_in_hertz: u32, seconds: u32) -> PathBuf {
    let path = fixture_directory().join(format!("tone_{frequency_in_hertz}_hertz_{seconds}_seconds.wav"));
    static GENERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = GENERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !path.exists() {
        let unfinished = fixture_directory().join(format!("tone_{frequency_in_hertz}_{seconds}.unfinished.wav"));
        let output = Command::new(toolchain.ffmpeg_path())
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
            .arg(format!(
                "sine=frequency={frequency_in_hertz}:sample_rate=48000:duration={seconds}"
            ))
            .args(["-ac", "1"])
            .arg(&unfinished)
            .output()
            .expect("could not run ffmpeg to make a tone");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        fs::rename(&unfinished, &path).expect("could not move the finished tone into place");
    }
    path
}

/// A project of the counter video's first 10 seconds (a steady 440 hertz tone), and a 4 second tone
/// file of 330 hertz that starts 5 seconds in, on a track in front.
pub fn project_with_a_video_and_a_tone_file(toolchain: &FfmpegToolchain) -> Project {
    let video = counter_video(toolchain);
    let tone = tone_file(toolchain, 330, 4);
    let mut project = project_with_tracks_of_videos(
        toolchain,
        &[&video],
        &[(
            0,
            &[ClipOfTheVideo {
                gap_before_frames: 0,
                first_source_frame: 0,
                end_source_frame: 300,
                speed: 1,
            }],
        )],
    );
    let imported = project
        .import_source(
            SourceMedia::audio(
                SourceMediaIdentifier::new(2),
                SourceMediaLink::from_absolute_path(tone),
                SourceAudioProperties {
                    duration_in_microseconds: 4_000_000,
                },
            ),
            TimelineFrameIndex::new(150),
        )
        .unwrap();
    assert!(imported.created_track, "the sound goes on a track of its own, in front");
    project
}

/// How strong the tone of `frequency_in_hertz` is in `samples` (mono, `sample_rate` a second): the
/// size of the sine wave of that frequency that they contain, found with the Goertzel method. For a
/// pure tone of amplitude `a` this is about `a`; for a sound without that tone it is near 0.
pub fn tone_strength(samples: &[f32], frequency_in_hertz: f64, sample_rate: u32) -> f64 {
    let angle = 2.0 * std::f64::consts::PI * frequency_in_hertz / f64::from(sample_rate);
    let coefficient = 2.0 * angle.cos();
    let (mut previous, mut before_that) = (0.0f64, 0.0f64);
    for &sample in samples {
        let current = f64::from(sample) + coefficient * previous - before_that;
        before_that = previous;
        previous = current;
    }
    let power = previous * previous + before_that * before_that - coefficient * previous * before_that;
    2.0 * power.max(0.0).sqrt() / samples.len().max(1) as f64
}

/// Whether the whole of a decoded RGB picture is black (the test videos' pictures are never).
pub fn is_black(picture: &[u8]) -> bool {
    picture.iter().all(|byte| *byte <= 3)
}

/// The place of the clip that is number `clip_number` (counting from 0) on the project's only track.
pub fn clip_number(project: &Project, clip_number: usize) -> ClipLocation {
    let track = project.tracks().last().expect("the project has a track");
    ClipLocation {
        track: track.id(),
        clip: track.entries()[clip_number].id(),
    }
}

/// How many clips the project's only track has.
pub fn clip_count(project: &Project) -> usize {
    project.tracks().last().map_or(0, |track| track.entries().len())
}

/// Cuts the project's clips at `timeline_frame` (counted on the timeline as it is when the cut is
/// made), as a user does with the S key. A cut that cannot be made is not made.
pub fn split(project: &mut Project, timeline_frame: u64) {
    project.split_at(TimelineFrameIndex::new(timeline_frame), None);
}

/// Plays clip number `clip_number` of the project's only track at `speed`.
pub fn set_clip_speed(project: &mut Project, clip_number: usize, speed: u32) {
    let location = self::clip_number(project, clip_number);
    project
        .set_clip_speed(location, SpeedFactor::new(speed).unwrap())
        .unwrap();
}

/// Splits the project at each timeline frame (counted on the timeline as it is when the split
/// is made) and sets each clip's speed, as a user would.
pub fn edited(mut project: Project, split_frames: &[u64], speeds: &[u32]) -> Project {
    for &frame in split_frames {
        split(&mut project, frame);
    }
    for (clip_index, &speed) in speeds.iter().enumerate() {
        set_clip_speed(&mut project, clip_index, speed);
    }
    project
}

/// What the finished video must show, straight from the project: the source frame number of each
/// finished picture. This is deliberately computed without any of the export code.
pub fn expected_source_frames(project: &Project) -> Vec<u64> {
    project
        .tracks()
        .last()
        .expect("the project has a track")
        .placements()
        .flat_map(|placement| {
            (0..placement.timeline_frame_count()).map(move |offset| {
                placement
                    .source_frame_at(TimelineFrameIndex::new(placement.first_timeline_frame.value() + offset))
                    .unwrap()
                    .value()
            })
        })
        .collect()
}

/// What stands in for the picture of a text object in tests of the export that have no text in them
/// (the real renderer needs fonts): a transparent picture of the text's area.
pub fn transparent_text_picture(
    object: &fastforwardrs::project::TextObject,
    picture_width_in_pixels: u32,
    picture_height_in_pixels: u32,
) -> fastforwardrs::rgba_picture::RgbaPicture {
    let width = (object.area.width() * picture_width_in_pixels as f32).round() as u32;
    let height = (object.area.height() * picture_height_in_pixels as f32).round() as u32;
    fastforwardrs::rgba_picture::RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: vec![0; (width * height * 4) as usize],
    }
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

/// How many times a second the sound `samples` (mono, `sample_rate` samples a second) goes up
/// through zero, which for a pure tone is its pitch in hertz.
pub fn pitch_in_hertz(samples: &[f32], sample_rate: u32) -> f64 {
    let rising_crossings = samples
        .windows(2)
        .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
        .count();
    rising_crossings as f64 / (samples.len() as f64 / f64::from(sample_rate))
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
