// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colours and sizes of the editor, in one place.

use egui::Color32;

/// How tall the row above the tracks is, in points. The ruler is in it, over the strips, and so is
/// the round button that adds a track, over the title boxes.
pub const TOP_ROW_HEIGHT: f32 = 40.0;

/// How wide the box at the left of a track, with the track's title and settings button, is, in points.
pub const TRACK_HEADER_WIDTH: f32 = 150.0;

/// How much room there is between that box and the strip of the track, and between one track and the next, in points.
pub const TRACK_HEADER_GAP: f32 = 8.0;
pub const SPACE_BETWEEN_TRACKS: f32 = 6.0;

/// How many tracks are shown at once, if there are that many: more are reached with the scroll bar.
pub const MOST_TRACKS_SHOWN_AT_ONCE: usize = 3;

/// How much room is left empty at the right of the ruler and the strips, for the scroll bar of the
/// rows to be in (it is always where it is, so that the strips do not change width when it comes), in points.
pub const SCROLL_BAR_ROOM: f32 = 12.0;

/// How much the corners of the clips on the tracks are rounded, in points. (A clip too narrow for
/// this much rounding is rounded as far as it can be, which is by half its width.)
pub const CLIP_CORNER_RADIUS: u8 = 27;

/// How tall a rack of keyframes is in a drawer, in points, how much room there is between two of them, and how much room
/// there is above the first.
pub const RACK_HEIGHT: f32 = 16.0;
pub const SPACE_BETWEEN_RACKS: f32 = 4.0;
pub const SPACE_ABOVE_THE_RACKS: f32 = 8.0;

/// How much room is left under the bottom of a drawer that the rows are scrolled to bring into view, in points.
pub const MARGIN_UNDER_A_DRAWER_BROUGHT_INTO_VIEW: f32 = 8.0;

/// How far the racks in a keyframe crate keep from the top and the bottom of it when it is tall enough for them to be in
/// the middle, and how little room there is above the first when it is not (the crate then reaches below the strip, and
/// has this much room under the last).
pub const CRATE_MARGIN: f32 = 18.0;
pub const CRATE_LEAST_MARGIN: f32 = 6.0;

/// What a rack of keyframes is filled with where it has no grain (at its round ends), which is about what the grain is
/// on average: a very dark grey.
pub const RACK_BACKGROUND: Color32 = Color32::from_rgb(20, 20, 22);

/// The label that comes to the left of a rack of keyframes when the pointer is over it, and says what it animates.
pub const RACK_LABEL_BACKGROUND: Color32 = Color32::from_rgba_premultiplied(20, 21, 24, 235);
pub const RACK_LABEL_TEXT: Color32 = Color32::from_rgb(238, 240, 244);

/// How wide one bar of the waveform of a sound clip is, in points.
pub const WAVEFORM_COLUMN_WIDTH: f64 = 2.0;

/// How tall the slider along the bottom is, in points.
pub const SCRUB_SLIDER_HEIGHT: f32 = 30.0;

/// The space around and between the parts of the window, in points.
pub const SPACING: f32 = 8.0;

/// How far the panels are from the edges of the window and from one another, in points.
pub const PANEL_GAP: i8 = 6;

/// How wide the properties panel is, in points, with the space that is round it.
pub const PROPERTIES_PANEL_WIDTH: f32 = 340.0;

/// How tall the thumbnail pictures are made, in pixels. They are shown about 104 points tall, and
/// this leaves room for screens that draw two pixels to a point.
pub const THUMBNAIL_HEIGHT_IN_PIXELS: u32 = 144;

/// What the panels are (and the menu bar): the window is divided into panels, with [`PANEL_GAP`] of the darker
/// [`GAP_BACKGROUND`] between them and round them.
pub const PANEL_BACKGROUND: Color32 = Color32::from_rgb(24, 25, 28);
pub const GAP_BACKGROUND: Color32 = Color32::from_rgb(12, 12, 14);
pub const TRACK_BACKGROUND: Color32 = Color32::from_rgb(34, 36, 40);
pub const TRACK_BORDER: Color32 = Color32::from_rgb(80, 84, 92);
/// What a thumbnail looks like until its picture has been made.
pub const THUMBNAIL_PLACEHOLDER: Color32 = Color32::from_rgb(48, 52, 58);
pub const CLIP_BOUNDARY_LIGHT: Color32 = Color32::from_rgb(235, 235, 235);
pub const CLIP_BOUNDARY_DARK: Color32 = Color32::from_rgb(12, 12, 14);
pub const PLAYHEAD: Color32 = Color32::from_rgb(255, 92, 72);
/// Drawn around the playhead, so that it can be seen against a picture of any colour (even its own).
pub const PLAYHEAD_OUTLINE: Color32 = Color32::from_rgba_premultiplied(6, 6, 8, 210);

/// The ruler above the tracks.
pub const RULER_BACKGROUND: Color32 = Color32::from_rgb(30, 32, 36);
pub const RULER_TICK: Color32 = Color32::from_rgb(150, 154, 162);
pub const RULER_LABEL: Color32 = Color32::from_rgb(176, 180, 188);

/// The outline around the selected clip.
pub const SELECTION_OUTLINE: Color32 = Color32::from_rgb(255, 204, 51);
/// The rectangle drawn round clips to select them.
pub const RUBBER_BAND_FILL: Color32 = Color32::from_rgba_premultiplied(40, 31, 8, 40);

/// A sound clip: light magenta waveform on a dark, desaturated blue background.
pub const SOUND_CLIP_BACKGROUND: Color32 = Color32::from_rgb(38, 48, 74);
pub const SOUND_CLIP_WAVEFORM: Color32 = Color32::from_rgb(240, 150, 228);

/// A clip with a text object in it.
pub const TEXT_CLIP_BACKGROUND: Color32 = Color32::from_rgb(38, 94, 104);
pub const TEXT_CLIP_TEXT: Color32 = Color32::from_rgb(236, 246, 246);

/// A clip with a picture in it: its file's name beside the picture, on a muted violet background.
pub const IMAGE_CLIP_BACKGROUND: Color32 = Color32::from_rgb(70, 52, 104);
pub const IMAGE_CLIP_TEXT: Color32 = Color32::from_rgb(240, 234, 250);

/// An adjustment clip: its name on a muted olive, which no other kind of clip is.
pub const ADJUSTMENT_CLIP_BACKGROUND: Color32 = Color32::from_rgb(100, 84, 36);
pub const ADJUSTMENT_CLIP_TEXT: Color32 = Color32::from_rgb(250, 244, 224);

/// A generator clip: its name on a deep teal, which no other kind of clip is.
pub const GENERATOR_CLIP_BACKGROUND: Color32 = Color32::from_rgb(24, 92, 96);
pub const GENERATOR_CLIP_TEXT: Color32 = Color32::from_rgb(226, 246, 244);

/// A keyframe crate: a box that is nothing but a drawer that is always open, a little darker than the strip it is on.
pub const CRATE_CLIP_BACKGROUND: Color32 = Color32::from_rgb(24, 26, 31);

/// A clip whose file cannot be found.
pub const MISSING_CLIP_BACKGROUND: Color32 = Color32::from_rgb(82, 42, 46);
pub const MISSING_CLIP_TEXT: Color32 = Color32::from_rgb(232, 196, 198);

/// The box at the left of a track, with its title.
pub const TRACK_HEADER_BACKGROUND: Color32 = Color32::from_rgb(40, 43, 49);

/// What a drawer on a clip is filled with: a grey that is plainly lighter than the strip it hangs from and than the box of
/// the track beside it (and so than the racks in it, which are dark), so that the drawers stand out from what is round them. And
/// the button that opens it (which is over a clip, whatever it looks like, so it dims what is behind it).
pub const DRAWER_BACKGROUND: Color32 = Color32::from_rgb(62, 66, 74);
pub const DRAWER_BUTTON_FILL: Color32 = Color32::from_black_alpha(150);
pub const DRAWER_BUTTON_FILL_HOVERED: Color32 = Color32::from_black_alpha(205);
pub const DRAWER_BUTTON_OUTLINE: Color32 = Color32::from_rgba_premultiplied(70, 70, 70, 70);
pub const DRAWER_BUTTON_OUTLINE_HOVERED: Color32 = Color32::from_rgba_premultiplied(150, 150, 150, 150);
pub const DRAWER_BUTTON_ARROW: Color32 = Color32::from_rgba_premultiplied(225, 225, 225, 225);

/// The box in the properties panel that has the properties of one thing in it: a little lighter than the panel.
pub const PROPERTIES_BOX_BACKGROUND: Color32 = Color32::from_rgb(38, 40, 46);

/// The badge of the track that a clip is on, in the chain of effects of the clip: a grey, which no group is (the badges of groups are in
/// the colours of the groups), with light writing.
pub const TRACK_BADGE_BACKGROUND: Color32 = Color32::from_rgb(98, 102, 110);
pub const TRACK_BADGE_TEXT: Color32 = Color32::from_rgb(244, 245, 247);

/// What is drawn over a box or a badge of a chain of effects while it is being dragged, which dims it, and the line that shows where it will
/// go when it is let go of (the colour that shows what is selected).
pub const BEING_DRAGGED_VEIL: Color32 = Color32::from_black_alpha(110);
pub const WHERE_IT_WILL_GO: Color32 = SELECTION_OUTLINE;

pub const SLIDER_TRACK: Color32 = Color32::from_rgb(70, 74, 82);
pub const SLIDER_FILLED: Color32 = Color32::from_rgb(214, 88, 70);
pub const SLIDER_KNOB: Color32 = Color32::from_rgb(240, 240, 240);

pub const ICON: Color32 = Color32::from_rgb(232, 232, 236);
pub const ICON_DISABLED: Color32 = Color32::from_rgb(110, 112, 118);
pub const ICON_BUTTON_HOVER: Color32 = Color32::from_rgb(60, 64, 72);
pub const ICON_BUTTON_PRESSED: Color32 = Color32::from_rgb(84, 90, 102);
pub const ICON_BUTTON_ACTIVE: Color32 = Color32::from_rgb(52, 90, 140);

/// The buttons of the keyframes in the properties panel (the stopwatch and the arrows to the keyframes): bright orange for one
/// that has a keyframe (or has one to go to), and grey for one that has not.
pub const KEYFRAME_BUTTON_ACTIVE: Color32 = Color32::from_rgb(255, 150, 24);
pub const KEYFRAME_BUTTON_INACTIVE: Color32 = Color32::from_rgb(116, 120, 128);
pub const KEYFRAME_BUTTON_HOVERED_BACKGROUND: Color32 = Color32::from_rgba_premultiplied(30, 30, 30, 30);

/// The outline shown around the fast-forward symbol while the pointer is over it.
pub const INDICATOR_OUTLINE: Color32 = Color32::from_rgb(120, 190, 255);

#[cfg(test)]
mod tests {
    use super::*;

    /// The hue in degrees, the saturation and the lightness (both from 0 to 1) of a colour.
    fn hue_saturation_lightness(colour: Color32) -> (f32, f32, f32) {
        let [red, green, blue] = [colour.r(), colour.g(), colour.b()].map(|channel| f32::from(channel) / 255.0);
        let (largest, smallest) = (red.max(green).max(blue), red.min(green).min(blue));
        let lightness = (largest + smallest) / 2.0;
        let spread = largest - smallest;
        if spread == 0.0 {
            return (0.0, 0.0, lightness);
        }
        let saturation = spread / (1.0 - (2.0 * lightness - 1.0).abs());
        let hue = if largest == red {
            60.0 * ((green - blue) / spread).rem_euclid(6.0)
        } else if largest == green {
            60.0 * ((blue - red) / spread + 2.0)
        } else {
            60.0 * ((red - green) / spread + 4.0)
        };
        (hue, saturation, lightness)
    }

    /// How much brighter the lighter of two colours is than the darker, as accessibility guidelines count it.
    fn contrast_ratio(first: Color32, second: Color32) -> f32 {
        let luminance = |colour: Color32| {
            let linear = |channel: u8| {
                let value = f32::from(channel) / 255.0;
                if value <= 0.03928 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(colour.r()) + 0.7152 * linear(colour.g()) + 0.0722 * linear(colour.b())
        };
        let (lighter, darker) = (
            luminance(first).max(luminance(second)),
            luminance(first).min(luminance(second)),
        );
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn the_name_on_a_picture_clip_and_the_words_of_a_text_clip_can_be_read() {
        assert!(contrast_ratio(IMAGE_CLIP_BACKGROUND, IMAGE_CLIP_TEXT) > 7.0);
        assert!(contrast_ratio(TEXT_CLIP_BACKGROUND, TEXT_CLIP_TEXT) > 4.5);
        assert!(contrast_ratio(ADJUSTMENT_CLIP_BACKGROUND, ADJUSTMENT_CLIP_TEXT) > 4.5);
        assert!(contrast_ratio(GENERATOR_CLIP_BACKGROUND, GENERATOR_CLIP_TEXT) > 4.5);
    }

    #[test]
    fn a_drawer_is_a_lighter_grey_than_the_strip_it_hangs_from_the_box_of_the_track_and_the_racks_in_it() {
        let grey = |colour: Color32| {
            let (_, saturation, lightness) = hue_saturation_lightness(colour);
            assert!(saturation < 0.15, "a grey, not a colour: {colour:?}");
            lightness
        };
        let drawer = grey(DRAWER_BACKGROUND);
        assert!(drawer > grey(TRACK_BACKGROUND) + 0.06, "lighter than the strip");
        assert!(
            drawer > grey(TRACK_HEADER_BACKGROUND) + 0.04,
            "and than the box of the track"
        );
        assert!(
            drawer > grey(RACK_BACKGROUND) + 0.15,
            "far lighter than the racks, which are dark in it"
        );
        assert!(
            drawer < 0.4,
            "still a dark grey, as the rest of the window is: {drawer}"
        );
        assert!(
            contrast_ratio(DRAWER_BACKGROUND, RACK_BACKGROUND) > 1.6,
            "the racks can be told from the drawer"
        );
    }

    #[test]
    fn the_writing_on_the_badge_of_a_track_can_be_read_and_the_badge_is_not_a_colour_that_a_group_can_be() {
        assert!(contrast_ratio(TRACK_BADGE_BACKGROUND, TRACK_BADGE_TEXT) > 4.5);
        let (_, saturation, _) = hue_saturation_lightness(TRACK_BADGE_BACKGROUND);
        assert!(saturation < 0.15, "a grey, not a colour: {saturation}");
        assert!(
            contrast_ratio(TRACK_BADGE_BACKGROUND, PROPERTIES_BOX_BACKGROUND) > 1.8,
            "it can be told from the box it is under"
        );
    }

    #[test]
    fn the_kinds_of_clip_that_are_not_pictures_of_a_video_have_their_own_colours() {
        let colours = [
            SOUND_CLIP_BACKGROUND,
            TEXT_CLIP_BACKGROUND,
            IMAGE_CLIP_BACKGROUND,
            MISSING_CLIP_BACKGROUND,
            CRATE_CLIP_BACKGROUND,
            ADJUSTMENT_CLIP_BACKGROUND,
            GENERATOR_CLIP_BACKGROUND,
        ];
        for (index, first) in colours.iter().enumerate() {
            for second in &colours[index + 1..] {
                assert_ne!(first, second);
            }
        }
    }

    #[test]
    fn a_sound_clip_is_a_dark_desaturated_blue_with_a_light_magenta_waveform_that_stands_out() {
        let (hue, saturation, lightness) = hue_saturation_lightness(SOUND_CLIP_BACKGROUND);
        assert!((200.0..=250.0).contains(&hue), "blue, not {hue} degrees");
        assert!(lightness < 0.3, "dark, not {lightness}");
        assert!((0.15..=0.5).contains(&saturation), "desaturated, not {saturation}");

        let (hue, saturation, lightness) = hue_saturation_lightness(SOUND_CLIP_WAVEFORM);
        assert!((285.0..=325.0).contains(&hue), "magenta, not {hue} degrees");
        assert!(lightness > 0.65, "light, not {lightness}");
        assert!(saturation > 0.4, "still a colour, not {saturation}");

        assert!(
            contrast_ratio(SOUND_CLIP_BACKGROUND, SOUND_CLIP_WAVEFORM) > 6.0,
            "the waveform can be seen"
        );
    }
}
