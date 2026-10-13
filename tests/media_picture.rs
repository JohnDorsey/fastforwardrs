//! Picture files (PNG, JPEG...) read with the real ffprobe and ffmpeg: told apart from videos, and
//! decoded into pixels.

mod support;

use std::process::Command;

use fastforwardrs::media::{decode_picture, probe_source_media};
use fastforwardrs::project::{SourceImageProperties, SourceMediaProperties};
use support::{red_and_blue_picture, scratch_directory};

/// Whether `found` is `expected` or as near as the way the test pictures are made (from a video
/// colour, by ffmpeg, which can make the red 253) allows.
fn is_about(found: [u8; 4], expected: [u8; 4]) -> bool {
    found
        .iter()
        .zip(expected)
        .all(|(found, expected)| found.abs_diff(expected) <= 4)
}

fn pixel(picture: &fastforwardrs::rgba_picture::RgbaPicture, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * picture.width_in_pixels + x) * 4) as usize;
    [
        picture.rgba_pixels[index],
        picture.rgba_pixels[index + 1],
        picture.rgba_pixels[index + 2],
        picture.rgba_pixels[index + 3],
    ]
}

#[test]
fn picture_files_of_several_kinds_are_probed_as_pictures_and_not_as_one_frame_videos() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    for (name, width, height) in [
        ("red_and_blue.png", 64, 32),
        ("red_and_blue.jpg", 80, 48),
        ("red_and_blue.bmp", 16, 8),
    ] {
        let path = red_and_blue_picture(&toolchain, folder.path(), name, width, height);
        assert_eq!(
            probe_source_media(&toolchain, &path).unwrap(),
            SourceMediaProperties::Image(SourceImageProperties {
                display_width_in_pixels: width,
                display_height_in_pixels: height
            }),
            "{name}"
        );
    }
}

#[test]
fn a_video_is_still_a_video_and_not_taken_for_a_picture() {
    let toolchain = toolchain_or_skip!();
    let video = support::counter_video(&toolchain);
    assert!(matches!(
        probe_source_media(&toolchain, &video.path).unwrap(),
        SourceMediaProperties::Video(_)
    ));
}

#[test]
fn a_picture_is_decoded_at_the_size_asked_for_with_its_pixels_where_they_belong() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let path = red_and_blue_picture(&toolchain, folder.path(), "red_and_blue.png", 64, 32);

    let whole = decode_picture(&toolchain, &path, (64, 32)).unwrap();
    assert_eq!((whole.width_in_pixels, whole.height_in_pixels), (64, 32));
    assert!(
        is_about(pixel(&whole, 5, 5), [255, 0, 0, 255]),
        "the left is red and opaque: {:?}",
        pixel(&whole, 5, 5)
    );
    assert!(
        is_about(pixel(&whole, 58, 27), [0, 0, 255, 255]),
        "the right is blue: {:?}",
        pixel(&whole, 58, 27)
    );

    let smaller = decode_picture(&toolchain, &path, (16, 8)).unwrap();
    assert_eq!((smaller.width_in_pixels, smaller.height_in_pixels), (16, 8));
    assert_eq!(smaller.rgba_pixels.len(), 16 * 8 * 4);
    assert!(
        is_about(pixel(&smaller, 1, 1), [255, 0, 0, 255]),
        "{:?}",
        pixel(&smaller, 1, 1)
    );
    assert!(
        is_about(pixel(&smaller, 14, 6), [0, 0, 255, 255]),
        "{:?}",
        pixel(&smaller, 14, 6)
    );
}

#[test]
fn a_jpeg_is_decoded_to_about_the_same_colours() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let path = red_and_blue_picture(&toolchain, folder.path(), "red_and_blue.jpg", 80, 48);
    let picture = decode_picture(&toolchain, &path, (80, 48)).unwrap();
    let [red, green, blue, opacity] = pixel(&picture, 10, 24);
    assert!(
        red > 200 && green < 60 && blue < 60 && opacity == 255,
        "{red} {green} {blue} {opacity}"
    );
    let [red, green, blue, _] = pixel(&picture, 70, 24);
    assert!(blue > 200 && red < 60 && green < 60, "{red} {green} {blue}");
}

#[test]
fn a_picture_with_clear_places_keeps_them_clear() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    // A PNG whose left half is red and opaque and whose right half has no colour at all.
    let path = folder.path().join("half_clear.png");
    let status = Command::new(toolchain.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "color=c=red@1.0:s=32x16,format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='if(lt(X,16),255,0)'",
        ])
        .args(["-frames:v", "1"])
        .arg(&path)
        .status()
        .expect("could not run ffmpeg");
    assert!(status.success());
    let picture = decode_picture(&toolchain, &path, (32, 16)).unwrap();
    assert_eq!(pixel(&picture, 4, 4)[3], 255, "opaque on the left");
    assert_eq!(pixel(&picture, 28, 4)[3], 0, "clear on the right");
}

#[test]
fn a_file_that_is_not_a_picture_is_an_error_with_what_ffmpeg_said() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let path = folder.path().join("not_a_picture.png");
    std::fs::write(&path, b"this is not a picture").unwrap();
    let error = decode_picture(&toolchain, &path, (10, 10)).unwrap_err();
    assert!(error.to_string().contains("could not be decoded"), "{error}");

    let missing = decode_picture(&toolchain, folder.path().join("nothing.png"), (10, 10)).unwrap_err();
    assert!(missing.to_string().contains("could not be decoded"), "{missing}");
}
