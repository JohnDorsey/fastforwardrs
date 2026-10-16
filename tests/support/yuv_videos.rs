// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Videos made of planes that a test says exactly, and the pictures that the program's stream makes of them: for the tests of how video pictures in planes
//! are made into colours (`media_yuv_delivery`) and of what ffmpeg does with them on each system (`media_ffmpeg_quirks`).
//!
//! The videos are tagged by saying what they are (`setparams`) and not by asking ffmpeg to convert them to what the tags say (which is what its `-colorspace`
//! does in the newest versions, and not in older ones): the planes that are stored are the ones that the picture's expressions make, whatever the tags.

#![allow(dead_code)] // Each test file uses a different part of this module.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::engine::chroma_size;
use fastforwardrs::media::{
    DecodedFrame, DecodedYuvFrame, FfmpegToolchain, FrameDelivery, FrameStream, FrameStreamEvent, FrameStreamRequest,
};
use fastforwardrs::project::{FrameRate, SourceFrameIndex, SpeedFactor};

pub const TIME_ALLOWED_FOR_A_STREAM: Duration = Duration::from_secs(30);

/// What a video is made to say about its colours, as the options of ffmpeg's `setparams` filter: nothing, or a matrix and a range.
pub type Tags = &'static [&'static str];

pub const UNTAGGED: Tags = &[];
pub const BT709_LIMITED: Tags = &["colorspace=bt709", "range=tv"];
pub const BT709_FULL: Tags = &["colorspace=bt709", "range=pc"];
pub const SMPTE170M_LIMITED: Tags = &["colorspace=smpte170m", "range=tv"];
pub const BT470BG_FULL: Tags = &["colorspace=bt470bg", "range=pc"];
pub const BT2020_LIMITED: Tags = &["colorspace=bt2020nc", "range=tv"];

/// A picture of four flat colours, one in each quarter, with a ramp of brightness across: strong colour differences (more than 60 either side of grey),
/// and borders between the colours that are as hard as can be.
pub const FOUR_COLOURS: &str = "lum='60+120*X*SW/W':cb='if(lt(X*SW,W/2),if(lt(Y*SH,H/2),64,192),if(lt(Y*SH,H/2),128,224))':\
cr='if(lt(X*SW,W/2),if(lt(Y*SH,H/2),192,64),if(lt(Y*SH,H/2),224,96))'";

/// A picture of stripes of colour four pixels of the colour planes wide, which are 48 on one side of their edge and 208 on the other: the hardest edges
/// of colour there are, which the colour planes of a video have no room to smooth.
pub const HARD_CHROMA_STRIPES: &str =
    "lum='128+60*sin(X/23)*sin(Y/17)':cb='if(lt(mod(X,8),4),48,208)':cr='if(lt(mod(Y,8),4),48,208)'";

pub fn ffmpeg(toolchain: &FfmpegToolchain) -> Command {
    let mut command = Command::new(toolchain.ffmpeg_path());
    command.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
    command
}

pub fn run(mut command: Command) {
    let output = command.output().expect("ffmpeg could not be run");
    assert!(
        output.status.success(),
        "ffmpeg failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The filter that makes a video say it has these colours, without changing a number of it.
pub fn tagging_filter(tags: Tags) -> String {
    if tags.is_empty() {
        "null".to_owned()
    } else {
        format!("setparams={}", tags.join(":"))
    }
}

/// How a picture in `pixel_format` is stored so that every number of it is kept, and what it says about its range with it: the numbers as they are,
/// uncompressed, for `yuv420p` (the lossless codec of ffmpeg, FFV1, keeps only the limited range of it, and changes the numbers of a picture that says
/// it is full range to fit), and FFV1 for the others (the tests that use them look at nothing but how they are stored).
pub fn stored_without_a_change(pixel_format: &str) -> Vec<&str> {
    if pixel_format == "yuv420p" {
        vec!["-c:v", "rawvideo"]
    } else {
        vec!["-c:v", "ffv1", "-pix_fmt", pixel_format]
    }
}

/// A video of one picture, with the three planes the `geq` filter makes of `expressions`, in `pixel_format`, tagged as `tags` say.
pub fn video_made_by_geq(
    toolchain: &FfmpegToolchain,
    folder: &Path,
    name: &str,
    (width, height): (u32, u32),
    expressions: &str,
    pixel_format: &str,
    tags: Tags,
) -> PathBuf {
    let path = folder.join(format!("{name}.mkv"));
    let mut command = ffmpeg(toolchain);
    command
        .args(["-f", "lavfi", "-i"])
        .arg(format!(
            "nullsrc=s={width}x{height}:r=1:d=2,format=yuv420p,geq={expressions},{}",
            tagging_filter(tags)
        ))
        .args(stored_without_a_change(pixel_format));
    command.args(["-frames:v", "1"]).arg(&path);
    run(command);
    path
}

/// The three planes of a picture: numbers that look random and are the same every time.
pub fn planes_of_noise(width: u32, height: u32, seed: u64) -> [Vec<u8>; 3] {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut noise = |count: usize| -> Vec<u8> {
        (0..count)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect()
    };
    let (chroma_width, chroma_height) = chroma_size(width, height);
    let chroma_bytes = (chroma_width * chroma_height) as usize;
    [
        noise((width * height) as usize),
        noise(chroma_bytes),
        noise(chroma_bytes),
    ]
}

/// A video of one picture whose planes are exactly `planes`, tagged as `tags` say.
pub fn video_of_planes(
    toolchain: &FfmpegToolchain,
    folder: &Path,
    name: &str,
    (width, height): (u32, u32),
    planes: &[Vec<u8>; 3],
    tags: Tags,
) -> PathBuf {
    let raw = folder.join(format!("{name}.yuv"));
    std::fs::write(&raw, planes.concat()).expect("the planes could be written");
    let path = folder.join(format!("{name}.mkv"));
    let mut command = ffmpeg(toolchain);
    command
        .args(["-f", "rawvideo", "-pixel_format", "yuv420p", "-video_size"])
        .arg(format!("{width}x{height}"))
        .args(["-framerate", "1", "-i"])
        .arg(&raw)
        .args(["-vf", &tagging_filter(tags)])
        .args(stored_without_a_change("yuv420p"))
        .args(["-frames:v", "1"])
        .arg(&path);
    run(command);
    path
}

/// The first picture that a stream of `video` (which is `video_size` big) delivers when the preview picture is `picture_size`.
pub fn first_picture(
    toolchain: &FfmpegToolchain,
    video: &Path,
    video_size: (u32, u32),
    picture_size: (u32, u32),
    delivery: FrameDelivery,
) -> FrameStreamEvent {
    first_picture_told(toolchain, video, video_size, picture_size, delivery, None)
}

/// [`first_picture`], with ffmpeg told how to read the colours of the planes it makes RGBA of.
pub fn first_picture_told(
    toolchain: &FfmpegToolchain,
    video: &Path,
    video_size: (u32, u32),
    picture_size: (u32, u32),
    delivery: FrameDelivery,
    ffmpeg_reads_planes_as: Option<fastforwardrs::media::HowToReadColours>,
) -> FrameStreamEvent {
    let request = FrameStreamRequest {
        video: video.to_path_buf().into(),
        project_frame_rate: FrameRate::new(1, 1).unwrap(),
        video_frame_rate: FrameRate::new(1, 1).unwrap(),
        first_source_frame: SourceFrameIndex::new(0),
        source_frame_count: 1,
        speed_factor: SpeedFactor::new(1).unwrap(),
        video_display_size_in_pixels: video_size,
        picture_width_in_pixels: picture_size.0,
        picture_height_in_pixels: picture_size.1,
        delivery,
        ffmpeg_reads_planes_as,
    };
    let stream = FrameStream::start(toolchain, &request, Arc::new(|| {})).expect("the stream should start");
    let started = Instant::now();
    loop {
        match stream.try_next_event() {
            Some(FrameStreamEvent::Finished { error_output }) => panic!("no picture came: {error_output:?}"),
            Some(picture) => return picture,
            None => {
                assert!(started.elapsed() < TIME_ALLOWED_FOR_A_STREAM, "no picture came in time");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

pub fn picture_as_rgba(
    toolchain: &FfmpegToolchain,
    video: &Path,
    video_size: (u32, u32),
    picture_size: (u32, u32),
) -> DecodedFrame {
    match first_picture(toolchain, video, video_size, picture_size, FrameDelivery::Rgba) {
        FrameStreamEvent::Frame(frame) => frame,
        other => panic!("RGBA was asked for: {other:?}"),
    }
}

pub fn picture_in_planes(
    toolchain: &FfmpegToolchain,
    video: &Path,
    video_size: (u32, u32),
    picture_size: (u32, u32),
) -> DecodedYuvFrame {
    match first_picture(toolchain, video, video_size, picture_size, FrameDelivery::Yuv420p) {
        FrameStreamEvent::YuvFrame(frame) => frame,
        other => panic!("planes were asked for: {other:?}"),
    }
}
