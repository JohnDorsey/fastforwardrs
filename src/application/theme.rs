//! The colours and sizes of the editor, in one place.

use egui::Color32;

/// How tall the strip of pictures (the video track) is, in points.
pub const TRACK_HEIGHT: f32 = 104.0;

/// How tall the ruler above the video track is, in points.
pub const RULER_HEIGHT: f32 = 22.0;

/// How much the corners of the clips on the video track are rounded, in points ("a little bit").
pub const CLIP_CORNER_RADIUS: u8 = 6;

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

/// The ruler above the video track.
pub const RULER_BACKGROUND: Color32 = Color32::from_rgb(30, 32, 36);
pub const RULER_TICK: Color32 = Color32::from_rgb(150, 154, 162);

pub const SLIDER_TRACK: Color32 = Color32::from_rgb(70, 74, 82);
pub const SLIDER_FILLED: Color32 = Color32::from_rgb(214, 88, 70);
pub const SLIDER_KNOB: Color32 = Color32::from_rgb(240, 240, 240);

pub const ICON: Color32 = Color32::from_rgb(232, 232, 236);
pub const ICON_DISABLED: Color32 = Color32::from_rgb(110, 112, 118);
pub const ICON_BUTTON_HOVER: Color32 = Color32::from_rgb(60, 64, 72);
pub const ICON_BUTTON_PRESSED: Color32 = Color32::from_rgb(84, 90, 102);
pub const ICON_BUTTON_ACTIVE: Color32 = Color32::from_rgb(52, 90, 140);

/// The pop-up speed slider.
pub const SPEED_SLIDER_BACKGROUND: Color32 = Color32::from_rgba_premultiplied(18, 19, 22, 235);
pub const SPEED_SLIDER_TICK: Color32 = Color32::from_rgb(150, 154, 162);
pub const SPEED_SLIDER_TEXT: Color32 = Color32::from_rgb(235, 235, 238);

/// The outline shown around the fast-forward symbol while the pointer is over it.
pub const INDICATOR_OUTLINE: Color32 = Color32::from_rgb(120, 190, 255);
