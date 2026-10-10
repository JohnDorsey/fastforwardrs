//! Reading the progress that ffmpeg reports while it exports.
//!
//! With `-progress pipe:1`, ffmpeg prints a block of `key=value` lines every half second or so,
//! such as `frame=240`, `fps=61.5`, `out_time=00:00:08.000000` and `progress=continue` (or
//! `progress=end` in the last block). Only the number of frames written is needed.

/// How many frames ffmpeg says it has written, if `line` is its `frame=` line.
pub fn frames_written_in_progress_line(line: &str) -> Option<u64> {
    line.trim().strip_prefix("frame=")?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_frame_count() {
        assert_eq!(frames_written_in_progress_line("frame=240"), Some(240));
        assert_eq!(frames_written_in_progress_line("frame=0"), Some(0));
        assert_eq!(frames_written_in_progress_line("  frame= 17 \r\n"), Some(17));
    }

    #[test]
    fn ignores_every_other_line() {
        for line in [
            "fps=61.5",
            "out_time=00:00:08.000000",
            "progress=continue",
            "frame_count=5",
            "",
            "frame=abc",
            "frame=-3",
            "frame=1.5",
        ] {
            assert_eq!(frames_written_in_progress_line(line), None, "{line:?}");
        }
    }
}
