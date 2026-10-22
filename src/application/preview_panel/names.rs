// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the parts of the preview are known by: to screen readers, to the tests, and to egui (which tells the
//! widgets apart by their ids).

use egui::Id;

use crate::project::{ClipId, Corner};

/// What the one widget that takes every click and drag on the panel of the preview is known by.
pub fn catcher_id() -> Id {
    Id::new("the preview")
}

/// The name a text object on the preview is known by to screen readers (and the tests).
pub fn text_object_label(clip: ClipId) -> String {
    format!("Text object, {clip}")
}

/// The name a video on the preview is known by.
pub fn video_label(clip: ClipId) -> String {
    format!("Video, {clip}")
}

/// The name a picture on the preview is known by.
pub fn picture_label(clip: ClipId) -> String {
    format!("Picture, {clip}")
}

/// The name a generator clip on the preview is known by.
pub fn generator_label(clip: ClipId) -> String {
    format!("Generator, {clip}")
}

/// The name the handle at a corner of the selected layer is known by.
pub fn corner_handle_label(corner: Corner) -> String {
    format!("Resize handle at the {} corner", corner_name(corner))
}

/// The name the place just outside a corner of the selected layer, where it is turned from, is known by.
pub fn turn_handle_label(corner: Corner) -> String {
    format!("Turning place outside the {} corner", corner_name(corner))
}

/// What the handle at `corner` of the layer of `clip` is known by to egui, which tells widgets apart by their ids.
pub fn corner_handle_id(clip: ClipId, corner: Corner) -> Id {
    Id::new(("corner of the layer", clip, corner_name(corner)))
}

/// What the place just outside `corner` of the layer of `clip`, where it is turned from, is known by to egui.
pub fn turn_place_id(clip: ClipId, corner: Corner) -> Id {
    Id::new(("place to turn from", clip, corner_name(corner)))
}

/// What a corner is called in the names of what is at it.
fn corner_name(corner: Corner) -> &'static str {
    match corner {
        Corner::TopLeft => "top left",
        Corner::TopRight => "top right",
        Corner::BottomLeft => "bottom left",
        Corner::BottomRight => "bottom right",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_corner_has_its_own_names() {
        let handles: std::collections::HashSet<String> = Corner::ALL.into_iter().map(corner_handle_label).collect();
        let turns: std::collections::HashSet<String> = Corner::ALL.into_iter().map(turn_handle_label).collect();
        assert_eq!(handles.len(), 4);
        assert_eq!(turns.len(), 4);
        assert!(handles.is_disjoint(&turns));
    }
}
