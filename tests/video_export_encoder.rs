// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The encoder of the export (`video_export::Encoder`), with the real ffmpeg: frames given to it as planes come out as the frames of a file, in order and none
//! lost; the file says what colours it has, and says nothing when it was told nothing; sound made from a file by a graph is in the file, as long as the picture; progress
//! is reported; stopping it leaves nothing running.

mod support;

use std::path::Path;
use std::sync::{Arc, Mutex};

use fastforwardrs::media::{FfmpegToolchain, MediaLocation};
use fastforwardrs::project::FrameRate;
use fastforwardrs::video_export::{Encoder, EncoderSpecification, OutputTags, SoundForTheEncoder};
use support::{decode_video, probe_json, scratch_directory, stream_durations, tone_file};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

/// The planes of a picture of 64 by 48 pixels that is all one brightness and has no colour.
fn grey_planes(brightness: u8) -> Vec<u8> {
    let luma = (WIDTH * HEIGHT) as usize;
    let chroma = (WIDTH * HEIGHT / 4) as usize;
    let mut planes = vec![brightness; luma];
    planes.extend(std::iter::repeat_n(128, 2 * chroma));
    planes
}

fn specification<'a>(
    output: &'a Path,
    tags: &'a OutputTags,
    sound: Option<SoundForTheEncoder>,
    encoder: fastforwardrs::media::export::VideoEncoder,
) -> EncoderSpecification<'a> {
    EncoderSpecification {
        picture_size: (WIDTH, HEIGHT),
        frame_rate: FrameRate::new(30, 1).unwrap(),
        sound,
        tags,
        video_encoder: encoder,
        output,
    }
}

fn start(toolchain: &FfmpegToolchain, specification: &EncoderSpecification<'_>) -> (Encoder, Arc<Mutex<Vec<u64>>>) {
    let reported = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reported);
    let encoder = Encoder::start(
        toolchain,
        specification,
        Box::new(move |frames| sink.lock().unwrap().push(frames)),
    )
    .expect("ffmpeg starts");
    (encoder, reported)
}

fn libx264_or_mpeg4(toolchain: &FfmpegToolchain) -> fastforwardrs::media::export::VideoEncoder {
    fastforwardrs::media::export::VideoEncoder::choose(toolchain)
}

#[test]
fn the_frames_that_are_written_are_the_frames_of_the_file_in_order_and_none_is_lost() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let output = scratch.path().join("grey.mp4");
    let tags = OutputTags::default();
    let (mut encoder, reported) = start(
        &toolchain,
        &specification(&output, &tags, None, libx264_or_mpeg4(&toolchain)),
    );
    // Frame n is all of the brightness 30 + 4n: 50 different frames (the last is 226, below the 235 that limited range goes up to), each told from the next by 4 levels.
    for number in 0..50u8 {
        encoder.write_frame(&grey_planes(30 + 4 * number)).expect("written");
    }
    let end = encoder.finish();
    assert!(end.status.as_ref().is_ok_and(|status| status.success()), "{end:?}");
    assert_eq!(end.error_output, "");

    let json = probe_json(&toolchain, &output, &["-show_streams", "-count_frames"]);
    let video = &json["streams"][0];
    assert_eq!(video["codec_type"], "video");
    assert_eq!(video["width"], WIDTH);
    assert_eq!(video["height"], HEIGHT);
    assert_eq!(video["pix_fmt"], "yuv420p");
    assert_eq!(video["r_frame_rate"], "30/1");
    assert_eq!(
        video["nb_read_frames"], "50",
        "every frame that was written is in the file"
    );

    let decoded = decode_video(&toolchain, &output, WIDTH, HEIGHT);
    assert_eq!(decoded.pictures.len(), 50);
    let brightness: Vec<u8> = decoded
        .pictures
        .iter()
        .map(|picture| {
            let sum: u64 = picture.iter().skip(1).step_by(3).map(|&green| u64::from(green)).sum();
            (sum / u64::from(WIDTH * HEIGHT)) as u8
        })
        .collect();
    // (Limited range brightness 30 + 4n becomes grey in red, green and blue: (Y - 16) * 255 / 219. Compressed, so within a few levels.)
    for (number, &green) in brightness.iter().enumerate() {
        let expected = ((30.0 + 4.0 * number as f64 - 16.0) * 255.0 / 219.0).clamp(0.0, 255.0);
        assert!(
            (f64::from(green) - expected).abs() <= 4.0,
            "frame {number} is {green}, and should be near {expected}"
        );
    }
    assert!(
        brightness.windows(2).all(|pair| pair[1] > pair[0]),
        "{brightness:?}: in order"
    );

    let reported = reported.lock().unwrap();
    assert_eq!(
        reported.last(),
        Some(&50),
        "the last thing ffmpeg says is how many frames it wrote: {reported:?}"
    );
    assert!(reported.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn a_file_says_the_colours_it_was_told_and_a_file_told_nothing_says_nothing() {
    let toolchain = toolchain_or_skip!();
    let encoder_kind = libx264_or_mpeg4(&toolchain);
    if encoder_kind != fastforwardrs::media::export::VideoEncoder::Libx264 {
        eprintln!("SKIPPED: this ffmpeg has no libx264, and mpeg4 does not write the colours of a picture");
        return;
    }
    let scratch = scratch_directory();
    let cases = [
        ("told_nothing", OutputTags::default(), (None, None, None, None)),
        (
            "bt709_all",
            OutputTags {
                matrix: Some("bt709".to_owned()),
                primaries: Some("bt709".to_owned()),
                transfer: Some("bt709".to_owned()),
            },
            (Some("bt709"), Some("tv"), Some("bt709"), Some("bt709")),
        ),
        (
            "smpte170m_only_the_matrix",
            OutputTags {
                matrix: Some("smpte170m".to_owned()),
                primaries: None,
                transfer: None,
            },
            (Some("smpte170m"), Some("tv"), None, None),
        ),
        (
            "bt2020nc_with_hdr_transfer",
            OutputTags {
                matrix: Some("bt2020nc".to_owned()),
                primaries: Some("bt2020".to_owned()),
                transfer: Some("smpte2084".to_owned()),
            },
            (Some("bt2020nc"), Some("tv"), Some("bt2020"), Some("smpte2084")),
        ),
    ];
    for (name, tags, (matrix, range, primaries, transfer)) in cases {
        let output = scratch.path().join(format!("{name}.mp4"));
        let (mut encoder, _) = start(&toolchain, &specification(&output, &tags, None, encoder_kind));
        for _ in 0..3 {
            encoder.write_frame(&grey_planes(128)).unwrap();
        }
        let end = encoder.finish();
        assert!(
            end.status.as_ref().is_ok_and(|status| status.success()),
            "{name}: {end:?}"
        );
        let json = probe_json(&toolchain, &output, &["-show_streams"]);
        let video = &json["streams"][0];
        let said = |key: &str| video[key].as_str().map(str::to_owned);
        assert_eq!(said("color_space").as_deref(), matrix, "{name}: matrix");
        assert_eq!(said("color_range").as_deref(), range, "{name}: range");
        assert_eq!(said("color_primaries").as_deref(), primaries, "{name}: primaries");
        assert_eq!(said("color_transfer").as_deref(), transfer, "{name}: transfer");
        assert_eq!(
            video["pix_fmt"], "yuv420p",
            "{name}: the colours are told, not converted"
        );
    }
}

#[test]
fn sound_made_from_a_file_by_a_graph_is_in_the_file_and_as_long_as_the_picture() {
    let toolchain = toolchain_or_skip!();
    let tone = tone_file(&toolchain, 330, 4);
    let scratch = scratch_directory();
    let graph_file = scratch.path().join("graph.txt");
    // The tone, made into the kind of sound the export makes, and cut to exactly 3 seconds: the length of 90 frames at 30 a second.
    std::fs::write(
        &graph_file,
        "[1:a:0]aformat=sample_rates=48000:channel_layouts=stereo,atrim=duration=3,asetpts=PTS-STARTPTS[finished_audio]",
    )
    .unwrap();
    let output = scratch.path().join("with_sound.mp4");
    let tags = OutputTags::default();
    let sound = SoundForTheEncoder {
        files: vec![MediaLocation::from(tone)],
        graph_file,
        output_label: "finished_audio",
    };
    let (mut encoder, _) = start(
        &toolchain,
        &specification(&output, &tags, Some(sound), libx264_or_mpeg4(&toolchain)),
    );
    for _ in 0..90 {
        encoder.write_frame(&grey_planes(100)).unwrap();
    }
    let end = encoder.finish();
    assert!(end.status.as_ref().is_ok_and(|status| status.success()), "{end:?}");
    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &output);
    assert!((video_seconds.unwrap() - 3.0).abs() < 0.05, "{video_seconds:?}");
    assert!((audio_seconds.unwrap() - 3.0).abs() < 0.1, "{audio_seconds:?}");
    let json = probe_json(&toolchain, &output, &["-show_streams"]);
    let codecs: Vec<&str> = json["streams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stream| stream["codec_name"].as_str().unwrap())
        .collect();
    assert!(codecs.contains(&"aac"), "{codecs:?}");
}

#[test]
fn an_encoder_that_is_stopped_makes_the_writes_fail_and_leaves_nothing_running() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let output = scratch.path().join("stopped.mp4");
    let tags = OutputTags::default();
    let (mut encoder, _) = start(
        &toolchain,
        &specification(&output, &tags, None, libx264_or_mpeg4(&toolchain)),
    );
    encoder.write_frame(&grey_planes(100)).unwrap();
    let killer = encoder.killer();
    killer.kill();
    let mut failed = false;
    for _ in 0..200 {
        if encoder.write_frame(&grey_planes(100)).is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed, "a write to an encoder that is gone fails");
    let end = encoder.stop();
    assert!(
        !end.status.as_ref().is_ok_and(|status| status.success()),
        "a stopped encoder did not end well: {end:?}"
    );
}

#[test]
fn an_encoder_that_is_dropped_without_being_finished_does_not_stay_running() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    // ffmpeg is given a sound that the graph makes by itself and that lasts an hour, so that it does not end when the picture does (which it would, within a moment, for a
    // picture of one frame): only the drop can end it, and it is still at work when the encoder is dropped.
    let graph_file = scratch.path().join("an_hour_of_silence.txt");
    std::fs::write(
        &graph_file,
        "anullsrc=r=48000:cl=stereo,atrim=duration=3600[finished_audio]",
    )
    .unwrap();
    let output = scratch.path().join("dropped.mp4");
    let tags = OutputTags::default();
    let sound = SoundForTheEncoder {
        files: Vec::new(),
        graph_file,
        output_label: "finished_audio",
    };
    let (mut encoder, _) = start(
        &toolchain,
        &specification(&output, &tags, Some(sound), libx264_or_mpeg4(&toolchain)),
    );
    encoder.write_frame(&grey_planes(100)).unwrap();
    let killer = encoder.killer();
    assert!(killer.is_running(), "a premise: ffmpeg is at work");

    drop(encoder);

    assert!(
        !killer.is_running(),
        "the drop stopped ffmpeg, and waited until it was gone"
    );
    // (So the file can be removed on Windows too, where a file that a program still has open cannot.)
    let _ = std::fs::remove_file(&output);
    assert!(!output.exists(), "nothing holds the file");
}
