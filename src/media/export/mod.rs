// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What is left of the export in the media module: the filter graph of the **sound** (the finished video's and the live preview's alike), how the video is
//! compressed, and reading what ffmpeg says of its progress. The export itself, the picture of it above all, is made by the picture engine: see `video_export`.

mod filter_graph;
mod progress;
mod sound_plan;
mod video_encoder;

pub(crate) use filter_graph::FILTER_GRAPH_FILE_OPTION;
pub use filter_graph::{
    AudioGraph, LoudnessToExport, SAMPLE_RATE, SoundClipToExport, SoundGraphSpecification, SoundItemToExport,
    SoundStripToExport, build_audio_graph, clip_feed_filters,
};
pub use progress::frames_written_in_progress_line;
pub use sound_plan::{sound_graph_specification, sources_of_sound};
pub use video_encoder::VideoEncoder;
