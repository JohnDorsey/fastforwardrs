// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colour of the quieter text of the plugin manager. egui's own "weak" text is the text colour at half strength, which on the background of a window is about 3 to 1 against it and is
//! hard to read; the quieter text here is **as quiet as it can be and still be 4.5 to 1 against the background** (the least that the guidelines for text on a screen ask for), so that the
//! window has two brightnesses of text besides the strong one, and neither is hard to read.

use egui::{Color32, RichText, Ui};

/// The least contrast (the ratio of the luminances of the lighter and the darker, each plus 0.05) that the quieter text has against the background.
pub const LEAST_CONTRAST: f32 = 4.5;

/// The luminance of a colour, 0 for black and 1 for white (the formula of WCAG 2).
fn luminance(colour: Color32) -> f32 {
    let linear = |byte: u8| {
        let value = f32::from(byte) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(colour.r()) + 0.7152 * linear(colour.g()) + 0.0722 * linear(colour.b())
}

/// How much more luminous the lighter of two colours is than the darker, as a ratio.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn mixed(from: Color32, to: Color32, towards_to: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * towards_to).round() as u8;
    Color32::from_rgb(mix(from.r(), to.r()), mix(from.g(), to.g()), mix(from.b(), to.b()))
}

/// The colour of the quieter text, given the colour of the text and of the background it is on: the text moved towards the background as far as it can be moved with the contrast still
/// [`LEAST_CONTRAST`]. If the text itself has less than that, it is the text (nothing is made quieter than it).
pub fn quieter_text_colour(text: Color32, background: Color32) -> Color32 {
    let text = text.to_opaque();
    let background = background.to_opaque();
    if contrast(text, background) <= LEAST_CONTRAST {
        return text;
    }
    // (The contrast falls as the mix goes towards the background: look for the last mix that keeps it.)
    let (mut kept, mut lost) = (0.0_f32, 1.0_f32);
    for _ in 0..16 {
        let middle = (kept + lost) / 2.0;
        if contrast(mixed(text, background, middle), background) >= LEAST_CONTRAST {
            kept = middle;
        } else {
            lost = middle;
        }
    }
    mixed(text, background, kept)
}

/// `text` in the quieter colour, for the background of a window.
pub fn quieter(ui: &Ui, text: impl Into<String>) -> RichText {
    let visuals = ui.visuals();
    RichText::new(text).color(quieter_text_colour(visuals.text_color(), visuals.window_fill))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quieter_text_is_readable_against_the_background_of_a_dark_window_and_of_a_light_one_and_is_quieter_than_the_text()
     {
        for visuals in [egui::Visuals::dark(), egui::Visuals::light()] {
            let (text, background) = (visuals.text_color(), visuals.window_fill);

            let quieter = quieter_text_colour(text, background);

            assert!(
                contrast(quieter, background) >= LEAST_CONTRAST - 0.01,
                "{quieter:?} on {background:?}: {}",
                contrast(quieter, background)
            );
            assert!(contrast(quieter, background) <= contrast(text, background) + 0.01);
        }
    }

    #[test]
    fn egui_s_weak_text_is_the_hard_to_read_text_that_this_replaces() {
        let visuals = egui::Visuals::dark();
        let weak = visuals.weak_text_color();

        assert!(
            contrast(weak, visuals.window_fill) < LEAST_CONTRAST,
            "weak text has {}",
            contrast(weak, visuals.window_fill)
        );
    }

    #[test]
    fn text_that_is_no_more_readable_than_the_least_is_left_as_it_is_and_black_on_white_is_made_grey() {
        let grey = Color32::from_gray(120);
        let background = Color32::from_gray(100);
        assert_eq!(quieter_text_colour(grey, background), grey);

        let quieter = quieter_text_colour(Color32::BLACK, Color32::WHITE);
        assert_ne!(quieter, Color32::BLACK);
        assert!(contrast(quieter, Color32::WHITE) >= LEAST_CONTRAST - 0.01);
        assert!(contrast(quieter, Color32::WHITE) < 6.0, "it is as quiet as it may be");
    }

    #[test]
    fn contrast_is_21_for_black_on_white_and_1_for_a_colour_on_itself() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(Color32::from_gray(77), Color32::from_gray(77)) - 1.0).abs() < 1e-6);
    }
}
