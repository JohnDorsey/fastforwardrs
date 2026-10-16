// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pictures that show what the colours of the engine look like beside ffmpeg's at the hard edges of colours, which is where they differ most: a video that has
//! a picture of colours with hard edges is decoded the way the preview always did it (ffmpeg makes the red, green and blue) and the way the engine does it (the
//! planes that ffmpeg decoded, made into colours on the graphics card by the arithmetic of the video standards, and every colour difference given to the four
//! pixels it stands for). Three panels of each picture, made three times as big as they are so that a pixel can be seen: ffmpeg's, the engine's, and what is
//! different between them (the biggest difference of any number of a pixel, made twice as big, in grey).
//!
//! They are written by a test that is marked to be ignored (it writes files into `docs/engine-comparison` and needs ffmpeg 9):
//!
//! ```text
//! cargo test --test engine_chroma_pictures -- --ignored
//! ```
//!
//! What the differences come to in numbers is measured by `media_yuv_delivery`, and written in `docs/engine-phase-1-design.md`, section 9.

mod support;

use std::path::{Path, PathBuf};

use fastforwardrs::engine::{YuvColour, rgba_of_yuv420p_planes};
use fastforwardrs::media::{FfmpegToolchain, probe_video_stream_format};
use fastforwardrs::playback::colour_for_the_engine;
use support::scratch_directory;
use support::yuv_videos::{
    BT709_LIMITED, FOUR_COLOURS, HARD_CHROMA_STRIPES, picture_as_rgba, picture_in_planes, video_made_by_geq,
};

/// How many times as big as they are the pictures are made.
const MADE_BIGGER: u32 = 3;

/// How many times as big as it is the difference is made.
const DIFFERENCE_MADE_BIGGER: u8 = 2;

const SPACE_BETWEEN_THE_PANELS: u32 = 6;

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("engine-comparison")
}

/// What the preview makes of a video it decodes in planes: the colour comes from what ffprobe says of the video, as in the program.
fn colour_the_preview_gives(toolchain: &FfmpegToolchain, video: &Path) -> YuvColour {
    let format = probe_video_stream_format(toolchain, video.to_path_buf()).expect("the video can be probed");
    // (The pictures of this test are small: standard definition, whatever their matrix.)
    colour_for_the_engine(
        format
            .as_native_yuv420p(64, 48)
            .expect("the preview can use the planes of this video"),
    )
}

#[test]
#[ignore = "writes the pictures into docs/engine-comparison, and needs ffmpeg 9"]
fn write_the_pictures_of_the_edges_of_colours() {
    let toolchain = toolchain_or_skip!();
    let folder_for_the_videos = scratch_directory();
    std::fs::create_dir_all(folder()).unwrap();
    for (name, video_size, picture_size, expressions) in [
        (
            "06-edges-of-colours-at-the-size-of-the-video",
            (128, 72),
            (128, 72),
            HARD_CHROMA_STRIPES,
        ),
        (
            "07-edges-of-colours-at-half-the-size-of-the-video",
            (256, 144),
            (128, 72),
            HARD_CHROMA_STRIPES,
        ),
        (
            "08-edges-of-colours-at-two-thirds-of-the-size-of-the-video",
            (192, 108),
            (128, 72),
            HARD_CHROMA_STRIPES,
        ),
        (
            "09-colours-that-change-smoothly-at-two-thirds-of-the-size-of-the-video",
            (192, 108),
            (128, 72),
            FOUR_COLOURS,
        ),
    ] {
        let video = video_made_by_geq(
            &toolchain,
            folder_for_the_videos.path(),
            name,
            video_size,
            expressions,
            "yuv420p",
            BT709_LIMITED,
        );
        let colour = colour_the_preview_gives(&toolchain, &video);
        let theirs = picture_as_rgba(&toolchain, &video, video_size, picture_size);
        let planes = picture_in_planes(&toolchain, &video, video_size, picture_size);
        assert_eq!((theirs.width_in_pixels, theirs.height_in_pixels), picture_size);
        let ours = rgba_of_yuv420p_planes(
            planes.width_in_pixels,
            planes.height_in_pixels,
            &planes.y,
            &planes.u,
            &planes.v,
            colour,
        );

        let (width, height) = picture_size;
        let panel_width = width * MADE_BIGGER;
        let mut panels = image::RgbaImage::from_pixel(
            3 * panel_width + 2 * SPACE_BETWEEN_THE_PANELS,
            height * MADE_BIGGER,
            image::Rgba([90, 90, 90, 255]),
        );
        let (mut largest, mut sum, mut over_ten) = (0u8, 0u64, 0usize);
        for down in 0..height {
            for across in 0..width {
                let place = ((down * width + across) * 4) as usize;
                let (theirs, ours) = (&theirs.rgba_pixels[place..place + 4], &ours[place..place + 4]);
                let difference = (0..3)
                    .map(|channel| theirs[channel].abs_diff(ours[channel]))
                    .max()
                    .unwrap();
                largest = largest.max(difference);
                sum += u64::from(difference);
                over_ten += usize::from(difference > 10);
                let shown = difference.saturating_mul(DIFFERENCE_MADE_BIGGER);
                for (panel, pixel) in [
                    (0, [theirs[0], theirs[1], theirs[2], 255]),
                    (1, [ours[0], ours[1], ours[2], 255]),
                    (2, [shown, shown, shown, 255]),
                ] {
                    for up in 0..MADE_BIGGER {
                        for along in 0..MADE_BIGGER {
                            panels.put_pixel(
                                panel * (panel_width + SPACE_BETWEEN_THE_PANELS) + across * MADE_BIGGER + along,
                                down * MADE_BIGGER + up,
                                image::Rgba(pixel),
                            );
                        }
                    }
                }
            }
        }
        panels.save(folder().join(format!("{name}.png"))).unwrap();
        println!(
            "{name}: the largest difference of any colour of a pixel is {largest} (of 255), {:.2} on average, and {over_ten} of the {} pixels differ by more than 10",
            sum as f64 / (width * height) as f64,
            width * height
        );
    }
}
