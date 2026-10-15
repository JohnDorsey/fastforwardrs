// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Work done on another thread whose single result is collected later, so that the window never
//! freezes while a video is being opened.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use crate::media::UserInterfaceWakeUp;

/// What a [`BackgroundTask`] has to report.
#[derive(Debug, PartialEq, Eq)]
pub enum TaskStatus<Output> {
    /// Still working.
    Pending,
    /// Finished: here is what it produced. (Reported once.)
    Finished(Output),
    /// The work stopped without producing anything, which means it crashed.
    Lost,
}

/// A piece of work running on its own thread.
pub struct BackgroundTask<Output> {
    receiver: Receiver<Output>,
}

impl<Output: Send + 'static> BackgroundTask<Output> {
    /// Starts `work` on a new thread. The `wake_up` callback is called once it has finished, so
    /// that the window draws again and notices.
    pub fn start(wake_up: UserInterfaceWakeUp, work: impl FnOnce() -> Output + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let output = work();
            let _ = sender.send(output);
            wake_up();
        });
        Self { receiver }
    }

    /// Whether the work has finished, without waiting for it.
    pub fn status(&self) -> TaskStatus<Output> {
        match self.receiver.try_recv() {
            Ok(output) => TaskStatus::Finished(output),
            Err(TryRecvError::Empty) => TaskStatus::Pending,
            Err(TryRecvError::Disconnected) => TaskStatus::Lost,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::*;

    fn wait_for_finish<Output: Send + 'static>(task: &BackgroundTask<Output>) -> TaskStatus<Output> {
        let started = Instant::now();
        loop {
            match task.status() {
                TaskStatus::Pending => {
                    assert!(started.elapsed() < Duration::from_secs(10), "the task never finished");
                    thread::sleep(Duration::from_millis(2));
                }
                other => return other,
            }
        }
    }

    #[test]
    fn a_task_reports_pending_until_it_finishes_then_its_result_once() {
        let (release, wait_to_be_released) = mpsc::channel::<()>();
        let task = BackgroundTask::start(Arc::new(|| {}), move || {
            wait_to_be_released.recv().unwrap();
            42
        });
        assert_eq!(task.status(), TaskStatus::Pending);

        release.send(()).unwrap();
        assert_eq!(wait_for_finish(&task), TaskStatus::Finished(42));
        // The result has been collected: asking again finds the work gone.
        assert_eq!(task.status(), TaskStatus::Lost);
    }

    #[test]
    fn the_wake_up_callback_is_called_when_the_work_is_done() {
        let wake_ups = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&wake_ups);
        let task = BackgroundTask::start(
            Arc::new(move || {
                counted.fetch_add(1, Ordering::SeqCst);
            }),
            || "done",
        );
        assert_eq!(wait_for_finish(&task), TaskStatus::Finished("done"));
        let started = Instant::now();
        while wake_ups.load(Ordering::SeqCst) == 0 {
            assert!(started.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(wake_ups.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn work_that_crashes_is_reported_as_lost() {
        let task: BackgroundTask<u32> =
            BackgroundTask::start(Arc::new(|| {}), || panic!("this crash is expected by the test"));
        assert_eq!(wait_for_finish(&task), TaskStatus::Lost);
    }
}
