// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The properties of a rack of keyframes: which property of what its keyframes are of, and how many there are and from where to
//! where. (What the property is at a moment is changed with the controls of what it is a property of.)

use egui::Ui;

use super::{PropertiesContent, kind_box};
use crate::application::naming::what_a_rack_is_called;
use crate::project::{RackId, TimelineFrameIndex, format_timecode};

/// What the box of a rack is headed with.
pub(super) const KIND_NAME: &str = "Rack of keyframes";

/// What is said of a rack that is in the project: what its property is, of what, and how many keyframes it has and when the first
/// and the last are at (the words for the number of them, and for the times, are one text).
pub(super) fn description(content: &PropertiesContent<'_>, rack: RackId) -> Option<(String, String)> {
    let located = content.project.located_rack(rack)?;
    let (first, last) = (located.rack.first_frame()?, located.rack.last_frame()?);
    let time_of = |frame: u64| {
        format_timecode(
            TimelineFrameIndex::new(located.timeline_frame_of(frame)),
            content.project.frame_rate(),
        )
    };
    let count = located.rack.len();
    let how_many = if count == 1 {
        format!("1 keyframe, at {}", time_of(first))
    } else {
        format!("{count} keyframes, from {} to {}", time_of(first), time_of(last))
    };
    Some((
        what_a_rack_is_called(content.project, located.rack, located.owner, content.source_looks),
        how_many,
    ))
}

/// Shows the properties of the rack `rack`.
pub(super) fn show(ui: &mut Ui, content: &PropertiesContent<'_>, rack: RackId) {
    let Some((what, how_many)) = description(content, rack) else {
        return;
    };
    kind_box::show(ui, KIND_NAME, |ui| {
        ui.weak(what);
        ui.add_space(4.0);
        ui.label(how_many);
    });
}
