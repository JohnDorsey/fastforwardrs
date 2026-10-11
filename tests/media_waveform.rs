//! Making the waveform of a real sound file with the real ffmpeg.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::media::{PEAKS_PER_SECOND, Waveform, WaveformEvent, WaveformExtraction};
use support::tone_file;

/// Waits for the extraction to finish and returns the waveform it made and the complaint it ended with.
fn finished_waveform(extraction: &WaveformExtraction) -> (Waveform, Option<String>) {
    let mut waveform = Waveform::default();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        match extraction.try_next_event() {
            Some(WaveformEvent::Peaks(peaks)) => waveform.extend(&peaks),
            Some(WaveformEvent::Finished { error_output }) => return (waveform, error_output),
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    panic!("the waveform was not finished in 20 seconds");
}

#[test]
fn a_steady_tone_has_a_steady_waveform_with_a_hundred_peaks_a_second() {
    let toolchain = toolchain_or_skip!();
    let tone = tone_file(&toolchain, 440, 3);

    let extraction = WaveformExtraction::start(&toolchain, &tone, Arc::new(|| {})).unwrap();
    let (waveform, error) = finished_waveform(&extraction);

    assert_eq!(error, None);
    let expected_peaks = 3 * PEAKS_PER_SECOND as usize;
    assert!(
        waveform.peak_count().abs_diff(expected_peaks) <= 2,
        "3 seconds of tone make about {expected_peaks} peaks, not {}",
        waveform.peak_count()
    );
    // The tone has an amplitude of an eighth of the loudest sound there can be: 255 / 8 = 32.
    for (index, peak) in waveform
        .peaks()
        .iter()
        .enumerate()
        .skip(2)
        .take(waveform.peak_count() - 4)
    {
        assert!((28..=36).contains(peak), "peak {index} is {peak}");
    }
    assert!(
        waveform
            .loudest_between(1.0, 2.0)
            .is_some_and(|peak| (28..=36).contains(&peak)),
        "the middle second is as loud as the rest"
    );
}

#[test]
fn a_file_that_is_not_a_sound_ends_with_the_complaint_and_no_peaks() {
    let toolchain = toolchain_or_skip!();
    let folder = support::scratch_directory();
    let not_a_sound = folder.path().join("notes.wav");
    std::fs::write(&not_a_sound, "this is text").unwrap();

    let extraction = WaveformExtraction::start(&toolchain, &not_a_sound, Arc::new(|| {})).unwrap();
    let (waveform, error) = finished_waveform(&extraction);

    assert_eq!(waveform.peak_count(), 0);
    assert!(
        error.is_some_and(|text| !text.is_empty()),
        "ffmpeg's complaint is passed on"
    );
}
