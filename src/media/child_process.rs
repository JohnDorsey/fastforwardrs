//! Small helpers shared by everything that runs ffmpeg or ffprobe.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

/// On Windows, asks for a child process to be started without opening a console window.
#[cfg(windows)]
const WINDOWS_CREATE_NO_WINDOW_FLAG: u32 = 0x0800_0000;

/// How much of a tool's error output is kept for showing to the user.
const ERROR_OUTPUT_LIMIT_IN_BYTES: usize = 4000;

/// How much error output is read in one go while draining a tool's error stream.
const ERROR_OUTPUT_CHUNK_IN_BYTES: usize = 4096;

/// A command that runs `program` quietly in the background: it never reads from the keyboard and
/// (on Windows) never flashes up a console window, which would otherwise happen every time the
/// editor started ffmpeg.
pub(super) fn background_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(WINDOWS_CREATE_NO_WINDOW_FLAG);
    }
    command
}

/// What a reading thread calls, once the tool's output has ended, to get what the tool said on its
/// error stream (which is collected in the background while it runs).
pub(super) type ErrorOutputWhenFinished = Box<dyn FnOnce() -> String + Send>;

/// A tool running in the background whose standard output is read, as it arrives, by a thread of
/// its own that turns it into events (decoded pictures, thumbnails, samples of sound...). The
/// events wait in a channel of limited size: when it is full the reading thread waits, so the
/// tool, which is much faster than playback, does not race ahead and use up the memory.
///
/// Dropping this stops the tool.
pub(super) struct StreamingTool<Event> {
    child: Child,
    events: Receiver<Event>,
}

impl<Event: Send + 'static> StreamingTool<Event> {
    /// Starts `command` with its standard output and error piped, and runs `read_events` on a new
    /// thread. `read_events` is given the tool's output, the way to send events (up to
    /// `buffered_event_count` can wait to be collected), and the way to get the tool's error output
    /// at the end. It should end when the output does, or when sending fails (nobody wants more).
    pub(super) fn start(
        mut command: Command,
        buffered_event_count: usize,
        read_events: impl FnOnce(ChildStdout, SyncSender<Event>, ErrorOutputWhenFinished) + Send + 'static,
    ) -> io::Result<Self> {
        let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let (Some(standard_output), Some(standard_error)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            return Err(io::Error::other("the program's pipes were not created"));
        };
        let (sender, events) = mpsc::sync_channel(buffered_event_count);
        let error_output_collector = collect_error_output_in_background(standard_error);
        thread::spawn(move || {
            let error_output_when_finished: ErrorOutputWhenFinished =
                Box::new(move || error_output_collector.join().unwrap_or_default());
            read_events(standard_output, sender, error_output_when_finished);
        });
        Ok(Self { child, events })
    }

    /// The next event, if one is ready. Never waits.
    pub(super) fn try_next_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }
}

impl<Event> Drop for StreamingTool<Event> {
    fn drop(&mut self) {
        // Stopping the tool closes its output, which ends the reading thread.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Reads a tool's error output in the background until it ends, keeping only the end of it, and
/// gives that back when the returned handle is joined.
///
/// A tool's error output has to be read while the tool is running. If nobody reads it, a tool that
/// prints a lot (say, one complaining about every frame of a damaged file) fills the pipe's small
/// buffer and then stops dead, waiting for someone to read, which would hang the editor.
pub(super) fn collect_error_output_in_background(mut error_stream: impl Read + Send + 'static) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut recent_output: Vec<u8> = Vec::new();
        let mut chunk = [0u8; ERROR_OUTPUT_CHUNK_IN_BYTES];
        loop {
            match error_stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(byte_count) => {
                    recent_output.extend_from_slice(&chunk[..byte_count]);
                    if recent_output.len() > 2 * ERROR_OUTPUT_LIMIT_IN_BYTES {
                        let surplus = recent_output.len() - ERROR_OUTPUT_LIMIT_IN_BYTES;
                        recent_output.drain(..surplus);
                    }
                }
            }
        }
        end_of_error_output(&recent_output)
    })
}

/// The end of a tool's error output, which is where the explanation of a failure is.
pub(super) fn end_of_error_output(error_output: &[u8]) -> String {
    let text = String::from_utf8_lossy(error_output);
    let text = text.trim();
    if text.len() <= ERROR_OUTPUT_LIMIT_IN_BYTES {
        return text.to_owned();
    }
    let mut start = text.len() - ERROR_OUTPUT_LIMIT_IN_BYTES;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("...{}", &text[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_error_output_is_kept_whole_and_trimmed() {
        assert_eq!(end_of_error_output(b"  bad file \n"), "bad file");
        assert_eq!(end_of_error_output(b""), "");
    }

    #[test]
    fn long_error_output_keeps_only_its_end() {
        let long_text = format!("{}the real reason", "x".repeat(10_000));
        let kept = end_of_error_output(long_text.as_bytes());
        assert!(kept.starts_with("..."));
        assert!(kept.ends_with("the real reason"));
        assert!(kept.len() <= ERROR_OUTPUT_LIMIT_IN_BYTES + 3);
    }

    #[test]
    fn cutting_long_output_never_splits_a_multi_byte_character() {
        let long_text = format!("{}é", "é".repeat(5_000));
        let kept = end_of_error_output(long_text.as_bytes());
        assert!(kept.ends_with('é'));
    }

    #[test]
    fn invalid_utf8_is_replaced_instead_of_failing() {
        assert_eq!(end_of_error_output(&[b'o', b'k', 0xFF]), "ok\u{FFFD}");
    }

    #[test]
    fn background_collection_keeps_the_end_of_an_enormous_error_stream() {
        let mut enormous_stream = "complaint about frame\n".repeat(100_000);
        enormous_stream.push_str("the final explanation");
        let collected = collect_error_output_in_background(std::io::Cursor::new(enormous_stream.into_bytes()))
            .join()
            .unwrap();
        assert!(collected.ends_with("the final explanation"));
        assert!(collected.len() <= ERROR_OUTPUT_LIMIT_IN_BYTES + 3);
    }

    #[test]
    fn background_collection_of_a_quiet_tool_gives_nothing() {
        let collected = collect_error_output_in_background(std::io::Cursor::new(Vec::new()))
            .join()
            .unwrap();
        assert_eq!(collected, "");
    }
}
