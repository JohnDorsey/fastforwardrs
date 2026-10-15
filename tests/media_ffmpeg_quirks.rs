// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What ffmpeg does that is not the program's own, and that is not the same on every system. CI runs these as **warnings** with the names of the ones that failed
//! (a failure here says something about the ffmpeg of that system, which the program cannot mend, and does not make the build red).
//!
//! **Found by the first CI run of the tests of video pictures in planes (2026-10-15):** on macOS, with the ffmpeg that Homebrew installs, the pictures that ffmpeg
//! makes of a video in RGBA are right at the sizes that are even across and down and wrong at the others (a difference of up to 255 from the arithmetic of the
//! conversion, and pixels that are not opaque). The preview always asked ffmpeg for RGBA (`format=rgba`), so on that system a video whose preview picture is of an
//! odd size (a portrait video of 1080 by 1920 has a preview picture 405 wide) was drawn with a part that is clear, until the engine takes the pictures of
//! 8-bit 4:2:0 video in planes and makes the colours itself. What is known of it, and what is not, is said by the test's failure.

mod support;

use fastforwardrs::media::DecodedFrame;
use support::scratch_directory;
use support::yuv_videos::{FOUR_COLOURS, UNTAGGED, picture_as_rgba, video_made_by_geq};

/// Where the pixels of `frame` that are not opaque are, as words, or `None` if every pixel is opaque.
fn where_the_pixels_are_not_opaque(frame: &DecodedFrame) -> Option<String> {
    let width = frame.width_in_pixels as usize;
    let (mut columns, mut rows, mut count) = (
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
        0usize,
    );
    let mut with_no_colour = 0usize;
    for (number, pixel) in frame.rgba_pixels.as_chunks::<4>().0.iter().enumerate() {
        if pixel[3] != 255 {
            columns.insert(number % width);
            rows.insert(number / width);
            count += 1;
            with_no_colour += usize::from(pixel[..3] == [0, 0, 0]);
        }
    }
    if count == 0 {
        return None;
    }
    let describe = |places: &std::collections::BTreeSet<usize>, name: &str| {
        let (first, last) = (places.first().unwrap(), places.last().unwrap());
        format!("{} {name}s ({first} to {last})", places.len())
    };
    Some(format!(
        "{count} of {} pixels are not opaque, in {} and {}; {with_no_colour} of them have no colour either",
        frame.rgba_pixels.len() / 4,
        describe(&columns, "column"),
        describe(&rows, "row"),
    ))
}

#[test]
fn ffmpeg_makes_pictures_that_are_opaque_of_every_size_even_and_odd() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let mut found = Vec::new();
    // (the size of the video, the size of the picture it is delivered for): unscaled, and scaled down to a size that is odd across or down or both.
    for (video_size, picture_size) in [
        ((64, 48), (64, 48)),
        ((65, 48), (65, 48)),
        ((64, 49), (64, 49)),
        ((65, 49), (65, 49)),
        ((33, 17), (33, 17)),
        ((100, 60), (50, 30)),
        ((130, 98), (65, 49)),
        ((96, 64), (45, 30)),
        ((64, 96), (21, 32)),
    ] {
        let video = video_made_by_geq(
            &toolchain,
            folder.path(),
            &format!("quirk_{}x{}", video_size.0, video_size.1),
            video_size,
            FOUR_COLOURS,
            "yuv420p",
            UNTAGGED,
        );
        let frame = picture_as_rgba(&toolchain, &video, video_size, picture_size);
        if let Some(words) = where_the_pixels_are_not_opaque(&frame) {
            found.push(format!(
                "a video of {}x{} made into a picture of {}x{}: {words}",
                video_size.0, video_size.1, frame.width_in_pixels, frame.height_in_pixels
            ));
        }
    }
    assert!(
        found.is_empty(),
        "this ffmpeg makes pictures that have pixels which are not opaque, and a pixel that is not opaque shows nothing on the preview:\n{}",
        found.join("\n")
    );
}
