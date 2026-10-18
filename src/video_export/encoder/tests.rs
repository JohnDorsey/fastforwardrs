// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The command line of the encoder, and the sound graph it is given. (The encoder running, with the real ffmpeg: `tests/video_export_encoder.rs`.)

use super::*;
use crate::project::{Project, Settings};
use crate::video_export::plan_export;
use crate::video_export::test_projects::{audio_source, frame, project_with_video};

fn as_text(arguments: Vec<OsString>) -> Vec<String> {
    arguments
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

fn sound_of(files: &[&str]) -> SoundForTheEncoder {
    SoundForTheEncoder::FilterGraph {
        files: files.iter().map(|name| MediaLocation::from(*name)).collect(),
        graph_file: PathBuf::from("graph.txt"),
        output_label: "finished_audio",
    }
}

fn arguments_for(
    tags: &OutputTags,
    sound: Option<SoundForTheEncoder>,
    video_encoder: VideoEncoder,
    output: &str,
    frame_rate: FrameRate,
) -> Vec<String> {
    let output = Path::new(output);
    as_text(encoder_arguments(&EncoderSpecification {
        picture_size: (1920, 1080),
        frame_rate,
        sound,
        tags,
        video_encoder,
        output,
    }))
}

fn thirty() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

#[test]
fn a_complete_command_with_sound_and_tags() {
    let tags = OutputTags {
        matrix: Some("bt709".to_owned()),
        primaries: Some("bt709".to_owned()),
        transfer: Some("bt709".to_owned()),
    };
    let arguments = arguments_for(
        &tags,
        Some(sound_of(&["a.wav", "b.mp3"])),
        VideoEncoder::Libx264,
        "out.mp4",
        thirty(),
    );
    assert_eq!(
        arguments,
        [
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "yuv420p",
            "-video_size",
            "1920x1080",
            "-framerate",
            "30/1",
            "-i",
            "pipe:0",
            "-i",
            "a.wav",
            "-i",
            "b.mp3",
            "-/filter_complex",
            "graph.txt",
            "-map",
            "0:v",
            "-map",
            "[finished_audio]",
            "-vf",
            "setparams=colorspace=bt709:range=tv:color_primaries=bt709:color_trc=bt709",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
            "out.mp4",
        ]
    );
}

#[test]
fn the_picture_is_the_first_input_and_the_sound_files_come_after_it_in_the_order_given() {
    let arguments = arguments_for(
        &OutputTags::default(),
        Some(sound_of(&["a.mp4", "b.mp3", "c.wav"])),
        VideoEncoder::Libx264,
        "out.mp4",
        thirty(),
    );
    let inputs: Vec<&String> = arguments
        .windows(2)
        .filter(|pair| pair[0] == "-i")
        .map(|pair| &pair[1])
        .collect();
    assert_eq!(inputs, ["pipe:0", "a.mp4", "b.mp3", "c.wav"]);
    let pipe = arguments.iter().position(|argument| argument == "pipe:0").unwrap();
    assert_eq!(arguments[pipe - 1], "-i");
    assert!(
        arguments[..pipe].windows(2).any(|pair| pair == ["-f", "rawvideo"]),
        "the options of the raw video come before the input they are for"
    );
}

#[test]
fn without_sound_there_is_no_sound_input_no_graph_and_no_audio_codec() {
    let arguments = arguments_for(&OutputTags::default(), None, VideoEncoder::Libx264, "out.mp4", thirty());
    assert_eq!(arguments.iter().filter(|argument| *argument == "-i").count(), 1);
    assert!(!arguments.contains(&"-/filter_complex".to_owned()));
    assert!(
        !arguments
            .iter()
            .any(|argument| argument.contains("audio") || argument == "aac")
    );
    assert_eq!(arguments.iter().filter(|argument| *argument == "-map").count(), 1);
}

#[test]
fn a_file_that_says_nothing_about_its_colours_has_no_filter_for_them() {
    let arguments = arguments_for(&OutputTags::default(), None, VideoEncoder::Libx264, "out.mp4", thirty());
    assert!(!arguments.contains(&"-vf".to_owned()), "{arguments:?}");
    assert!(
        !arguments
            .iter()
            .any(|argument| argument.contains("setparams") || argument.contains("colorspace")),
        "{arguments:?}"
    );
}

#[test]
fn the_colours_are_told_by_a_filter_and_never_by_an_option_that_converts() {
    let tags = OutputTags {
        matrix: Some("smpte170m".to_owned()),
        primaries: None,
        transfer: None,
    };
    let arguments = arguments_for(&tags, None, VideoEncoder::Libx264, "out.mkv", thirty());
    let position = arguments.iter().position(|argument| argument == "-vf").unwrap();
    assert_eq!(arguments[position + 1], "setparams=colorspace=smpte170m:range=tv");
    for option in ["-colorspace", "-color_range", "-color_primaries", "-color_trc"] {
        assert!(
            !arguments.contains(&option.to_owned()),
            "{option} would make ffmpeg convert the picture"
        );
    }
}

#[test]
fn the_frame_rate_is_given_as_a_fraction_so_that_a_rate_like_29_97_is_exact() {
    let ntsc = FrameRate::new(30000, 1001).unwrap();
    let arguments = arguments_for(&OutputTags::default(), None, VideoEncoder::Libx264, "out.mp4", ntsc);
    let position = arguments.iter().position(|argument| argument == "-framerate").unwrap();
    assert_eq!(arguments[position + 1], "30000/1001");
}

#[test]
fn the_fallback_encoder_is_used_when_asked_for() {
    let arguments = arguments_for(&OutputTags::default(), None, VideoEncoder::Mpeg4, "out.mp4", thirty());
    assert!(
        arguments.windows(2).any(|pair| pair == ["-c:v", "mpeg4"]),
        "{arguments:?}"
    );
    assert!(!arguments.contains(&"libx264".to_owned()));
}

#[test]
fn the_index_is_put_at_the_start_only_of_files_that_can_have_it_there() {
    for (output, expected) in [
        ("a.mp4", true),
        ("a.MP4", true),
        ("a.mov", true),
        ("a.m4v", true),
        ("a.mkv", false),
        ("a.webm", false),
        ("a", false),
    ] {
        let arguments = arguments_for(&OutputTags::default(), None, VideoEncoder::Libx264, output, thirty());
        assert_eq!(arguments.contains(&"+faststart".to_owned()), expected, "{output}");
    }
}

#[test]
fn the_output_file_is_the_last_argument_and_progress_goes_to_the_standard_output() {
    let arguments = arguments_for(
        &OutputTags::default(),
        Some(sound_of(&["a.wav"])),
        VideoEncoder::Libx264,
        "folder/out.mkv",
        thirty(),
    );
    assert_eq!(arguments.last().unwrap(), "folder/out.mkv");
    assert!(arguments.windows(2).any(|pair| pair == ["-progress", "pipe:1"]));
    assert!(
        !arguments.contains(&"-nostdin".to_owned()),
        "the standard input is where the picture comes from"
    );
}

// --- the sound graph ---

#[test]
fn the_sound_graph_for_the_encoder_is_the_one_of_the_plan_with_every_input_one_further_on() {
    // A video with sound and a sound file: two files the sound is made from, which are the plan's sound inputs 0 and 1.
    let mut project: Project = project_with_video(320, 180, true);
    project.import_source(audio_source(2, 4), frame(50)).unwrap();
    let plan = plan_export(&project, Settings::DEFAULT).unwrap();
    let sound = plan.sound.expect("the project has sound");
    assert_eq!(sound.source_files.len(), 2);

    let for_the_plan = build_audio_graph(&sound.specification);
    let for_the_encoder = sound_graph_for_the_encoder(&sound);
    assert_ne!(for_the_plan.text, for_the_encoder.text);
    assert!(for_the_plan.text.contains("[0:a:0]") && for_the_plan.text.contains("[1:a:0]"));
    assert!(!for_the_plan.text.contains("[2:a:0]"));
    assert!(for_the_encoder.text.contains("[1:a:0]") && for_the_encoder.text.contains("[2:a:0]"));
    assert!(
        !for_the_encoder.text.contains("[0:a:0]"),
        "input 0 is the picture, which has no sound"
    );
    let moved_by_hand = for_the_plan
        .text
        .replace("[1:a:0]", "[2:a:0]")
        .replace("[0:a:0]", "[1:a:0]");
    assert_eq!(for_the_encoder.text, moved_by_hand, "nothing else differs");
    assert_eq!(for_the_encoder.output_label, for_the_plan.output_label);
}

#[test]
fn sound_that_is_made_already_is_the_second_input_as_raw_samples_and_is_what_is_mapped_and_the_graph_is_not_used() {
    let sound = Some(SoundForTheEncoder::RawFile {
        path: PathBuf::from("sound.f32"),
    });
    let arguments = arguments_for(
        &OutputTags::default(),
        sound,
        VideoEncoder::Libx264,
        "out.mp4",
        thirty(),
    );
    let position = |wanted: &str| arguments.iter().position(|argument| argument == wanted).unwrap();
    // The picture is input 0 and the sound input 1, read as the kind of sound the sound engine writes.
    let picture = position("pipe:0");
    let sound_file = position("sound.f32");
    assert!(sound_file > picture);
    assert_eq!(
        &arguments[sound_file - 7..=sound_file],
        ["-f", "f32le", "-ar", "48000", "-ac", "2", "-i", "sound.f32"],
        "the format is said before the input it is about"
    );
    let map = arguments.iter().rposition(|argument| argument == "-map").unwrap();
    assert_eq!(
        arguments[map + 1],
        "1:a",
        "the sound is mapped from input 1, and not from a graph"
    );
    assert!(!arguments.iter().any(|argument| argument == "-/filter_complex"));
    assert!(
        arguments.contains(&"aac".to_owned()),
        "and it is compressed as a sound is"
    );
}
