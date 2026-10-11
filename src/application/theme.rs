//! The colours and sizes of the editor, in one place.

use egui::Color32;

/// How tall a track is, in points: the strip of pictures a video clip makes is this tall.
pub const TRACK_HEIGHT: f32 = 104.0;

/// How tall the ruler above the tracks is, in points.
pub const RULER_HEIGHT: f32 = 22.0;

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
pub const ADD_TRACK_BUTTON_DIAMETER: f32 = 40.0;

/// How much the corners of the clips on the tracks are rounded, in points. (A clip too narrow for
/// this much rounding is rounded as far as it can be, which is by half its width.)
pub const CLIP_CORNER_RADIUS: u8 = 18;

/// How wide one bar of the waveform of a sound clip is, in points.
pub const WAVEFORM_COLUMN_WIDTH: f64 = 2.0;

/// How tall the slider along the bottom is, in points.
pub const SCRUB_SLIDER_HEIGHT: f32 = 30.0;

/// The space around and between the parts of the window, in points.
pub const SPACING: f32 = 8.0;

/// How tall the thumbnail pictures are made, in pixels. They are shown about 104 points tall, and
/// this leaves room for screens that draw two pixels to a point.
pub const THUMBNAIL_HEIGHT_IN_PIXELS: u32 = 144;

pub const WINDOW_BACKGROUND: Color32 = Color32::from_rgb(24, 25, 28);
pub const PREVIEW_BACKGROUND: Color32 = Color32::from_rgb(10, 10, 12);
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

/// The outline around the selected clip.
pub const SELECTION_OUTLINE: Color32 = Color32::from_rgb(255, 204, 51);

/// A sound clip: dark blue waveform on a pale violet background.
pub const SOUND_CLIP_BACKGROUND: Color32 = Color32::from_rgb(224, 214, 246);
pub const SOUND_CLIP_WAVEFORM: Color32 = Color32::from_rgb(22, 36, 128);

/// A clip with a text object in it.
pub const TEXT_CLIP_BACKGROUND: Color32 = Color32::from_rgb(38, 94, 104);
pub const TEXT_CLIP_TEXT: Color32 = Color32::from_rgb(236, 246, 246);

/// A clip whose file cannot be found.
pub const MISSING_CLIP_BACKGROUND: Color32 = Color32::from_rgb(82, 42, 46);
pub const MISSING_CLIP_TEXT: Color32 = Color32::from_rgb(232, 196, 198);

/// The box at the left of a track, with its title.
pub const TRACK_HEADER_BACKGROUND: Color32 = Color32::from_rgb(40, 43, 49);

/// The floating settings boxes (and the pointer that comes out of them).
pub const BUBBLE_BACKGROUND: Color32 = Color32::from_rgb(38, 40, 46);
pub const BUBBLE_BORDER: Color32 = Color32::from_rgb(112, 118, 130);

pub const SLIDER_TRACK: Color32 = Color32::from_rgb(70, 74, 82);
pub const SLIDER_FILLED: Color32 = Color32::from_rgb(214, 88, 70);
pub const SLIDER_KNOB: Color32 = Color32::from_rgb(240, 240, 240);

pub const ICON: Color32 = Color32::from_rgb(232, 232, 236);
pub const ICON_DISABLED: Color32 = Color32::from_rgb(110, 112, 118);
pub const ICON_BUTTON_HOVER: Color32 = Color32::from_rgb(60, 64, 72);
pub const ICON_BUTTON_PRESSED: Color32 = Color32::from_rgb(84, 90, 102);
pub const ICON_BUTTON_ACTIVE: Color32 = Color32::from_rgb(52, 90, 140);

/// The outline shown around the fast-forward symbol while the pointer is over it.
pub const INDICATOR_OUTLINE: Color32 = Color32::from_rgb(120, 190, 255);
