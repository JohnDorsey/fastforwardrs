// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

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
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use fastforwardrs::engine::{AdapterChoice, there_is_a_software_adapter};
use fastforwardrs::media::{FfmpegToolchain, MediaLocation, probe_source_media, probe_video};
use fastforwardrs::project::{
    AnimationTarget, Clip, ClipId, InputColourOverride, MediaClip, Project, PropertyId, PropertyValue, Settings,
    SourceAudioProperties, SourceFrameIndex, SourceMedia, SourceMediaIdentifier, SourceMediaLink, SpeedFactor,
    SpeedIndicatorLayout, TextObject, TimelineFrameIndex, Track, TrackId,
};
use fastforwardrs::rgba_picture::RgbaPicture;
use fastforwardrs::video_export::{ExportEvent, ExportOptions, ExportResult, VideoExport, plan_export};

pub mod sound_files;
pub mod yuv_videos;

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

/// Whether the tests that export can run: the export is made by the picture engine, which the tests run on a **software** graphics adapter and nothing else (WARP on
/// Windows, lavapipe on Linux), so that nothing of it is done on a real graphics card. Where there is none the test says so and is skipped, unless
/// `FASTFORWARDRS_REQUIRE_SOFTWARE_ADAPTER` is `1`, in which case it fails (as in the CI of Windows and Ubuntu, where there is one and must be).
pub fn there_is_graphics_to_export_with() -> bool {
    // (Asked once, and with the right to use the graphics: looking for the adapters is what makes a graphics instance, and a software driver (lavapipe, on Linux) has
    // been seen to crash the program when that is done on one thread while another thread draws.)
    static THERE_IS_ONE: OnceLock<bool> = OnceLock::new();
    let there_is_one = *THERE_IS_ONE.get_or_init(|| {
        let _one_at_a_time = one_export_at_a_time();
        there_is_a_software_adapter()
    });
    if there_is_one {
        return true;
    }
    if std::env::var("FASTFORWARDRS_REQUIRE_SOFTWARE_ADAPTER").as_deref() == Ok("1") {
        panic!("there is no software adapter, and FASTFORWARDRS_REQUIRE_SOFTWARE_ADAPTER=1 says there must be one");
    }
    eprintln!("SKIPPED: there is no software graphics adapter on this computer");
    false
}

/// Starts a test that exports with the toolchain, or returns from it (skipping it) when ffmpeg or a software graphics adapter is missing.
#[macro_export]
macro_rules! toolchain_and_graphics_or_skip {
    () => {{
        let toolchain = $crate::toolchain_or_skip!();
        if !$crate::support::there_is_graphics_to_export_with() {
            return;
        }
        toolchain
    }};
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

    /// Like [`Self::COUNTER`], but at 25 frames per second (as European television) and 3 seconds long (75
    /// pictures): for a project of 30 frames a second, which has to make 90 frames of them.
    pub const PAL_3_SECONDS: TestVideoSpecification = TestVideoSpecification {
        name: "pal_3_seconds",
        frame_rate: "25",
        duration_in_seconds: 3,
        ..Self::COUNTER
    };

    /// Like [`Self::COUNTER`], but at 24 frames per second (as film) and 3 seconds long (72 pictures): for a
    /// project of 30 frames a second, in which every fourth picture is exactly half way between two frames.
    pub const FILM_3_SECONDS: TestVideoSpecification = TestVideoSpecification {
        name: "film_3_seconds",
        frame_rate: "24",
        duration_in_seconds: 3,
        ..Self::COUNTER
    };

    /// Like [`Self::COUNTER`], but at 12 frames per second and 3 seconds long (36 pictures): for a project of
    /// 30 frames a second, in which each picture is on screen for two or three frames.
    pub const SLOW_3_SECONDS: TestVideoSpecification = TestVideoSpecification {
        name: "twelve_a_second_3_seconds",
        frame_rate: "12",
        duration_in_seconds: 3,
        ..Self::COUNTER
    };

    /// Like [`Self::PAL_3_SECONDS`], and 20 seconds long (500 pictures): as long as [`Self::COUNTER`] is,
    /// which is 600 frames at 30 a second.
    pub const PAL_20_SECONDS: TestVideoSpecification = TestVideoSpecification {
        name: "pal_20_seconds",
        duration_in_seconds: 20,
        ..Self::PAL_3_SECONDS
    };
}

/// A generated test video file.
#[derive(Clone, Debug)]
pub struct TestVideo {
    pub path: PathBuf,
    pub specification: TestVideoSpecification,
}

/// A number that is the same for two specifications exactly when they say the same (FNV-1a of what `Debug` prints, which has every field).
fn fingerprint_of(specification: &TestVideoSpecification) -> u32 {
    format!("{specification:?}").bytes().fold(0x811c_9dc5u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
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

    // (The file is named by everything the specification says and not only by its name: two specifications that share a name but differ would otherwise share
    // a file, and whichever test program ran first would decide what the other found. One did: the video of a test of the probe was made without its sound by a
    // test of the frame stream that ran before it, on a computer that skipped the tests that export.)
    let file_stem = format!("{}-{:08x}", specification.name, fingerprint_of(specification));
    let path = fixture_directory().join(format!("{file_stem}.{}", specification.container));
    if !path.exists() {
        let unfinished_path = fixture_directory().join(format!("{file_stem}.unfinished.{}", specification.container));
        generate_video(toolchain, specification, &unfinished_path);
        fs::rename(&unfinished_path, &path).expect("could not move the finished test video into place");
    }
    TestVideo {
        path,
        specification: specification.clone(),
    }
}

/// A video of 2 seconds (60 frames at 30 a second) of one flat colour, 320 by 180 pixels, lossless, made in `folder`.
pub fn flat_colour_video(toolchain: &FfmpegToolchain, folder: &Path, (red, green, blue): (u8, u8, u8)) -> PathBuf {
    let path = folder.join(format!("flat_{red}_{green}_{blue}.mp4"));
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
        .arg(format!("color=c=0x{red:02X}{green:02X}{blue:02X}:s=320x180:r=30:d=2"))
        .args([
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
        ])
        .arg(&path)
        .output()
        .expect("could not run ffmpeg to make a video");
    assert!(
        output.status.success(),
        "making a flat colour video failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    path
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

/// The number of the picture of a video that runs at `video_rate` frames a second which a project that
/// runs at `project_rate` shows in its frame `slot`. This is what ffmpeg's `fps` filter does with
/// `round=near`, which is how the editor and the export bring a video to the rate of the project
/// (measured, not assumed: the tests of the frame stream check this against the real thing, and the
/// tests that use it check the editor with it): each picture goes to the frame of the project that is
/// nearest to its time, and each frame shows the latest picture that went there or before it. At the
/// exact middle between two pictures the earlier one stays.
pub fn picture_shown_in_frame(video_rate: u64, project_rate: u64, slot: u64) -> u64 {
    let slot_the_picture_goes_to = |picture: u64| (2 * picture * project_rate + video_rate) / (2 * video_rate);
    (0..)
        .take_while(|&picture| slot_the_picture_goes_to(picture) <= slot)
        .last()
        .expect("the first picture goes to the first frame")
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

/// A project of the whole of `video` that was told that the video has `told_total_frame_count` pictures, as it was when the project was made: the file has been
/// replaced since by one that has fewer, so that a decoder gives fewer pictures than the project asks for.
pub fn project_for_a_video_that_has_fewer_pictures_than_the_project_was_told(
    toolchain: &FfmpegToolchain,
    video: &TestVideo,
    told_total_frame_count: u64,
) -> Project {
    let mut told = probe_video(toolchain, &video.path).expect("the test video can be probed");
    assert!(
        told.total_frame_count < told_total_frame_count,
        "a premise: the file is shorter than it was said to be"
    );
    told.total_frame_count = told_total_frame_count;
    let mut project = Project::empty();
    project
        .import_source(
            SourceMedia::video(
                SourceMediaIdentifier::new(1),
                SourceMediaLink::from_absolute_path(video.path.clone()),
                told,
            ),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    project
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

/// A project of the counter video's first 60 frames with, on a track in front of it, a picture file
/// (twice as wide as it is tall, red on its left half and blue on its right half) from timeline frame
/// `first_frame` on, shown for `frames` frames. The picture file is made in `folder`.
pub fn project_with_a_picture_over_a_video(
    toolchain: &FfmpegToolchain,
    folder: &Path,
    first_frame: u64,
    frames: u64,
) -> (Project, ClipId) {
    let video = counter_video(toolchain);
    let mut project = project_with_tracks_of_videos(
        toolchain,
        &[&video],
        &[(
            0,
            &[ClipOfTheVideo {
                gap_before_frames: 0,
                first_source_frame: 0,
                end_source_frame: 60,
                speed: 1,
            }],
        )],
    );
    let path = red_and_blue_picture(toolchain, folder, "red_and_blue.png", 64, 32);
    let properties = probe_source_media(toolchain, &path).expect("the picture can be probed");
    let imported = project
        .import_source(
            SourceMedia {
                identifier: SourceMediaIdentifier::new(2),
                link: SourceMediaLink::from_absolute_path(path),
                properties,
                colour: InputColourOverride::AUTOMATIC,
            },
            TimelineFrameIndex::new(first_frame),
        )
        .unwrap();
    assert!(
        imported.created_track,
        "the picture goes on a track of its own, in front"
    );
    let picture = imported.clip.expect("a picture file makes a clip");
    project
        .set_clip_duration(picture, frames)
        .expect("a picture can be shown for any length of time");
    (project, picture)
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
pub fn clip_number(project: &Project, clip_number: usize) -> ClipId {
    let track = project.tracks().last().expect("the project has a track");
    track.entries()[clip_number].id()
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

/// How bright the green of the pixel at `(x, y)` of a decoded picture `width` pixels across is.
pub fn brightness_at(picture: &[u8], width: usize, x: usize, y: usize) -> u8 {
    picture[(y * width + x) * 3 + 1]
}

/// The smallest box that has all of the pixels of a decoded picture (`width` by `height`, RGB) that are not black:
/// `(left, top, right, bottom)`, of the pixels that are in it.
pub fn box_of_what_is_not_black(picture: &[u8], width: usize, height: usize) -> (usize, usize, usize, usize) {
    let mut found: Option<(usize, usize, usize, usize)> = None;
    for y in 0..height {
        for x in 0..width {
            if brightness_at(picture, width, x, y) > 40 {
                found = Some(match found {
                    None => (x, y, x, y),
                    Some((left, top, right, bottom)) => (left.min(x), top.min(y), right.max(x), bottom.max(y)),
                });
            }
        }
    }
    found.expect("something that is not black is in the picture")
}

/// Puts a keyframe at `timeline_frame` that makes `property` of `target` have `value` there. (The property is animated from
/// then on, if it was not.)
pub fn make_a_keyframe(
    project: &mut Project,
    target: AnimationTarget,
    property: PropertyId,
    timeline_frame: u64,
    value: PropertyValue,
) {
    let frame = TimelineFrameIndex::new(timeline_frame);
    project
        .add_keyframe(target, property, frame)
        .expect("the frame is in what the keyframe is for");
    project
        .set_property_at(target, property, value, frame)
        .expect("a keyframe was just made there");
}

/// What stands in for the picture of a text object in tests of the export that have no text in them
/// (the real renderer needs fonts): a transparent picture of the text's area.
pub fn transparent_text_picture(
    object: &fastforwardrs::project::TextObject,
    picture_width_in_pixels: u32,
    picture_height_in_pixels: u32,
) -> fastforwardrs::rgba_picture::RgbaPicture {
    let (width, height) =
        fastforwardrs::text_rendering::area_size_in_pixels(object, picture_width_in_pixels, picture_height_in_pixels);
    fastforwardrs::rgba_picture::RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: vec![0; (width * height * 4) as usize],
    }
}

/// What stands in for the picture of a text object in tests of what the export does to where it is
/// and how it is turned (the real renderer needs fonts): the text's area with its left half red and
/// its right half blue, all of it opaque, so that where each half goes shows which way it was turned.
pub fn red_then_blue_text_picture(
    object: &fastforwardrs::project::TextObject,
    picture_width_in_pixels: u32,
    picture_height_in_pixels: u32,
) -> fastforwardrs::rgba_picture::RgbaPicture {
    let (width, height) =
        fastforwardrs::text_rendering::area_size_in_pixels(object, picture_width_in_pixels, picture_height_in_pixels);
    let mut rgba_pixels = Vec::with_capacity((width * height * 4) as usize);
    for _row in 0..height {
        for column in 0..width {
            rgba_pixels.extend_from_slice(if column < width / 2 {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 255, 255]
            });
        }
    }
    fastforwardrs::rgba_picture::RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels,
    }
}

/// What stands in for the picture of a text object in tests of what the export does with text whose colour changes (the real
/// renderer needs fonts): the text's area filled with the colour of the text, all of it opaque.
pub fn solid_text_picture(
    object: &TextObject,
    picture_width_in_pixels: u32,
    picture_height_in_pixels: u32,
) -> RgbaPicture {
    let (width, height) =
        fastforwardrs::text_rendering::area_size_in_pixels(object, picture_width_in_pixels, picture_height_in_pixels);
    let colour = object.colour;
    RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: [colour.red, colour.green, colour.blue, 255].repeat((width * height) as usize),
    }
}

/// How long an export of a test project may take before the test gives up.
pub const TIME_ALLOWED_FOR_AN_EXPORT: Duration = Duration::from_secs(300);

/// Two exports drawing on the software adapter at once have crashed programs before (see the notes on the tests of the picture engine): in a test program, one export
/// is made at a time. Whoever starts an export holds this until it has ended.
pub fn one_export_at_a_time() -> MutexGuard<'static, ()> {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What an export in a test is run with: the picture engine on a software graphics adapter, and the text objects drawn by `render_text` (the fonts of a computer are not
/// what a test can count on).
pub fn export_options_with_text_pictures(
    render_text: impl Fn(&TextObject, u32, u32) -> RgbaPicture + Send + 'static,
) -> ExportOptions {
    ExportOptions::new(AdapterChoice::SoftwareOnly).with_text_pictures(render_text)
}

/// What an export in a test is run with when it has no text: see [`export_options_with_text_pictures`].
pub fn export_options() -> ExportOptions {
    export_options_with_text_pictures(transparent_text_picture)
}

/// Exports `project` to `destination` (reading its files from where its links say they are) and waits for it to finish.
pub fn export(toolchain: &FfmpegToolchain, project: &Project, destination: &Path) {
    export_with_text_pictures(toolchain, project, destination, transparent_text_picture);
}

/// Like [`export`], with `render_text` making the pictures of the text objects.
pub fn export_with_text_pictures(
    toolchain: &FfmpegToolchain,
    project: &Project,
    destination: &Path,
    render_text: impl Fn(&TextObject, u32, u32) -> RgbaPicture + Send + 'static,
) {
    let _one_at_a_time = one_export_at_a_time();
    let plan = plan_export(project, Settings::DEFAULT).expect("the project can be exported");
    let source_paths: Vec<MediaLocation> = plan
        .source_files
        .iter()
        .map(|identifier| project.source(*identifier).unwrap().link.absolute_path.clone().into())
        .collect();
    let export = VideoExport::start(
        toolchain,
        &plan,
        &source_paths,
        destination,
        export_options_with_text_pictures(render_text),
        Arc::new(|| {}),
    )
    .expect("it starts");
    let started = Instant::now();
    loop {
        match export.try_next_event() {
            Some(ExportEvent::Finished(result)) => {
                assert_eq!(result, ExportResult::Completed);
                return;
            }
            Some(ExportEvent::Progress { .. }) => {}
            None => {
                assert!(
                    started.elapsed() < TIME_ALLOWED_FOR_AN_EXPORT,
                    "the export did not finish in time"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

/// Makes a picture file of `width` by `height` pixels whose left half is red and whose right half
/// is blue, with ffmpeg, as `name` (what kind of file it is is told by how it ends) in `folder`.
pub fn red_and_blue_picture(
    toolchain: &FfmpegToolchain,
    folder: &Path,
    name: &str,
    width: u32,
    height: u32,
) -> PathBuf {
    let path = folder.join(name);
    let half = width / 2;
    let status = Command::new(toolchain.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(["-f", "lavfi", "-i"])
        .arg(format!(
            "color=c=blue:s={width}x{height},drawbox=x=0:y=0:w={half}:h={height}:color=red:t=fill"
        ))
        .args(["-frames:v", "1"])
        .arg(&path)
        .status()
        .expect("could not run ffmpeg");
    assert!(status.success(), "ffmpeg could not make {name}");
    path
}

/// A scratch folder that is removed when dropped. It is made beside the test videos (under `target`), not in the system's temporary folder: a project file saved
/// in the scratch folder gets a *relative* path to a video only if both are on the same drive, and on the Windows machines of GitHub the system's temporary folder is on
/// another drive than the checkout (the program is right to write no relative path between drives, and `relative_path` has a test for that).
pub fn scratch_directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("scratch-")
        .tempdir_in(fixture_directory())
        .expect("could not make a scratch folder")
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
