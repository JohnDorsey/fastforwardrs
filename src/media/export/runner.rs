//! Running an export: starting ffmpeg, reporting its progress, and finishing or cancelling cleanly.

use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tempfile::TempDir;
use thiserror::Error;

use super::command::{ExportFiles, VideoEncoder, export_arguments};
use super::plan::ExportPlan;
use super::progress::frames_written_in_progress_line;
use crate::media::UserInterfaceWakeUp;
use crate::media::child_process::{background_command, collect_error_output_in_background};
use crate::media::ffmpeg_toolchain::FfmpegToolchain;
use crate::media::location::MediaLocation;

/// How often the export checks whether ffmpeg has finished, once it has stopped printing.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(
        "the export would overwrite one of the files the project is made from ({path}); choose a different name for the exported video"
    )]
    DestinationIsASourceFile { path: PathBuf },
    #[error("the export needs {needed} source files, but {given} were given")]
    WrongNumberOfSourceFiles { needed: usize, given: usize },
    #[error("the export's working files could not be prepared: {0}")]
    CouldNotPrepare(#[source] io::Error),
    #[error("ffmpeg could not be started for the export: {0}")]
    CouldNotStart(#[source] io::Error),
}

/// How an export ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportResult {
    /// The finished video is at the destination.
    Completed,
    /// The export was cancelled. Nothing was left at the destination; a file that was already
    /// there is untouched.
    Cancelled,
    /// The export failed; `reason` is ffmpeg's explanation. Nothing was left at the destination;
    /// a file that was already there is untouched.
    Failed { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportEvent {
    /// How many frames of the finished video have been written so far.
    Progress {
        frames_written: u64,
    },
    Finished(ExportResult),
}

/// A running export. Dropping it cancels the export, and waits for the clean-up (which takes a
/// moment, once ffmpeg has been stopped) so that no half-written file is left behind, even if the
/// program is about to exit.
pub struct VideoExport {
    child: Arc<Mutex<Child>>,
    cancel_requested: Arc<AtomicBool>,
    events: Receiver<ExportEvent>,
    finisher: Option<JoinHandle<()>>,
}

impl VideoExport {
    /// Starts exporting. `source_file_locations` are where the plan's source files are, in the order
    /// the plan lists them ([`ExportPlan::source_files`]). The video is written to a temporary
    /// file beside `destination` and moved into place only when it is complete, so a cancelled or
    /// failed export never damages a file that is already at `destination`.
    pub fn start(
        toolchain: &FfmpegToolchain,
        plan: &ExportPlan,
        source_file_locations: &[MediaLocation],
        destination: &Path,
        wake_up: UserInterfaceWakeUp,
    ) -> Result<Self, ExportError> {
        if source_file_locations.len() != plan.source_files.len() {
            return Err(ExportError::WrongNumberOfSourceFiles {
                needed: plan.source_files.len(),
                given: source_file_locations.len(),
            });
        }
        // (A source file that is a stretch of another file, such as a project file, is the file that it is in, for this: writing
        // over that would destroy it as much.)
        if let Some(location) = source_file_locations
            .iter()
            .find(|location| is_the_same_file(location.file(), destination))
        {
            return Err(ExportError::DestinationIsASourceFile {
                path: location.file().to_owned(),
            });
        }

        let working_directory = tempfile::Builder::new()
            .prefix("fastforwardrs-export-")
            .tempdir()
            .map_err(ExportError::CouldNotPrepare)?;
        let temporary_output = temporary_output_path(destination);
        let files = write_working_files(&working_directory, plan, source_file_locations, &temporary_output)?;
        let arguments = export_arguments(&files, &plan.filter_graph, VideoEncoder::choose(toolchain));

        let mut child = background_command(toolchain.ffmpeg_path())
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(ExportError::CouldNotStart)?;
        let (Some(standard_output), Some(standard_error)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            return Err(ExportError::CouldNotStart(io::Error::other(
                "the exporter's pipes were not created",
            )));
        };

        let child = Arc::new(Mutex::new(child));
        let cancel_requested = Arc::new(AtomicBool::new(false));
        let (sender, events) = mpsc::channel();
        let error_output = collect_error_output_in_background(standard_error);

        let finishing = Finishing {
            child: Arc::clone(&child),
            cancel_requested: Arc::clone(&cancel_requested),
            error_output,
            temporary_output,
            destination: destination.to_owned(),
            sender,
            wake_up,
            _working_directory: working_directory,
        };
        let finisher = thread::spawn(move || report_progress_until_done(standard_output, finishing));

        Ok(Self {
            child,
            cancel_requested,
            events,
            finisher: Some(finisher),
        })
    }

    /// The next event, if one is ready. Never waits.
    pub fn try_next_event(&self) -> Option<ExportEvent> {
        self.events.try_recv().ok()
    }

    /// Stops the export. A [`ExportResult::Cancelled`] event follows once ffmpeg has stopped and
    /// its temporary file has been removed.
    pub fn cancel(&self) {
        self.cancel_requested.store(true, Ordering::SeqCst);
        let _ = lock(&self.child).kill();
    }
}

impl Drop for VideoExport {
    fn drop(&mut self) {
        self.cancel();
        if let Some(finisher) = self.finisher.take() {
            let _ = finisher.join();
        }
    }
}

/// Everything the background thread needs to bring an export to its end.
struct Finishing {
    child: Arc<Mutex<Child>>,
    cancel_requested: Arc<AtomicBool>,
    error_output: JoinHandle<String>,
    temporary_output: PathBuf,
    destination: PathBuf,
    sender: Sender<ExportEvent>,
    wake_up: UserInterfaceWakeUp,
    /// Kept only so that the working files are deleted when the thread ends.
    _working_directory: TempDir,
}

fn report_progress_until_done(standard_output: ChildStdout, finishing: Finishing) {
    let Finishing {
        child,
        cancel_requested,
        error_output,
        temporary_output,
        destination,
        sender,
        wake_up,
        // Held (not dropped) until the end of this function, so the working files stay available
        // to ffmpeg for as long as it runs.
        _working_directory,
    } = finishing;

    for line in BufReader::new(standard_output).lines() {
        let Ok(line) = line else { break };
        if let Some(frames_written) = frames_written_in_progress_line(&line) {
            let _ = sender.send(ExportEvent::Progress { frames_written });
            wake_up();
        }
    }

    // Wait for ffmpeg to be completely finished without holding the lock while waiting, so that
    // a cancel request can still get in.
    let exit_status = loop {
        match lock(&child).try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => break Err(error),
        }
    };
    let error_output = error_output.join().unwrap_or_default();

    let result = match exit_status {
        Ok(status) if status.success() => match fs::rename(&temporary_output, &destination) {
            Ok(()) => ExportResult::Completed,
            Err(error) => ExportResult::Failed {
                reason: format!(
                    "the finished video could not be moved to {}: {error}",
                    destination.display()
                ),
            },
        },
        _ if cancel_requested.load(Ordering::SeqCst) => ExportResult::Cancelled,
        Ok(status) => ExportResult::Failed {
            reason: if error_output.is_empty() {
                format!("ffmpeg stopped with {status}")
            } else {
                error_output
            },
        },
        Err(error) => ExportResult::Failed {
            reason: format!("ffmpeg could not be waited for: {error}"),
        },
    };
    if result != ExportResult::Completed {
        let _ = fs::remove_file(&temporary_output);
    }
    let _ = sender.send(ExportEvent::Finished(result));
    wake_up();
}

fn lock(child: &Mutex<Child>) -> MutexGuard<'_, Child> {
    child.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether `destination` is the very file `source_file_path` (so that exporting would destroy
/// the original).
fn is_the_same_file(source_file_path: &Path, destination: &Path) -> bool {
    let Ok(canonical_source) = fs::canonicalize(source_file_path) else {
        return false;
    };
    let canonical_destination = fs::canonicalize(destination).ok().or_else(|| {
        // The destination does not exist yet, so find where it would be.
        let folder = destination
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        Some(fs::canonicalize(folder).ok()?.join(destination.file_name()?))
    });
    canonical_destination == Some(canonical_source)
}

/// Where the video is written until it is complete: beside the destination, with `.exporting`
/// before the extension so that ffmpeg still recognises the file type.
fn temporary_output_path(destination: &Path) -> PathBuf {
    let stem = destination
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match destination.extension() {
        Some(extension) => format!("{stem}.exporting.{}", extension.to_string_lossy()),
        None => format!("{stem}.exporting"),
    };
    destination.with_file_name(name)
}

/// Saves the filter graph and the still pictures where ffmpeg can read them.
fn write_working_files(
    working_directory: &TempDir,
    plan: &ExportPlan,
    source_file_locations: &[MediaLocation],
    output: &Path,
) -> Result<ExportFiles, ExportError> {
    let filter_graph_file = working_directory.path().join("filter_graph.txt");
    fs::write(&filter_graph_file, &plan.filter_graph.text).map_err(ExportError::CouldNotPrepare)?;

    let mut inputs = source_file_locations.to_vec();
    for (number, still_picture) in plan.still_pictures.iter().enumerate() {
        let path = working_directory.path().join(format!("still_picture_{number}.png"));
        let png_bytes = still_picture
            .picture
            .to_png_bytes()
            .map_err(|error| ExportError::CouldNotPrepare(io::Error::other(error)))?;
        fs::write(&path, png_bytes).map_err(ExportError::CouldNotPrepare)?;
        inputs.push(path.into());
    }

    Ok(ExportFiles {
        inputs,
        filter_graph_file,
        output: output.to_owned(),
    })
}
