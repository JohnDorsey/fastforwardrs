//! The properties of a keyframe: which property of what it is a keyframe of, and where it is in time. (What the property is
//! at that moment is changed with the controls of what it is a property of, with the playhead put on the keyframe.)

use egui::Ui;

use super::{PropertiesContent, kind_box};
use crate::application::naming::what_a_rack_is_called;
use crate::project::{KeyframeId, TimelineFrameIndex, format_timecode};

/// What the box of a keyframe is headed with.
pub(super) const KIND_NAME: &str = "Keyframe";

/// What is said of a keyframe that is in the project: what its property is, of what, and when it is at.
pub(super) fn description(content: &PropertiesContent<'_>, keyframe: KeyframeId) -> Option<(String, String)> {
    let located = content.project.located_rack(keyframe.rack())?;
    let point = located.rack.keyframe(keyframe)?;
    let at = TimelineFrameIndex::new(located.timeline_frame_of(point.frame));
    Some((
        what_a_rack_is_called(content.project, located.rack, located.owner, content.source_looks),
        format_timecode(at, content.project.frame_rate()),
    ))
}

/// Shows the properties of the keyframe `keyframe`.
pub(super) fn show(ui: &mut Ui, content: &PropertiesContent<'_>, keyframe: KeyframeId) {
    let Some((what, when)) = description(content, keyframe) else {
        return;
    };
    kind_box::show(ui, KIND_NAME, |ui| {
        ui.weak(what);
        ui.add_space(4.0);
        ui.label(format!("At {when}"));
    });
}
