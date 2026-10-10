//! Exporting a project as a finished video file.
//!
//! The export has three parts, each testable on its own:
//!
//! * [`plan_export`] works out what has to happen, from a project (pure logic);
//! * [`build_filter_graph`] and [`export_arguments`] turn that into the text ffmpeg is given (pure
//!   text building);
//! * [`VideoExport`] runs ffmpeg, reports its progress and can cancel it.

mod command;
mod filter_graph;
mod plan;
mod progress;
mod runner;

pub(crate) use command::filter_graph_file_option;
pub use command::{ExportFiles, VideoEncoder, export_arguments};
pub use filter_graph::{
    AudioGraph, AudioGraphSpecification, ClipToExport, FilterGraph, FilterGraphSpecification, IndicatorOverlay,
    build_audio_graph, build_filter_graph,
};
pub use plan::{ExportPlan, ExportPlanError, SymbolInput, clips_to_export, plan_export};
pub use progress::frames_written_in_progress_line;
pub use runner::{ExportError, ExportEvent, ExportResult, VideoExport};
