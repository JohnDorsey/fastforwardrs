use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use super::*;
use crate::project::{Angle, CanvasArea, FontSize, TextColour};

/// One renderer for all the tests, because loading the computer's fonts is slow.
fn shared_renderer() -> MutexGuard<'static, TextRenderer> {
    static RENDERER: OnceLock<Mutex<TextRenderer>> = OnceLock::new();
    RENDERER
        .get_or_init(|| Mutex::new(TextRenderer::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The shared renderer, or, on a computer that has no fonts at all (nothing could be drawn), ends the test.
macro_rules! renderer_or_skip {
    () => {{
        let renderer = shared_renderer();
        if renderer.font_family_names().is_empty() {
            eprintln!("skipped: this computer has no fonts to draw text with");
            return;
        }
        renderer
    }};
}

const PICTURE_WIDTH: u32 = 800;
const PICTURE_HEIGHT: u32 = 400;

fn text_object(text: &str, font_size_fraction: f32, area: CanvasArea) -> TextObject {
    TextObject {
        text: text.to_owned(),
        font_family: String::new(),
        font_size: FontSize::from_fraction_of_picture_height(font_size_fraction),
        colour: TextColour::new(200, 40, 10, 255),
        area,
        rotation: Angle::ZERO,
    }
}

/// A text object whose area is the top half of the picture.
fn caption(text: &str, font_size_fraction: f32) -> TextObject {
    text_object(text, font_size_fraction, CanvasArea::new(0.0, 0.0, 1.0, 0.5))
}

fn render(renderer: &mut TextRenderer, object: &TextObject) -> RgbaPicture {
    renderer.render(object, PICTURE_WIDTH, PICTURE_HEIGHT)
}

fn pixel(picture: &RgbaPicture, column: u32, row: u32) -> [u8; 4] {
    let first = (row as usize * picture.width_in_pixels as usize + column as usize) * 4;
    picture.rgba_pixels[first..first + 4].try_into().unwrap()
}

fn inked_pixels(picture: &RgbaPicture) -> usize {
    picture
        .rgba_pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
        .count()
}

/// The first and last rows that have anything drawn in them.
fn inked_rows(picture: &RgbaPicture) -> Option<(u32, u32)> {
    let inked_row = |row: u32| (0..picture.width_in_pixels).any(|column| pixel(picture, column, row)[3] > 0);
    let first = (0..picture.height_in_pixels).find(|&row| inked_row(row))?;
    let last = (0..picture.height_in_pixels).rev().find(|&row| inked_row(row))?;
    Some((first, last))
}

#[test]
fn the_picture_is_exactly_as_big_as_the_area_of_the_text_object() {
    let area = CanvasArea::new(0.1, 0.2, 0.5, 0.25);
    let object = text_object("Hi", 0.1, area);
    assert_eq!(area_size_in_pixels(&object, 1000, 800), (500, 200));
    let mut renderer = renderer_or_skip!();
    let picture = renderer.render(&object, 1000, 800);
    assert_eq!((picture.width_in_pixels, picture.height_in_pixels), (500, 200));
    assert_eq!(picture.rgba_pixels.len(), 500 * 200 * 4);
}

#[test]
fn the_picture_is_never_empty_however_small_the_area_or_the_finished_picture() {
    let object = text_object(
        "Hi",
        0.1,
        CanvasArea::new(0.0, 0.0, CanvasArea::SMALLEST_SIDE_FRACTION, 0.02),
    );
    assert_eq!(area_size_in_pixels(&object, 10, 10), (1, 1));
    assert_eq!(area_size_in_pixels(&object, 0, 0), (1, 1));
}

#[test]
fn text_with_nothing_in_it_is_a_transparent_picture() {
    let mut renderer = renderer_or_skip!();
    let picture = render(&mut renderer, &caption("", 0.1));
    assert_eq!(inked_pixels(&picture), 0);
    assert_eq!(picture.width_in_pixels, PICTURE_WIDTH);
}

#[test]
fn text_is_drawn_in_its_colour_and_the_rest_of_the_picture_stays_transparent() {
    let mut renderer = renderer_or_skip!();
    let picture = render(&mut renderer, &caption("Hello", 0.2));

    let inked = inked_pixels(&picture);
    assert!(inked > 500, "some text is drawn: {inked} pixels");
    assert!(
        inked < picture.rgba_pixels.len() / 4 / 2,
        "but most of the picture is empty: {inked} pixels"
    );
    let solid_pixels: Vec<[u8; 4]> = picture
        .rgba_pixels
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .filter(|pixel| pixel[3] == 255)
        .collect();
    assert!(solid_pixels.len() > 100, "the middle of the letters is solid");
    for pixel in solid_pixels {
        assert_eq!(&pixel[..3], &[200, 40, 10], "the colour of the text");
    }
    // The edges of the letters are the same colour, only fainter (not premultiplied: a straight-alpha picture).
    for pixel in picture
        .rgba_pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
    {
        for (found, wanted) in pixel[..3].iter().zip([200u8, 40, 10]) {
            assert!(found.abs_diff(wanted) <= 2, "{pixel:?}");
        }
    }
    assert_eq!(pixel(&picture, PICTURE_WIDTH - 1, 0)[3], 0, "the far corner is empty");
}

#[test]
fn text_starts_at_the_top_left_corner_of_its_area() {
    let mut renderer = renderer_or_skip!();
    let picture = render(&mut renderer, &caption("Hello", 0.2));
    let (first_row, _) = inked_rows(&picture).unwrap();
    assert!(first_row < 40, "the first line is at the top: row {first_row}");
    let first_column = (0..picture.width_in_pixels)
        .find(|&column| (0..picture.height_in_pixels).any(|row| pixel(&picture, column, row)[3] > 0))
        .unwrap();
    assert!(first_column < 20, "and at the left: column {first_column}");
}

#[test]
fn the_opacity_of_the_colour_is_the_opacity_of_the_text() {
    let mut renderer = renderer_or_skip!();
    let mut object = caption("Hello", 0.2);
    object.colour = TextColour::new(10, 200, 30, 128);
    let picture = render(&mut renderer, &object);
    let strongest = picture
        .rgba_pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[3])
        .max()
        .unwrap();
    assert!(
        strongest.abs_diff(128) <= 2,
        "the strongest the text gets is {strongest}"
    );
    assert!(inked_pixels(&picture) > 500);
}

#[test]
fn bigger_letters_cover_more_of_the_picture() {
    let mut renderer = renderer_or_skip!();
    let small = inked_pixels(&render(&mut renderer, &caption("Hello", 0.1)));
    let big = inked_pixels(&render(&mut renderer, &caption("Hello", 0.2)));
    assert!(
        big > small * 2,
        "twice as tall letters make about four times the ink: {small} then {big}"
    );
}

#[test]
fn text_that_is_too_long_for_a_line_goes_on_to_the_next_line_inside_the_area() {
    let mut renderer = renderer_or_skip!();
    let long_text = "a good many words that will not fit side by side in a box this wide";
    let object = text_object(long_text, 0.1, CanvasArea::new(0.0, 0.0, 0.5, 0.5));
    let picture = render(&mut renderer, &object);
    assert_eq!(picture.width_in_pixels, 400);
    let (first_row, last_row) = inked_rows(&picture).unwrap();
    let letter_size = 40; // 0.1 of the picture's 400 pixels
    assert!(
        last_row - first_row > letter_size * 3 / 2,
        "more than one line is drawn: rows {first_row} to {last_row}"
    );
    assert!(last_row < picture.height_in_pixels, "and all of it is inside the area");
}

#[test]
fn a_line_that_does_not_fit_the_height_of_the_area_is_cut_off() {
    let mut renderer = renderer_or_skip!();
    let long_text = "a good many words ".repeat(30);
    let object = text_object(&long_text, 0.1, CanvasArea::new(0.0, 0.0, 0.5, 0.25));
    let picture = render(&mut renderer, &object);
    assert_eq!(picture.height_in_pixels, 100);
    let (_, last_row) = inked_rows(&picture).unwrap();
    assert!(last_row < 100);
}

#[test]
fn letters_with_accents_are_drawn() {
    let mut renderer = renderer_or_skip!();
    assert!(inked_pixels(&render(&mut renderer, &caption("é", 0.3))) > 300);
}

#[test]
fn letters_that_no_font_has_do_not_stop_the_rest_being_drawn() {
    let mut renderer = renderer_or_skip!();
    let picture = render(&mut renderer, &caption("Ab \u{10FFFD} 你好 😀", 0.2));
    assert!(
        inked_pixels(&picture) > 500,
        "at least the letters that can be drawn are"
    );
}

#[test]
fn a_font_the_computer_does_not_have_is_replaced_by_another() {
    let mut renderer = renderer_or_skip!();
    let mut object = caption("Hello", 0.2);
    object.font_family = "No Such Font Family 7421".to_owned();
    assert!(inked_pixels(&render(&mut renderer, &object)) > 500);
}

#[test]
fn a_font_that_the_computer_has_can_be_chosen_by_name() {
    let mut renderer = renderer_or_skip!();
    let families = renderer.font_family_names();
    let mut object = caption("Hello", 0.2);
    object.font_family = families[0].clone();
    // Whatever it looks like, it is drawn (a symbol font may draw something else than letters).
    let _ = render(&mut renderer, &object);
}

#[test]
fn the_font_families_are_in_alphabetical_order_and_each_is_listed_once() {
    let renderer = renderer_or_skip!();
    let names = renderer.font_family_names();
    let keys: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
    assert!(keys.windows(2).all(|pair| pair[0] <= pair[1]), "{names:?}");
    let mut without_repeats = names.clone();
    without_repeats.dedup();
    assert_eq!(names, without_repeats);
}

// --- laying colour over a pixel ---

#[test]
fn a_colour_laid_over_nothing_is_that_colour_with_that_opacity() {
    let mut pixel = [0, 0, 0, 0];
    lay_over(&mut pixel, [10, 20, 30], 100);
    assert_eq!(pixel, [10, 20, 30, 100]);
}

#[test]
fn a_colour_laid_over_a_solid_pixel_is_mixed_in_by_its_opacity() {
    let mut pixel = [255, 0, 0, 255];
    lay_over(&mut pixel, [0, 0, 255], 128);
    assert_eq!(pixel[3], 255);
    assert!(pixel[0].abs_diff(127) <= 1 && pixel[2].abs_diff(128) <= 1, "{pixel:?}");
}

#[test]
fn two_half_layers_make_three_quarters() {
    let mut pixel = [0, 0, 0, 0];
    lay_over(&mut pixel, [50, 60, 70], 128);
    lay_over(&mut pixel, [50, 60, 70], 128);
    assert!(pixel[3].abs_diff(192) <= 1, "{pixel:?}");
    assert_eq!(&pixel[..3], &[50, 60, 70], "the colour stays the same");
}

#[test]
fn a_colour_with_no_opacity_changes_nothing() {
    let mut pixel = [1, 2, 3, 4];
    lay_over(&mut pixel, [200, 200, 200], 0);
    assert_eq!(pixel, [1, 2, 3, 4]);
}

#[test]
fn how_much_of_a_pixel_a_letter_covers_is_scaled_by_the_opacity_of_its_colour() {
    assert_eq!(opacity_of_covering(255, 255), 255);
    assert_eq!(opacity_of_covering(255, 128), 128);
    assert_eq!(opacity_of_covering(128, 128), 64);
    assert_eq!(opacity_of_covering(0, 255), 0);
    assert_eq!(
        opacity_of_covering(255, 0),
        0,
        "text with a transparent colour is not seen"
    );
    assert_eq!(opacity_of_covering(1, 1), 0);
}

// --- the renderer that loads in the background ---

#[test]
fn the_fonts_load_in_the_background_when_first_wanted_and_the_window_is_woken_when_they_are_ready() {
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    let mut lazy = LazyTextRenderer::new(Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    }));
    assert!(!lazy.is_ready(), "nothing is loaded before it is wanted");

    let _ = lazy.get_if_ready(); // the first request starts the loading
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while lazy.get_if_ready().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "the fonts did not load in a minute"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(lazy.is_ready());
    assert_eq!(woken.load(Ordering::SeqCst), 1, "woken exactly once, when loaded");
}

#[test]
fn waiting_for_the_fonts_gives_a_renderer_that_works() {
    let mut lazy = LazyTextRenderer::new(Arc::new(|| {}));
    let renderer = lazy.get_waiting_if_needed().expect("the fonts load");
    let picture = renderer.render(&caption("", 0.1), 100, 100);
    assert_eq!(picture.width_in_pixels, 100);
    assert!(lazy.is_ready());
}
