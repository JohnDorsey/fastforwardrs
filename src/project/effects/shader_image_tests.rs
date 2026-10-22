// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The image that a shader brings with it (phase 6.4.4): what its file may be, at the edge of every limit it has.

use std::sync::Arc;

use super::*;

fn png_of(width: u32, height: u32) -> Vec<u8> {
    crate::rgba_picture::RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: vec![7; 4 * width as usize * height as usize],
    }
    .to_png_bytes()
    .unwrap()
}

#[test]
fn a_png_is_read_with_its_pixels_straight_and_the_top_row_first() {
    let bytes = crate::rgba_picture::RgbaPicture {
        width_in_pixels: 2,
        height_in_pixels: 2,
        rgba_pixels: vec![1, 2, 3, 255, 4, 5, 6, 128, 7, 8, 9, 0, 10, 11, 12, 255],
    }
    .to_png_bytes()
    .unwrap();
    let image = ShaderImage::new("tex", bytes);
    let pixels = image.pixels().unwrap();
    assert_eq!((pixels.width_in_pixels, pixels.height_in_pixels), (2, 2));
    assert_eq!(
        pixels.rgba_pixels,
        vec![1, 2, 3, 255, 4, 5, 6, 128, 7, 8, 9, 0, 10, 11, 12, 255]
    );
    assert_eq!(image.name(), "tex");
    // Read once, and what is read is shared.
    assert!(Arc::ptr_eq(&pixels, &image.pixels().unwrap()));
    assert!(
        Arc::ptr_eq(&pixels, &image.clone().pixels().unwrap()),
        "the copies of an image share what was read"
    );
}

#[test]
fn a_jpeg_is_read_too_and_what_is_neither_is_not() {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode(
            &[200, 100, 50, 200, 100, 50, 200, 100, 50, 200, 100, 50],
            2,
            2,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    let pixels = ShaderImage::new("tex", jpeg).pixels().unwrap();
    assert_eq!((pixels.width_in_pixels, pixels.height_in_pixels), (2, 2));
    assert_eq!(
        ShaderImage::new("tex", b"GIF89a....".to_vec()).pixels().unwrap_err(),
        "the file is not a PNG or a JPEG"
    );
    assert_eq!(
        ShaderImage::new("tex", Vec::new()).pixels().unwrap_err(),
        "the file is not a PNG or a JPEG"
    );
    // A file that begins as a PNG and is cut short is not a picture, and says what is wrong.
    let mut cut = png_of(4, 4);
    cut.truncate(40);
    assert!(ShaderImage::new("tex", cut).pixels().is_err());
}

#[test]
fn a_file_of_exactly_the_most_bytes_is_read_and_one_byte_more_is_not() {
    // (Bytes after the end of a PNG are not looked at.)
    let mut at_most = png_of(2, 2);
    at_most.resize(MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER, 0);
    assert!(ShaderImage::new("tex", at_most.clone()).pixels().is_ok());
    at_most.push(0);
    let said = ShaderImage::new("tex", at_most).pixels().unwrap_err();
    assert!(said.contains("are the most"), "{said}");
}

#[test]
fn a_picture_of_exactly_the_most_pixels_across_or_down_is_read_and_one_more_is_not() {
    let most = MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER;
    assert!(ShaderImage::new("tex", png_of(most, 1)).pixels().is_ok(), "across");
    assert!(ShaderImage::new("tex", png_of(1, most)).pixels().is_ok(), "down");
    for (width, height) in [(most + 1, 1), (1, most + 1)] {
        let said = ShaderImage::new("tex", png_of(width, height)).pixels().unwrap_err();
        assert!(
            said.contains("across or down are the most"),
            "{width} by {height}: {said}"
        );
    }
}

#[test]
fn two_images_are_equal_if_they_have_the_same_name_and_the_same_bytes_and_not_if_either_differs() {
    let image = ShaderImage::new("tex", vec![1, 2, 3]);
    assert_eq!(image, ShaderImage::new("tex", vec![1, 2, 3]));
    assert_ne!(image, ShaderImage::new("map", vec![1, 2, 3]));
    assert_ne!(image, ShaderImage::new("tex", vec![1, 2, 4]));
    assert_ne!(image, ShaderImage::new("map", vec![9]));
}
