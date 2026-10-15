// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The frame that a gesture makes keyframes at.
//!
//! A gesture (a slider dragged, a value dragged, a picture moved about on the preview) asks for a change in every frame of the
//! window for as long as it goes on, and what it changes, when that has keyframes, is changed at the playhead: a keyframe is made
//! there, or the one that is there is changed. If the playhead moved while the gesture went on (the video was playing, or a key
//! moved it), every frame of the gesture would put a keyframe at another frame, and a gesture of a few seconds would leave a
//! keyframe at each of the hundreds of frames it went over: the whole movement of the hand written down. A gesture is to make at
//! most one keyframe of what it changes, so it keeps to the frame that the playhead was at when it began.
//!
//! What a gesture is, for this, is a press of the mouse button (egui says when it began, and it is another press every time the
//! button goes down) that makes the same sort of edit all the way. An edit with no button down (typing a value in) is a gesture
//! of one frame of the window, and is made where the playhead is.

use super::history::EditKey;
use crate::project::TimelineFrameIndex;

/// What a gesture began as.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Gesture {
    /// What the edits of the gesture are about (see [`EditKey`]).
    key: EditKey,
    /// When the press that the gesture is, began: the seconds of the window's own clock, which are what egui says.
    press_began_at: f64,
    /// Where the playhead was then.
    frame: TimelineFrameIndex,
}

/// The gesture that is being made, if there is one, and the frame it keeps to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GestureFrame {
    latest: Option<Gesture>,
}

impl GestureFrame {
    /// The frame that the keyframes that an edit makes are made at: the frame the playhead was at when the gesture that the edit is
    /// more of began, if it is; and `playhead` (and what begins a gesture) otherwise.
    ///
    /// `key` is what the edit is about, if it is the kind of edit that a gesture makes over and over; and `press_began_at` when the
    /// press that is held down began (`None` if no button is down).
    pub fn frame_for(
        &mut self,
        key: Option<EditKey>,
        press_began_at: Option<f64>,
        playhead: TimelineFrameIndex,
    ) -> TimelineFrameIndex {
        let (Some(key), Some(press_began_at)) = (key, press_began_at) else {
            self.latest = None;
            return playhead;
        };
        if let Some(gesture) = self.latest
            && gesture.key == key
            && gesture.press_began_at == press_began_at
        {
            return gesture.frame;
        }
        self.latest = Some(Gesture {
            key,
            press_began_at,
            frame: playhead,
        });
        playhead
    }

    /// Lets go of the gesture (the project was replaced, or an edit was undone: what comes next is not more of it).
    pub fn forget(&mut self) {
        self.latest = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(number: u64) -> TimelineFrameIndex {
        TimelineFrameIndex::new(number)
    }

    fn key(about: &str) -> Option<EditKey> {
        Some(EditKey::about(&about))
    }

    #[test]
    fn an_edit_with_no_button_down_is_made_at_the_playhead_whatever_came_before() {
        let mut gesture = GestureFrame::default();
        assert_eq!(gesture.frame_for(key("position"), None, frame(20)), frame(20));
        assert_eq!(gesture.frame_for(key("position"), None, frame(30)), frame(30));
        assert_eq!(gesture.frame_for(None, None, frame(40)), frame(40));
    }

    #[test]
    fn the_edits_of_a_press_are_made_at_the_frame_the_playhead_was_at_when_it_began_whatever_the_playhead_does() {
        let mut gesture = GestureFrame::default();
        let press = Some(5.0);
        assert_eq!(gesture.frame_for(key("position"), press, frame(20)), frame(20));
        for playhead in [21, 22, 23, 90, 3] {
            assert_eq!(
                gesture.frame_for(key("position"), press, frame(playhead)),
                frame(20),
                "the playhead is at {playhead}"
            );
        }
    }

    #[test]
    fn another_press_is_another_gesture_even_of_the_same_edit() {
        let mut gesture = GestureFrame::default();
        assert_eq!(gesture.frame_for(key("position"), Some(5.0), frame(20)), frame(20));
        assert_eq!(
            gesture.frame_for(key("position"), Some(9.5), frame(60)),
            frame(60),
            "the button went down again, with the playhead somewhere else"
        );
        assert_eq!(gesture.frame_for(key("position"), Some(9.5), frame(61)), frame(60));
    }

    #[test]
    fn another_sort_of_edit_in_the_same_press_is_another_gesture() {
        let mut gesture = GestureFrame::default();
        assert_eq!(gesture.frame_for(key("position"), Some(5.0), frame(20)), frame(20));
        assert_eq!(gesture.frame_for(key("size"), Some(5.0), frame(25)), frame(25));
        assert_eq!(gesture.frame_for(key("size"), Some(5.0), frame(30)), frame(25));
    }

    #[test]
    fn an_edit_that_is_not_the_sort_that_a_gesture_makes_over_and_over_has_no_frame_to_keep_to() {
        let mut gesture = GestureFrame::default();
        assert_eq!(gesture.frame_for(None, Some(5.0), frame(20)), frame(20));
        assert_eq!(gesture.frame_for(None, Some(5.0), frame(30)), frame(30));
    }

    #[test]
    fn a_gesture_that_was_let_go_of_is_not_gone_on_with() {
        let mut gesture = GestureFrame::default();
        assert_eq!(gesture.frame_for(key("position"), Some(5.0), frame(20)), frame(20));
        gesture.forget();
        assert_eq!(
            gesture.frame_for(key("position"), Some(5.0), frame(70)),
            frame(70),
            "the edit that followed was undone, say, so this is not more of the gesture"
        );
    }

    #[test]
    fn an_edit_with_no_button_down_ends_the_gesture_so_that_it_does_not_come_back() {
        let mut gesture = GestureFrame::default();
        assert_eq!(gesture.frame_for(key("position"), Some(5.0), frame(20)), frame(20));
        assert_eq!(gesture.frame_for(key("position"), None, frame(40)), frame(40));
        assert_eq!(
            gesture.frame_for(key("position"), Some(5.0), frame(50)),
            frame(50),
            "the same press, as far as the clock goes, cannot be gone back to"
        );
    }
}
