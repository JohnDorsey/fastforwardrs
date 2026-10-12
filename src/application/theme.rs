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

/// How wide across the round button that adds a track is, in points.
pub const ADD_TRACK_BUTTON_DIAMETER: f32 = 34.0;

/// How much the corners of the clips on the tracks are rounded, in points. (A clip too narrow for
/// this much rounding is rounded as far as it can be, which is by half its width.)
pub const CLIP_CORNER_RADIUS: u8 = 27;

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

/// A clip whose file cannot be found.
pub const MISSING_CLIP_BACKGROUND: Color32 = Color32::from_rgb(82, 42, 46);
pub const MISSING_CLIP_TEXT: Color32 = Color32::from_rgb(232, 196, 198);

/// The box at the left of a track, with its title.
pub const TRACK_HEADER_BACKGROUND: Color32 = Color32::from_rgb(40, 43, 49);

/// What a drawer on a clip is filled with, and the button that opens it (which is over a clip, whatever it looks
/// like, so it dims what is behind it).
pub const DRAWER_BACKGROUND: Color32 = Color32::from_rgb(32, 34, 40);
pub const DRAWER_BUTTON_FILL: Color32 = Color32::from_black_alpha(150);
pub const DRAWER_BUTTON_FILL_HOVERED: Color32 = Color32::from_black_alpha(205);
pub const DRAWER_BUTTON_OUTLINE: Color32 = Color32::from_rgba_premultiplied(70, 70, 70, 70);
pub const DRAWER_BUTTON_OUTLINE_HOVERED: Color32 = Color32::from_rgba_premultiplied(150, 150, 150, 150);
pub const DRAWER_BUTTON_ARROW: Color32 = Color32::from_rgba_premultiplied(225, 225, 225, 225);

/// The box in the properties panel that has the properties of one thing in it: a little lighter than the panel.
pub const PROPERTIES_BOX_BACKGROUND: Color32 = Color32::from_rgb(38, 40, 46);

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
    }

    #[test]
    fn the_kinds_of_clip_that_are_not_pictures_of_a_video_have_their_own_colours() {
        let colours = [
            SOUND_CLIP_BACKGROUND,
            TEXT_CLIP_BACKGROUND,
            IMAGE_CLIP_BACKGROUND,
            MISSING_CLIP_BACKGROUND,
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
