//! The recent views of the timeline, which the Back and Forward buttons of a mouse go through, as the buttons of a browser go
//! through the pages it has been at.
//!
//! A view is how the timeline is looked at: how far it is zoomed and scrolled along, and how far its rows are zoomed and scrolled
//! up and down (see [`View`]). What the user does to it is a series of actions, each of which is one step in the history: a turn
//! of the wheel that scrolls it, and the turns that follow it soon after, are one action; so is a zoom (which goes on for as long
//! as the wheel is turned), and every press of the key that frames what is selected is an action of its own. What the timeline
//! does by itself is not: it follows the current frame while that moves (when the video plays, or the arrow keys are pressed), and
//! scrolls to show a drawer that is opening, and those are not steps to go back through.
//!
//! Going back is to the view as it was when the latest action began, which is where it was before that action changed it. Going on
//! from there with a new action forgets the views that were gone back from, as it does in a browser.

use super::vertical_zoom::VerticalZoom;

/// How the timeline is looked at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct View {
    /// How far along the strips the left edge of the view is, in points.
    pub(super) scroll_offset: f64,
    /// How many points of the screen a second of the finished video takes up, if the user has zoomed (see
    /// [`super::TimelineViewState`]).
    pub(super) points_per_second: Option<f64>,
    /// How far down the rows are scrolled, in points.
    pub(super) rows_scroll_offset: f32,
    /// How far the rows are zoomed up and down.
    pub(super) vertical_zoom: VerticalZoom,
}

/// What kind of thing the user did to the view. (Changes of one kind that come soon after one another are one action, which is
/// one step of the history.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ViewAction {
    /// Scrolled the strips or the rows.
    Scrolling,
    /// Zoomed the strips or the rows.
    Zooming,
    /// Framed what is selected: every time is an action of its own.
    Framing,
}

impl ViewAction {
    /// Whether changes of this kind that come one soon after another are one action (as the turns of a wheel are), and not each
    /// an action.
    fn goes_on_for_as_long_as_it_is_kept_up(self) -> bool {
        match self {
            Self::Scrolling | Self::Zooming => true,
            Self::Framing => false,
        }
    }
}

/// The views the timeline had, as far back as it is worth keeping, and the ones that were gone back from.
#[derive(Clone, Debug, Default)]
pub(super) struct ViewHistory {
    /// The views from before each action, the latest last.
    earlier: Vec<View>,
    /// The views that were gone back from, the one that was gone back from first last, so that going forward takes the last.
    later: Vec<View>,
    /// What the latest action was, and when it last changed the view.
    latest_action: Option<(ViewAction, f64)>,
}

impl ViewHistory {
    /// How many steps back are kept.
    pub(super) const MOST_STEPS_KEPT: usize = 100;

    /// How long the user can leave off an action, in seconds, and go on with it as the same action.
    pub(super) const PAUSE_THAT_ENDS_AN_ACTION: f64 = 0.4;

    /// Notes that an action of the kind `action` is changing the view now, at `time` seconds, and that the view was `before` it
    /// changed. If the action is the one that was going on, nothing is noted: its step is already there.
    pub(super) fn note_a_change(&mut self, action: ViewAction, before: View, time: f64) {
        let goes_on = action.goes_on_for_as_long_as_it_is_kept_up()
            && self
                .latest_action
                .is_some_and(|(latest, when)| latest == action && time - when <= Self::PAUSE_THAT_ENDS_AN_ACTION);
        if !goes_on {
            self.earlier.push(before);
            if self.earlier.len() > Self::MOST_STEPS_KEPT {
                self.earlier.remove(0);
            }
            // (A new action from a view that was gone back to leaves what was gone back from behind.)
            self.later.clear();
        }
        self.latest_action = Some((action, time));
    }

    /// The view to go back to from `current`, if there is one that is not what it is already (going back through views that are
    /// as the view is would show nothing happening), and remembers `current` to go forward to.
    pub(super) fn go_back(&mut self, current: View) -> Option<View> {
        let earlier = Self::take_one_that_differs_from(&mut self.earlier, current)?;
        self.later.push(current);
        self.latest_action = None;
        Some(earlier)
    }

    /// The view to go forward to from `current`, if there is one (after going back), and remembers `current` to go back to. (No action is
    /// going on to be ended: going back ended it, and an action after that would have made the views that were gone back from go.)
    pub(super) fn go_forward(&mut self, current: View) -> Option<View> {
        let later = Self::take_one_that_differs_from(&mut self.later, current)?;
        self.earlier.push(current);
        Some(later)
    }

    /// Takes the latest of `views` that is not `current`, and drops those after it that are (they are as the view is).
    fn take_one_that_differs_from(views: &mut Vec<View>, current: View) -> Option<View> {
        while let Some(view) = views.pop() {
            if view != current {
                return Some(view);
            }
        }
        None
    }

    #[cfg(test)]
    pub(super) fn steps_back(&self) -> usize {
        self.earlier.len()
    }

    #[cfg(test)]
    pub(super) fn steps_forward(&self) -> usize {
        self.later.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view that is scrolled `along` points along the strips, and otherwise as a timeline starts.
    fn view(along: f64) -> View {
        View {
            scroll_offset: along,
            points_per_second: None,
            rows_scroll_offset: 0.0,
            vertical_zoom: VerticalZoom::NORMAL,
        }
    }

    #[test]
    fn nothing_can_be_gone_back_or_forward_to_before_anything_was_done() {
        let mut history = ViewHistory::default();
        assert_eq!(history.go_back(view(0.0)), None);
        assert_eq!(history.go_forward(view(0.0)), None);
    }

    #[test]
    fn going_back_is_to_the_view_from_before_the_latest_action_and_going_forward_is_to_where_it_went_back_from() {
        let mut history = ViewHistory::default();
        history.note_a_change(ViewAction::Scrolling, view(0.0), 1.0);

        assert_eq!(history.go_back(view(50.0)), Some(view(0.0)));
        assert_eq!(history.go_back(view(0.0)), None, "there is nothing before it");
        assert_eq!(history.go_forward(view(0.0)), Some(view(50.0)));
        assert_eq!(history.go_forward(view(50.0)), None, "and nothing after it");
    }

    #[test]
    fn changes_of_one_kind_that_come_soon_after_one_another_are_one_action_and_one_step() {
        let mut history = ViewHistory::default();
        // A wheel turned for a second, a frame at a time (a sixtieth of a second).
        for frame in 0..60 {
            history.note_a_change(
                ViewAction::Scrolling,
                view(f64::from(frame)),
                1.0 + f64::from(frame) / 60.0,
            );
        }

        assert_eq!(history.steps_back(), 1);
        assert_eq!(
            history.go_back(view(60.0)),
            Some(view(0.0)),
            "the view from before the first change"
        );
    }

    #[test]
    fn a_pause_ends_an_action_and_the_next_change_is_another() {
        let mut history = ViewHistory::default();
        // (From the time 0, so that the pause is exactly as long as the pause: the sum of two other times may not be, to the last digit.)
        history.note_a_change(ViewAction::Scrolling, view(0.0), 0.0);
        history.note_a_change(
            ViewAction::Scrolling,
            view(10.0),
            ViewHistory::PAUSE_THAT_ENDS_AN_ACTION,
        );
        assert_eq!(
            history.steps_back(),
            1,
            "as long as the pause is not longer than that it is the same action"
        );

        history.note_a_change(
            ViewAction::Scrolling,
            view(20.0),
            2.0 + ViewHistory::PAUSE_THAT_ENDS_AN_ACTION + 0.01,
        );

        assert_eq!(history.steps_back(), 2);
        assert_eq!(history.go_back(view(30.0)), Some(view(20.0)));
        assert_eq!(history.go_back(view(20.0)), Some(view(0.0)));
    }

    #[test]
    fn what_goes_on_is_measured_from_the_latest_change_and_not_from_where_the_action_began() {
        let mut history = ViewHistory::default();
        // A change every third of a second for three seconds, which is never as long a pause as ends an action.
        for step in 0..9 {
            history.note_a_change(ViewAction::Zooming, view(f64::from(step)), f64::from(step) / 3.0);
        }
        assert_eq!(history.steps_back(), 1);
    }

    #[test]
    fn a_change_of_another_kind_is_another_action_whatever_the_time_is() {
        let mut history = ViewHistory::default();
        history.note_a_change(ViewAction::Scrolling, view(0.0), 1.0);
        history.note_a_change(ViewAction::Zooming, view(10.0), 1.01);
        history.note_a_change(ViewAction::Scrolling, view(20.0), 1.02);

        assert_eq!(history.steps_back(), 3);
    }

    #[test]
    fn every_framing_is_an_action_of_its_own_though_they_come_one_after_another() {
        let mut history = ViewHistory::default();
        history.note_a_change(ViewAction::Framing, view(0.0), 1.0);
        history.note_a_change(ViewAction::Framing, view(10.0), 1.0);
        history.note_a_change(ViewAction::Framing, view(20.0), 1.0);

        assert_eq!(history.steps_back(), 3);
    }

    #[test]
    fn going_back_ends_the_action_that_was_going_on_so_that_the_next_change_is_a_step_of_its_own() {
        let mut history = ViewHistory::default();
        history.note_a_change(ViewAction::Scrolling, view(0.0), 1.0);
        assert_eq!(history.go_back(view(5.0)), Some(view(0.0)));

        history.note_a_change(ViewAction::Scrolling, view(0.0), 1.05);

        assert_eq!(
            history.steps_back(),
            1,
            "it is a step of its own, from the view that was gone back to"
        );
        assert_eq!(history.go_back(view(9.0)), Some(view(0.0)));
    }

    #[test]
    fn a_new_action_after_going_back_leaves_what_was_gone_back_from_behind() {
        let mut history = ViewHistory::default();
        history.note_a_change(ViewAction::Scrolling, view(0.0), 1.0);
        history.note_a_change(ViewAction::Zooming, view(10.0), 2.0);
        assert_eq!(history.go_back(view(20.0)), Some(view(10.0)));
        assert_eq!(history.steps_forward(), 1);

        history.note_a_change(ViewAction::Scrolling, view(10.0), 3.0);

        assert_eq!(history.steps_forward(), 0, "there is nothing to go forward to");
        assert_eq!(history.go_forward(view(15.0)), None);
    }

    #[test]
    fn going_back_and_forward_goes_through_every_view_in_order_and_comes_back() {
        let mut history = ViewHistory::default();
        for (step, kind) in [ViewAction::Scrolling, ViewAction::Zooming, ViewAction::Framing]
            .into_iter()
            .enumerate()
        {
            history.note_a_change(kind, view(step as f64 * 10.0), step as f64);
        }
        // The views were 0, 10, 20 before the actions, and it is 30 now.

        let mut now = view(30.0);
        let mut visited = Vec::new();
        while let Some(earlier) = history.go_back(now) {
            visited.push(earlier);
            now = earlier;
        }
        assert_eq!(visited, vec![view(20.0), view(10.0), view(0.0)]);

        let mut forward = Vec::new();
        while let Some(later) = history.go_forward(now) {
            forward.push(later);
            now = later;
        }
        assert_eq!(forward, vec![view(10.0), view(20.0), view(30.0)]);
        assert_eq!(
            history.steps_back(),
            3,
            "and back where it was, with all of it to go back through"
        );
    }

    #[test]
    fn going_back_skips_the_views_that_are_as_the_view_already_is() {
        let mut history = ViewHistory::default();
        history.note_a_change(ViewAction::Scrolling, view(0.0), 1.0);
        history.note_a_change(ViewAction::Zooming, view(7.0), 2.0);
        history.note_a_change(ViewAction::Framing, view(7.0), 3.0);

        // It is at 7 now, so there is nothing to see in going back to the view from before the framing, or the zoom.
        assert_eq!(history.go_back(view(7.0)), Some(view(0.0)));
    }

    #[test]
    fn only_so_many_steps_back_are_kept_and_the_oldest_are_forgotten() {
        let mut history = ViewHistory::default();
        for step in 0..ViewHistory::MOST_STEPS_KEPT + 20 {
            history.note_a_change(ViewAction::Framing, view(step as f64), step as f64);
        }

        assert_eq!(history.steps_back(), ViewHistory::MOST_STEPS_KEPT);
        let mut oldest = None;
        let mut now = view(-1.0);
        while let Some(earlier) = history.go_back(now) {
            oldest = Some(earlier);
            now = earlier;
        }
        assert_eq!(oldest, Some(view(20.0)), "the first twenty are gone");
    }

    #[test]
    fn the_view_is_restored_exactly_as_it_was() {
        let zoomed = View {
            scroll_offset: 1234.5678,
            points_per_second: Some(88.25),
            rows_scroll_offset: 31.5,
            vertical_zoom: VerticalZoom::of(1.75),
        };
        let mut history = ViewHistory::default();
        history.note_a_change(ViewAction::Zooming, zoomed, 0.0);

        assert_eq!(history.go_back(view(0.0)), Some(zoomed));
    }
}
