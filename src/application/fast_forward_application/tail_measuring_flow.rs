// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! When the window has the tails of the sound effects measured (docs/engine-phase-15-vst3-design.md, section 16): in the background, **once the project has been left as it is for a
//! moment**. A hand that drags the decay of a reverb makes a new project at every frame, and a measurement runs the plugin for a fraction of a second: the sound is not measured
//! for each of the thousand values on the way, only for the one that the hand stops at. (Until then the tail is the one that was measured for the values before, or the guess of the
//! scan.)

use std::time::{Duration, Instant};

use eframe::egui::Context;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::FastForwardApplication;

/// How long the project, the plugins and what is measured must have stayed as they are before the tails of the sound are looked for.
const QUIET_BEFORE_MEASURING: Duration = Duration::from_millis(300);

/// Everything that makes a different list of tails to measure: the project (another, or an edit, an undo, a redo), which plugins may be run, how many tails are measured, how many plugins are
/// installed and how many tails are being measured now (a thread that ends is a moment to look again).
type WhatTailsDependOn = ((u64, u64), u64, u64, usize, usize);

/// What to do at a frame about the tails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WhatToDoAboutTails {
    /// Nothing: they were looked for as things are.
    Nothing,
    /// Ask again after this long (things have just changed).
    WaitFor(Duration),
    /// Look for them now.
    Look,
}

/// Remembers what the tails were last looked for at, and when things last changed.
#[derive(Default)]
pub(super) struct WhenToMeasureTails {
    seen: Option<(WhatTailsDependOn, Instant)>,
    looked_for_at: Option<WhatTailsDependOn>,
    /// Whether a thread is going to ask the window to draw again when the moment is over (one at a time).
    wake_is_pending: Arc<AtomicBool>,
}

impl WhenToMeasureTails {
    /// What to do now that things are as `things_are`. **`is_there_anything_to_measure` is asked once for each state of things** (not at each frame): when there is nothing, nothing is
    /// waited for and the window is not asked to draw again for it (a project with no plugin on it has no business with this).
    pub(super) fn ask(
        &mut self,
        things_are: WhatTailsDependOn,
        now: Instant,
        is_there_anything_to_measure: impl FnOnce() -> bool,
    ) -> WhatToDoAboutTails {
        if self.looked_for_at == Some(things_are) {
            return WhatToDoAboutTails::Nothing;
        }
        let since = match self.seen {
            Some((seen, since)) if seen == things_are => since,
            _ => {
                if !is_there_anything_to_measure() {
                    self.seen = None;
                    self.looked_for_at = Some(things_are);
                    return WhatToDoAboutTails::Nothing;
                }
                self.seen = Some((things_are, now));
                now
            }
        };
        let quiet_for = now.saturating_duration_since(since);
        if quiet_for >= QUIET_BEFORE_MEASURING {
            self.looked_for_at = Some(things_are);
            WhatToDoAboutTails::Look
        } else {
            WhatToDoAboutTails::WaitFor(QUIET_BEFORE_MEASURING - quiet_for)
        }
    }
}

impl WhenToMeasureTails {
    /// Has the window draw again after `wait`, from a thread (one at a time): **not with `request_repaint_after`**, which a test harness takes for a window that never settles.
    fn draw_again_after(&self, context: &Context, wait: Duration) {
        if self.wake_is_pending.swap(true, Ordering::SeqCst) {
            return;
        }
        let (pending, context) = (Arc::clone(&self.wake_is_pending), context.clone());
        let started = std::thread::Builder::new()
            .name("tail meter timer".to_owned())
            .spawn(move || {
                std::thread::sleep(wait);
                pending.store(false, Ordering::SeqCst);
                context.request_repaint();
            });
        if started.is_err() {
            self.wake_is_pending.store(false, Ordering::SeqCst);
        }
    }
}

impl FastForwardApplication {
    /// Has the tails of the sound effects of the project measured in the background when the project has stayed as it is for a moment (see the module).
    pub(super) fn keep_the_tails_of_the_sound_measured(&mut self, context: &Context, now: Instant) {
        let host = &self.environment.plugin_host;
        let things = (
            self.document.history.state_of_the_project(),
            host.generation(),
            crate::composition::how_many_tails_have_been_measured(),
            crate::project::installed_plugins().len(),
            host.how_many_tails_are_being_measured(),
        );
        let project = &self.document.project;
        let is_there_anything_to_measure = || {
            !crate::project::installed_plugins().is_empty()
                && !crate::composition::the_tails_to_measure_in(project).is_empty()
        };
        match self.jobs.tail_measuring.ask(things, now, is_there_anything_to_measure) {
            WhatToDoAboutTails::Nothing => {}
            WhatToDoAboutTails::WaitFor(wait) => self.jobs.tail_measuring.draw_again_after(context, wait),
            WhatToDoAboutTails::Look => host.measure_the_tails_in_the_background(&self.document.project),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn things(edit: u64) -> WhatTailsDependOn {
        ((0, edit), 0, 0, 1, 0)
    }

    #[test]
    fn the_tails_are_looked_for_when_things_have_stayed_as_they_are_for_a_moment_and_once() {
        let mut when = WhenToMeasureTails::default();
        let start = Instant::now();
        assert_eq!(
            when.ask(things(1), start, || true),
            WhatToDoAboutTails::WaitFor(QUIET_BEFORE_MEASURING)
        );
        assert_eq!(
            when.ask(things(1), start + Duration::from_millis(100), || true),
            WhatToDoAboutTails::WaitFor(Duration::from_millis(200))
        );
        assert_eq!(
            when.ask(things(1), start + QUIET_BEFORE_MEASURING, || true),
            WhatToDoAboutTails::Look
        );
        assert_eq!(
            when.ask(things(1), start + QUIET_BEFORE_MEASURING * 5, || true),
            WhatToDoAboutTails::Nothing,
            "once for things as they are"
        );
    }

    #[test]
    fn a_hand_that_keeps_changing_the_project_is_waited_for_and_what_it_stops_at_is_measured() {
        let mut when = WhenToMeasureTails::default();
        let start = Instant::now();
        for edit in 0..50 {
            let at = start + Duration::from_millis(10 * edit);
            assert!(
                matches!(when.ask(things(edit), at, || true), WhatToDoAboutTails::WaitFor(_)),
                "a project that changes at every frame is not measured, edit {edit}"
            );
        }
        let stopped = start + Duration::from_millis(500);
        assert!(matches!(
            when.ask(things(49), stopped, || true),
            WhatToDoAboutTails::WaitFor(_)
        ));
        assert_eq!(
            when.ask(things(49), stopped + QUIET_BEFORE_MEASURING, || true),
            WhatToDoAboutTails::Look
        );
    }

    #[test]
    fn a_change_after_the_look_is_looked_for_again_after_a_moment() {
        let mut when = WhenToMeasureTails::default();
        let start = Instant::now();
        when.ask(things(1), start, || true);
        assert_eq!(
            when.ask(things(1), start + QUIET_BEFORE_MEASURING, || true),
            WhatToDoAboutTails::Look
        );
        let later = start + Duration::from_secs(10);
        assert!(matches!(
            when.ask(things(2), later, || true),
            WhatToDoAboutTails::WaitFor(_)
        ));
        assert_eq!(
            when.ask(things(2), later + QUIET_BEFORE_MEASURING, || true),
            WhatToDoAboutTails::Look
        );
    }

    #[test]
    fn where_there_is_nothing_to_measure_nothing_is_waited_for_and_it_is_asked_once_for_each_state_of_things() {
        let mut when = WhenToMeasureTails::default();
        let start = Instant::now();
        let mut asked = 0;
        for frame in 0..5 {
            let at = start + Duration::from_millis(10 * frame);
            assert_eq!(
                when.ask(things(1), at, || {
                    asked += 1;
                    false
                }),
                WhatToDoAboutTails::Nothing,
                "nothing to wait for, so the window is not asked to draw again"
            );
        }
        assert_eq!(asked, 1, "once for these things, not at each frame");
        assert_eq!(
            when.ask(things(2), start + Duration::from_secs(1), || {
                asked += 1;
                true
            }),
            WhatToDoAboutTails::WaitFor(QUIET_BEFORE_MEASURING),
            "things changed: asked again, and now there is something"
        );
        assert_eq!(asked, 2);
    }
}
