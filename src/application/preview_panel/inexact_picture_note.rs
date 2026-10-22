// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The line over the bottom of the picture of the preview **while the picture is not exact** (EN-14): an effect that depends on earlier frames went on from a state that is not the one of the
//! frame before, because the playhead was moved (or a frame was skipped), and the engine has only a copy that it took some frames before to go on from. It is never there otherwise, and it goes
//! when the effect has gone on for a while (the engine says when), or, when the lead-up on a seek is made, when it is. It says which of the two it waits for. The export is exact, and does not say this.

use egui::{Color32, Label, Rect, RichText, Ui, pos2};

/// What the line says (and the name it is known by to screen readers and to tests).
pub const THE_PICTURE_IS_NOT_EXACT_NOTE: &str =
    "An effect depends on earlier frames: this picture is approximate until playback has caught up.";

/// What the line says while the lead-up on a seek is being made: the frames before this one are being made, and the picture is exact when they are.
pub const THE_PICTURE_IS_NOT_EXACT_AND_THE_LEAD_UP_IS_BEING_MADE_NOTE: &str =
    "An effect depends on earlier frames: this picture is approximate until the frames before it have been made.";

/// How tall the line is, in points.
const HEIGHT: f32 = 22.0;

/// What the line says: that the picture is approximate until playback has caught up, or until the lead-up has been made.
pub fn the_note(a_lead_up_is_being_made: bool) -> &'static str {
    if a_lead_up_is_being_made {
        THE_PICTURE_IS_NOT_EXACT_AND_THE_LEAD_UP_IS_BEING_MADE_NOTE
    } else {
        THE_PICTURE_IS_NOT_EXACT_NOTE
    }
}

/// Shows the line over the bottom of the part of the picture that is in the panel (`picture` is where all of it is, which may reach past `panel`), saying whether it is the lead-up that the
/// picture waits for.
pub(super) fn show(ui: &mut Ui, picture: Rect, panel: Rect, a_lead_up_is_being_made: bool) {
    let visible = picture.intersect(panel);
    if visible.width() < 1.0 || visible.height() < 1.0 {
        return;
    }
    let strip = Rect::from_min_max(
        pos2(visible.left(), (visible.bottom() - HEIGHT).max(visible.top())),
        visible.right_bottom(),
    );
    ui.painter().rect_filled(strip, 0.0, Color32::from_black_alpha(170));
    // (Not selectable, so that it is not something to click on over the layers that are under it.)
    ui.put(
        strip,
        Label::new(
            RichText::new(the_note(a_lead_up_is_being_made))
                .color(Color32::WHITE)
                .size(12.0),
        )
        .selectable(false)
        .truncate(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_says_whether_the_picture_waits_for_playback_or_for_the_lead_up() {
        assert_eq!(the_note(false), THE_PICTURE_IS_NOT_EXACT_NOTE);
        assert_eq!(
            the_note(true),
            THE_PICTURE_IS_NOT_EXACT_AND_THE_LEAD_UP_IS_BEING_MADE_NOTE
        );
        assert!(the_note(false).contains("until playback has caught up"));
        assert!(the_note(true).contains("until the frames before it have been made"));
    }
}
