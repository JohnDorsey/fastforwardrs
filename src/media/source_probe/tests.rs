// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// Most tests below do not care about counting packets, so this stand-in (which shadows the
/// real function) makes counting always fail: the length then comes from the file's own
/// statement of its frame count, or from its duration.
fn properties_from_probe_output(probe_output_json: &str) -> Result<SourceVideoProperties, ProbeError> {
    super::properties_from_probe_output(probe_output_json, |_| Err(ProbeError::UnknownLength))
}

fn frame_rate(numerator: u32, denominator: u32) -> FrameRate {
    FrameRate::new(numerator, denominator).unwrap()
}

/// What ffprobe prints for a 20 second, 30 fps Matroska file: no frame count, and a format
/// duration that is a fraction of a frame too long (20.021 s would be 600.6 frames).
const MATROSKA_WITHOUT_A_FRAME_COUNT: &str = r#"{
        "streams": [
            { "index": 0, "codec_type": "video", "width": 320, "height": 180,
              "avg_frame_rate": "30/1", "r_frame_rate": "30/1", "duration": "N/A", "nb_frames": "N/A" }
        ],
        "format": { "duration": "20.021000" }
    }"#;

/// Regression test: the duration of a Matroska file made the video seem to have 601 frames
/// when it has 600, so the timeline ended one frame after the real video did.
#[test]
fn a_file_that_does_not_state_its_frame_count_has_its_packets_counted() {
    let estimated_from_duration = properties_from_probe_output(MATROSKA_WITHOUT_A_FRAME_COUNT).unwrap();
    assert_eq!(
        estimated_from_duration.total_frame_count, 601,
        "the duration alone is misleading"
    );

    let counted = super::properties_from_probe_output(MATROSKA_WITHOUT_A_FRAME_COUNT, |_| Ok(600)).unwrap();
    assert_eq!(counted.total_frame_count, 600);
}

#[test]
fn the_packets_of_the_video_stream_are_counted_not_those_of_another_stream() {
    let json = r#"{ "streams": [
            { "index": 0, "codec_type": "audio", "sample_rate": "48000" },
            { "index": 1, "codec_type": "video", "width": 320, "height": 180, "avg_frame_rate": "30/1" } ],
            "format": { "duration": "2.0" } }"#;
    let mut counted_stream = None;
    let properties = super::properties_from_probe_output(json, |stream_index| {
        counted_stream = Some(stream_index);
        Ok(60)
    })
    .unwrap();
    assert_eq!(counted_stream, Some(1));
    assert_eq!(properties.total_frame_count, 60);
}

#[test]
fn a_stated_frame_count_is_trusted_and_nothing_is_counted() {
    let properties = super::properties_from_probe_output(ORDINARY_VIDEO_WITH_AUDIO, |_| {
        panic!("the packets must not be counted when the file states its frame count")
    })
    .unwrap();
    assert_eq!(properties.total_frame_count, 600);
}

#[test]
fn a_failed_or_empty_count_falls_back_to_the_duration() {
    for count in [Err(ProbeError::UnknownLength), Ok(0)] {
        let properties = super::properties_from_probe_output(MATROSKA_WITHOUT_A_FRAME_COUNT, |_| count).unwrap();
        assert_eq!(properties.total_frame_count, 601);
    }
}

/// What ffprobe prints (trimmed to the fields we read) for a plain 320x180, 30 fps, 20 second
/// video with a mono audio track, such as the test video the integration tests generate.
const ORDINARY_VIDEO_WITH_AUDIO: &str = r#"{
        "streams": [
            { "index": 0, "codec_type": "video", "width": 320, "height": 180,
              "sample_aspect_ratio": "1:1", "r_frame_rate": "30/1", "avg_frame_rate": "30/1",
              "duration": "20.000000", "nb_frames": "600",
              "disposition": { "default": 1, "attached_pic": 0 } },
            { "index": 1, "codec_type": "audio", "sample_rate": "48000", "channels": 1,
              "disposition": { "default": 1, "attached_pic": 0 } }
        ],
        "format": { "duration": "20.000000" }
    }"#;

fn format_of(stream_fields: &str) -> VideoStreamFormat {
    video_stream_format_from_probe_output(&format!(
        r#"{{ "streams": [ {{ "index": 0, "codec_type": "video", "width": 64, "height": 48 {stream_fields} }} ] }}"#
    ))
    .unwrap()
}

#[test]
fn where_the_colour_differences_sit_is_read_and_a_file_that_does_not_say_has_the_usual_place() {
    for (said, expected) in [
        (r#", "chroma_location": "left""#, ChromaSiting::Left),
        (r#", "chroma_location": "center""#, ChromaSiting::Center),
        (r#", "chroma_location": "topleft""#, ChromaSiting::TopLeft),
        (r#", "chroma_location": "unspecified""#, ChromaSiting::Left),
        (r#", "chroma_location": "top""#, ChromaSiting::Left),
        ("", ChromaSiting::Left),
    ] {
        assert_eq!(format_of(said).chroma_siting(), expected, "{said}");
    }
    assert_eq!(
        format_of(r#", "chroma_location": "center""#).chroma_location.as_deref(),
        Some("center")
    );
    assert_eq!(format_of(r#", "chroma_location": "unspecified""#).chroma_location, None);
}

#[test]
fn the_pixel_format_and_the_colours_a_video_says_are_read_and_what_it_does_not_say_is_nothing() {
    let format = format_of(
        r#", "pix_fmt": "yuv420p", "color_space": "bt709", "color_range": "tv", "color_primaries": "bt709", "color_transfer": "bt709""#,
    );
    assert_eq!(
        format,
        VideoStreamFormat {
            pixel_format: Some("yuv420p".to_owned()),
            colour_space: Some("bt709".to_owned()),
            colour_range: Some("tv".to_owned()),
            colour_primaries: Some("bt709".to_owned()),
            colour_transfer: Some("bt709".to_owned()),
            chroma_location: None,
        }
    );
    assert_eq!(format_of(""), VideoStreamFormat::default());
    for nothing in ["unknown", "unspecified", "N/A", "", "  "] {
        let format = format_of(&format!(
            r#", "pix_fmt": "yuv420p", "color_space": "{nothing}", "color_range": "{nothing}""#
        ));
        assert_eq!(format.colour_space, None, "{nothing:?}");
        assert_eq!(format.colour_range, None, "{nothing:?}");
        assert_eq!(format.pixel_format.as_deref(), Some("yuv420p"));
    }
}

#[test]
fn the_format_is_that_of_the_first_real_video_stream_and_not_of_cover_art_or_sound() {
    let json = r#"{ "streams": [
            { "index": 0, "codec_type": "audio" },
            { "index": 1, "codec_type": "video", "pix_fmt": "yuvj444p", "disposition": { "attached_pic": 1 } },
            { "index": 2, "codec_type": "video", "pix_fmt": "yuv420p", "color_space": "bt470bg" } ] }"#;
    let format = video_stream_format_from_probe_output(json).unwrap();
    assert_eq!(format.pixel_format.as_deref(), Some("yuv420p"));
    assert_eq!(format.colour_space.as_deref(), Some("bt470bg"));
    assert!(matches!(
        video_stream_format_from_probe_output(r#"{ "streams": [ { "index": 0, "codec_type": "audio" } ] }"#),
        Err(ProbeError::NoVideoStream)
    ));
    assert!(matches!(
        video_stream_format_from_probe_output("not json"),
        Err(ProbeError::UnreadableAnswer(_))
    ));
}

#[test]
fn eight_bit_four_two_zero_video_is_native_and_which_matrix_and_range_follow_what_the_file_says() {
    let native = |fields: &str| format_of(&format!(r#", "pix_fmt": "yuv420p" {fields}"#)).as_native_yuv420p(64, 48);
    assert_eq!(
        native(""),
        Some(NativeYuv420p {
            matrix: VideoMatrix::Bt601,
            full_range: false
        }),
        "says nothing, and is small: BT.601, limited"
    );
    // (Rewritten on purpose in phase 3: a file that says nothing is BT.709 when its picture is high definition: see `source_colour`.)
    assert_eq!(
        format_of(r#", "pix_fmt": "yuv420p""#).as_native_yuv420p(1920, 1080),
        Some(NativeYuv420p {
            matrix: VideoMatrix::Bt709,
            full_range: false
        }),
        "says nothing, and is high definition: BT.709, limited"
    );
    for (name, matrix) in [
        ("bt470bg", VideoMatrix::Bt601),
        ("smpte170m", VideoMatrix::Bt601),
        ("bt709", VideoMatrix::Bt709),
        ("bt2020nc", VideoMatrix::Bt2020),
    ] {
        assert_eq!(
            native(&format!(r#", "color_space": "{name}""#)),
            Some(NativeYuv420p {
                matrix,
                full_range: false
            }),
            "{name}"
        );
    }
    assert_eq!(
        native(r#", "color_range": "tv""#).map(|native| native.full_range),
        Some(false)
    );
    assert_eq!(
        native(r#", "color_range": "pc""#).map(|native| native.full_range),
        Some(true)
    );
    assert_eq!(
        native(r#", "color_space": "bt709", "color_range": "pc""#),
        Some(NativeYuv420p {
            matrix: VideoMatrix::Bt709,
            full_range: true
        })
    );
}

#[test]
fn video_that_is_stored_another_way_or_uses_a_matrix_that_is_not_known_is_left_to_ffmpeg() {
    for pixel_format in [
        "yuv420p10le",
        "yuv422p",
        "yuv444p",
        "yuvj420p",
        "nv12",
        "rgb24",
        "gbrp",
        "gray",
        "yuva420p",
    ] {
        assert_eq!(
            format_of(&format!(r#", "pix_fmt": "{pixel_format}""#)).as_native_yuv420p(64, 48),
            None,
            "{pixel_format}"
        );
    }
    assert_eq!(
        format_of("").as_native_yuv420p(64, 48),
        None,
        "a video that does not even say how it is stored"
    );
    for matrix in [
        "bt2020c",
        "smpte240m",
        "fcc",
        "ycgco",
        "rgb",
        "smpte2085",
        "chroma-derived-nc",
        "ictcp",
    ] {
        assert_eq!(
            format_of(&format!(r#", "pix_fmt": "yuv420p", "color_space": "{matrix}""#)).as_native_yuv420p(64, 48),
            None,
            "{matrix}"
        );
    }
    assert_eq!(
        format_of(r#", "pix_fmt": "yuv420p", "color_range": "strange""#).as_native_yuv420p(64, 48),
        None
    );
}

#[test]
fn reads_an_ordinary_video() {
    let properties = properties_from_probe_output(ORDINARY_VIDEO_WITH_AUDIO).unwrap();
    assert_eq!(
        properties,
        SourceVideoProperties {
            frame_rate: frame_rate(30, 1),
            total_frame_count: 600,
            display_width_in_pixels: 320,
            display_height_in_pixels: 180,
            has_audio: true,
        }
    );
}

#[test]
fn a_video_without_an_audio_stream_reports_no_audio() {
    let json = r#"{ "streams": [ { "codec_type": "video", "width": 64, "height": 48,
            "avg_frame_rate": "25/1", "nb_frames": "50" } ], "format": {} }"#;
    let properties = properties_from_probe_output(json).unwrap();
    assert!(!properties.has_audio);
    assert_eq!(properties.total_frame_count, 50);
}

#[test]
fn a_video_turned_on_its_side_swaps_its_width_and_height() {
    for rotation in ["-90", "90", "270", "-270"] {
        let json = format!(
            r#"{{ "streams": [ {{ "codec_type": "video", "width": 1920, "height": 1080, "avg_frame_rate": "30/1",
                    "nb_frames": "30", "side_data_list": [ {{ "side_data_type": "Display Matrix", "rotation": {rotation} }} ] }} ] }}"#
        );
        let properties = properties_from_probe_output(&json).unwrap();
        assert_eq!(
            (properties.display_width_in_pixels, properties.display_height_in_pixels),
            (1080, 1920),
            "rotation {rotation}"
        );
    }
}

#[test]
fn a_video_turned_upside_down_or_not_at_all_keeps_its_width_and_height() {
    for rotation in ["0", "180", "-180", "360"] {
        let json = format!(
            r#"{{ "streams": [ {{ "codec_type": "video", "width": 1920, "height": 1080, "avg_frame_rate": "30/1",
                    "nb_frames": "30", "side_data_list": [ {{ "rotation": {rotation} }} ] }} ] }}"#
        );
        let properties = properties_from_probe_output(&json).unwrap();
        assert_eq!(
            (properties.display_width_in_pixels, properties.display_height_in_pixels),
            (1920, 1080),
            "rotation {rotation}"
        );
    }
}

#[test]
fn old_style_rotate_tags_are_understood_too() {
    let json = r#"{ "streams": [ { "codec_type": "video", "width": 1280, "height": 720, "avg_frame_rate": "30/1",
            "nb_frames": "30", "tags": { "rotate": "90" } } ] }"#;
    let properties = properties_from_probe_output(json).unwrap();
    assert_eq!(
        (properties.display_width_in_pixels, properties.display_height_in_pixels),
        (720, 1280)
    );
}

#[test]
fn non_square_pixels_widen_or_narrow_the_shown_picture() {
    // A PAL widescreen DVD frame: 720x576 stored, pixels 64:45 wide, shown 1024x576.
    let json = r#"{ "streams": [ { "codec_type": "video", "width": 720, "height": 576,
            "sample_aspect_ratio": "64:45", "avg_frame_rate": "25/1", "nb_frames": "25" } ] }"#;
    let properties = properties_from_probe_output(json).unwrap();
    assert_eq!(
        (properties.display_width_in_pixels, properties.display_height_in_pixels),
        (1024, 576)
    );
}

#[test]
fn unknown_or_nonsensical_pixel_shapes_count_as_square() {
    for shape in ["N/A", "0:1", "1:0", "abc", "", "-4:3"] {
        let json = format!(
            r#"{{ "streams": [ {{ "codec_type": "video", "width": 640, "height": 360, "sample_aspect_ratio": "{shape}",
                    "avg_frame_rate": "25/1", "nb_frames": "25" }} ] }}"#
        );
        let properties = properties_from_probe_output(&json).unwrap();
        assert_eq!(properties.display_width_in_pixels, 640, "shape {shape:?}");
    }
}

#[test]
fn without_a_counted_number_of_frames_the_length_is_worked_out_from_the_duration() {
    // Matroska files usually have no frame count. 12.345 s at 29.97 fps is about 370 frames.
    let json = r#"{ "streams": [ { "codec_type": "video", "width": 640, "height": 360,
            "avg_frame_rate": "30000/1001", "duration": "12.345000" } ] }"#;
    assert_eq!(properties_from_probe_output(json).unwrap().total_frame_count, 370);
}

#[test]
fn the_container_duration_is_used_when_the_stream_has_none() {
    let json = r#"{ "streams": [ { "codec_type": "video", "width": 640, "height": 360,
            "avg_frame_rate": "25/1", "nb_frames": "N/A", "duration": "N/A" } ],
            "format": { "duration": "4.000000" } }"#;
    assert_eq!(properties_from_probe_output(json).unwrap().total_frame_count, 100);
}

#[test]
fn a_video_with_neither_frame_count_nor_duration_is_an_error() {
    let json = r#"{ "streams": [ { "codec_type": "video", "width": 640, "height": 360, "avg_frame_rate": "25/1" } ] }"#;
    assert!(matches!(
        properties_from_probe_output(json),
        Err(ProbeError::UnknownLength)
    ));
}

#[test]
fn the_real_frame_rate_is_used_when_the_average_is_unusable() {
    let json = r#"{ "streams": [ { "codec_type": "video", "width": 640, "height": 360,
            "avg_frame_rate": "0/0", "r_frame_rate": "24000/1001", "nb_frames": "100" } ] }"#;
    assert_eq!(
        properties_from_probe_output(json).unwrap().frame_rate,
        frame_rate(24000, 1001)
    );
}

#[test]
fn a_video_with_no_usable_frame_rate_is_an_error() {
    let json = r#"{ "streams": [ { "codec_type": "video", "width": 640, "height": 360,
            "avg_frame_rate": "0/0", "r_frame_rate": "0/0", "nb_frames": "100" } ] }"#;
    assert!(matches!(
        properties_from_probe_output(json),
        Err(ProbeError::UnknownFrameRate)
    ));
}

#[test]
fn an_audio_only_file_has_no_video_stream() {
    let json = r#"{ "streams": [ { "codec_type": "audio", "sample_rate": "44100" } ] }"#;
    assert!(matches!(
        properties_from_probe_output(json),
        Err(ProbeError::NoVideoStream)
    ));
}

#[test]
fn album_art_attached_to_an_audio_file_is_not_a_video() {
    let json = r#"{ "streams": [
            { "codec_type": "audio", "sample_rate": "44100" },
            { "codec_type": "video", "width": 500, "height": 500, "avg_frame_rate": "90000/1",
              "nb_frames": "1", "disposition": { "attached_pic": 1 } } ] }"#;
    assert!(matches!(
        properties_from_probe_output(json),
        Err(ProbeError::NoVideoStream)
    ));
}

#[test]
fn the_first_real_video_stream_is_used_when_there_are_several() {
    let json = r#"{ "streams": [
            { "codec_type": "video", "width": 100, "height": 100, "avg_frame_rate": "90000/1",
              "nb_frames": "1", "disposition": { "attached_pic": 1 } },
            { "codec_type": "video", "width": 640, "height": 360, "avg_frame_rate": "25/1", "nb_frames": "25" },
            { "codec_type": "video", "width": 320, "height": 180, "avg_frame_rate": "30/1", "nb_frames": "30" } ] }"#;
    let properties = properties_from_probe_output(json).unwrap();
    assert_eq!(properties.display_width_in_pixels, 640);
}

#[test]
fn a_video_stream_without_a_size_is_not_usable() {
    let json = r#"{ "streams": [ { "codec_type": "video", "avg_frame_rate": "25/1", "nb_frames": "25" } ] }"#;
    assert!(matches!(
        properties_from_probe_output(json),
        Err(ProbeError::NoVideoStream)
    ));
}

#[test]
fn text_that_is_not_probe_output_is_reported_as_unreadable() {
    for bad_text in ["", "not json", "[1, 2]"] {
        assert!(
            matches!(
                properties_from_probe_output(bad_text),
                Err(ProbeError::UnreadableAnswer(_))
            ),
            "{bad_text:?}"
        );
    }
    // Valid JSON without any streams is simply a file with no video.
    assert!(matches!(
        properties_from_probe_output("{}"),
        Err(ProbeError::NoVideoStream)
    ));
}

// --- pictures ---

/// What ffprobe prints (trimmed) for a PNG file of 800 by 600 pixels.
const PNG_FILE: &str = r#"{
        "streams": [
            { "index": 0, "codec_type": "video", "codec_name": "png", "width": 800, "height": 600,
              "sample_aspect_ratio": "1:1", "r_frame_rate": "25/1", "avg_frame_rate": "0/0",
              "disposition": { "default": 0, "attached_pic": 0 } }
        ],
        "format": { "format_name": "png_pipe", "duration": "N/A" }
    }"#;

#[test]
fn a_png_file_is_a_picture_of_the_size_it_says() {
    assert_eq!(
        source_properties(PNG_FILE).unwrap(),
        SourceMediaProperties::Image(SourceImageProperties {
            display_width_in_pixels: 800,
            display_height_in_pixels: 600
        })
    );
}

#[test]
fn a_jpeg_file_which_ffprobe_knows_by_its_extension_is_a_picture_too() {
    let json = PNG_FILE.replace("png_pipe", "image2").replace("\"png\"", "\"mjpeg\"");
    assert!(matches!(
        source_properties(&json).unwrap(),
        SourceMediaProperties::Image(_)
    ));
    for kind in ["jpeg_pipe", "webp_pipe", "bmp_pipe", "tiff_pipe", "image2pipe"] {
        let json = PNG_FILE.replace("png_pipe", kind);
        assert!(
            matches!(source_properties(&json).unwrap(), SourceMediaProperties::Image(_)),
            "{kind}"
        );
    }
}

#[test]
fn a_video_of_a_single_picture_or_an_animated_gif_is_a_video_and_not_a_picture() {
    let one_frame_video = r#"{ "streams": [
            { "index": 0, "codec_type": "video", "width": 640, "height": 360, "avg_frame_rate": "30/1", "nb_frames": "1" } ],
            "format": { "format_name": "mov,mp4,m4a,3gp,3g2,mj2", "duration": "0.033" } }"#;
    assert!(matches!(
        source_properties(one_frame_video).unwrap(),
        SourceMediaProperties::Video(_)
    ));
    let gif = r#"{ "streams": [
            { "index": 0, "codec_type": "video", "width": 100, "height": 100, "avg_frame_rate": "10/1", "nb_frames": "20" } ],
            "format": { "format_name": "gif", "duration": "2.0" } }"#;
    assert!(matches!(
        source_properties(gif).unwrap(),
        SourceMediaProperties::Video(_)
    ));
}

#[test]
fn the_size_of_a_picture_is_the_size_it_is_shown_at_after_its_pixel_shape_and_turning() {
    let non_square = PNG_FILE.replace("\"1:1\"", "\"2:1\"");
    let SourceMediaProperties::Image(image) = source_properties(&non_square).unwrap() else {
        panic!("a picture")
    };
    assert_eq!(
        (image.display_width_in_pixels, image.display_height_in_pixels),
        (1600, 600)
    );
    let turned = PNG_FILE.replace(
        "\"disposition\"",
        "\"side_data_list\": [ { \"rotation\": -90 } ], \"disposition\"",
    );
    let SourceMediaProperties::Image(image) = source_properties(&turned).unwrap() else {
        panic!("a picture")
    };
    assert_eq!(
        (image.display_width_in_pixels, image.display_height_in_pixels),
        (600, 800)
    );
}

#[test]
fn a_picture_file_with_no_size_or_no_stream_is_an_error() {
    let no_size = r#"{ "streams": [ { "codec_type": "video" } ], "format": { "format_name": "png_pipe" } }"#;
    assert!(matches!(source_properties(no_size), Err(ProbeError::NoVideoStream)));
    let no_stream = r#"{ "streams": [], "format": { "format_name": "image2" } }"#;
    assert!(matches!(source_properties(no_stream), Err(ProbeError::NoVideoStream)));
}

// --- sound files ---

fn source_properties(probe_output_json: &str) -> Result<SourceMediaProperties, ProbeError> {
    source_properties_from_probe_output(probe_output_json, |_| Err(ProbeError::UnknownLength))
}

/// What ffprobe prints (trimmed) for a 12.5 second mono WAV file.
const SOUND_FILE: &str = r#"{
        "streams": [
            { "index": 0, "codec_type": "audio", "sample_rate": "48000", "channels": 1, "duration": "12.500000" }
        ],
        "format": { "duration": "12.500000" }
    }"#;

#[test]
fn a_file_with_only_sound_is_a_sound_file_that_lasts_as_long_as_it_says() {
    assert_eq!(
        source_properties(SOUND_FILE).unwrap(),
        SourceMediaProperties::Audio(SourceAudioProperties {
            duration_in_microseconds: 12_500_000
        })
    );
}

#[test]
fn a_sound_file_that_does_not_state_the_length_of_its_sound_has_the_length_of_the_file() {
    let json = r#"{ "streams": [ { "codec_type": "audio", "duration": "N/A" } ],
            "format": { "duration": "3.250000" } }"#;
    assert_eq!(
        source_properties(json).unwrap(),
        SourceMediaProperties::Audio(SourceAudioProperties {
            duration_in_microseconds: 3_250_000
        })
    );
}

#[test]
fn a_sound_file_whose_length_is_unknown_or_nothing_is_an_error() {
    for json in [
        r#"{ "streams": [ { "codec_type": "audio" } ] }"#,
        r#"{ "streams": [ { "codec_type": "audio", "duration": "0.000000" } ], "format": { "duration": "0" } }"#,
        r#"{ "streams": [ { "codec_type": "audio", "duration": "-4" } ] }"#,
    ] {
        assert!(
            matches!(source_properties(json), Err(ProbeError::UnknownLength)),
            "{json}"
        );
    }
}

#[test]
fn a_sound_file_with_album_art_is_a_sound_file_and_not_a_video() {
    let json = r#"{ "streams": [
            { "codec_type": "audio", "sample_rate": "44100", "duration": "200.0" },
            { "codec_type": "video", "width": 500, "height": 500, "avg_frame_rate": "90000/1",
              "nb_frames": "1", "disposition": { "attached_pic": 1 } } ] }"#;
    assert_eq!(
        source_properties(json).unwrap(),
        SourceMediaProperties::Audio(SourceAudioProperties {
            duration_in_microseconds: 200_000_000
        })
    );
}

#[test]
fn a_file_with_a_picture_is_still_a_video_whether_or_not_it_also_has_sound() {
    let properties = source_properties(ORDINARY_VIDEO_WITH_AUDIO).unwrap();
    let expected = properties_from_probe_output(ORDINARY_VIDEO_WITH_AUDIO).unwrap();
    assert_eq!(properties, SourceMediaProperties::Video(expected));
    assert!(properties.has_sound());
}

#[test]
fn a_file_with_neither_picture_nor_sound_says_so() {
    for json in [
        "{}",
        r#"{ "streams": [ { "codec_type": "subtitle" }, { "codec_type": "data" } ] }"#,
    ] {
        assert!(
            matches!(source_properties(json), Err(ProbeError::NoVideoPictureOrSound)),
            "{json}"
        );
    }
}

/// Regression guard: only a file with *no* video picture falls back to being a sound file. A
/// video that cannot be understood is reported as that, not quietly turned into its sound.
#[test]
fn a_video_that_cannot_be_understood_is_not_taken_for_a_sound_file_because_it_has_sound() {
    let json = r#"{ "streams": [
            { "codec_type": "video", "width": 640, "height": 360, "avg_frame_rate": "0/0", "r_frame_rate": "0/0",
              "nb_frames": "100" },
            { "codec_type": "audio", "duration": "5.0" } ] }"#;
    assert!(matches!(source_properties(json), Err(ProbeError::UnknownFrameRate)));
}

#[test]
fn text_that_is_not_probe_output_is_unreadable_whatever_it_is_read_as() {
    assert!(matches!(
        source_properties("not json"),
        Err(ProbeError::UnreadableAnswer(_))
    ));
}
