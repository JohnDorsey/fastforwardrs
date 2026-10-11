//! The pictures of all the videos that the preview shows at once.
//!
//! Several videos can be seen at a frame (a small one over a big one, say), and each is a stream of
//! pictures of its own, so there is a [`PictureSupplier`] for each track that has a video that is
//! seen. A track that has none to show lets go of its supplier, and with it of ffmpeg's work for it.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::supplier::{PictureStreamStarter, PictureSupplier, PreviewSetup, SuppliedPicture, SupplyReport};
use super::wanted::WantedPicture;
use crate::project::TrackId;

/// What supplying the pictures came to: how each track's picture is, and when to ask again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PicturesReport {
    reports: BTreeMap<TrackId, SupplyReport>,
    /// If set, ask again after this long even if nothing else happens (a stream was put off).
    pub ask_again_in: Option<Duration>,
}

impl PicturesReport {
    /// Whether the picture of `track` is the one that was wanted of it (rather than an older one,
    /// or none, while the wanted one is still being got ready). `false` for a track nothing was wanted of.
    pub fn shows_the_wanted_picture_of(&self, track: TrackId) -> bool {
        self.reports
            .get(&track)
            .is_some_and(|report| report.shows_the_wanted_picture)
    }
}

/// Keeps the preview supplied with a picture of every video that is seen. See the module documentation.
pub struct PreviewPictures<Starter: PictureStreamStarter + Clone> {
    starter: Starter,
    setup: Option<PreviewSetup>,
    suppliers: BTreeMap<TrackId, PictureSupplier<Starter>>,
}

impl<Starter: PictureStreamStarter + Clone> PreviewPictures<Starter> {
    pub fn new(starter: Starter) -> Self {
        Self {
            starter,
            setup: None,
            suppliers: BTreeMap::new(),
        }
    }

    /// Chooses the videos pictures are made from (or none). Everything in progress is dropped.
    pub fn set_setup(&mut self, setup: Option<PreviewSetup>) {
        self.suppliers.clear();
        self.setup = setup;
    }

    pub fn setup(&self) -> Option<&PreviewSetup> {
        self.setup.as_ref()
    }

    /// Does whatever is needed to get the `wanted` pictures, which are one for each track at most,
    /// and says how that is going. Call it each time the screen is drawn. While `is_playing` it also
    /// gets the next clip of each track ready ahead of time.
    pub fn supply(&mut self, wanted: &[WantedPicture], is_playing: bool, now: Instant) -> PicturesReport {
        self.suppliers
            .retain(|track, _| wanted.iter().any(|wanted| wanted.track == *track));
        let mut report = PicturesReport::default();
        for wanted in wanted {
            let supplier = self.suppliers.entry(wanted.track).or_insert_with(|| {
                let mut supplier = PictureSupplier::new(self.starter.clone());
                supplier.set_setup(self.setup.clone());
                supplier
            });
            let supplied = supplier.supply(Some(*wanted), is_playing, now);
            report.ask_again_in = match (report.ask_again_in, supplied.ask_again_in) {
                (Some(first), Some(second)) => Some(first.min(second)),
                (first, second) => first.or(second),
            };
            report.reports.insert(wanted.track, supplied);
        }
        report
    }

    /// The latest picture got of the video of `track`, which may be older than the one wanted.
    pub fn picture_of(&self, track: TrackId) -> Option<&SuppliedPicture> {
        self.suppliers.get(&track)?.picture()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::playback::scripted_streams::{Script, ScriptedStarter};
    use crate::playback::supplier::PreviewVideo;
    use crate::project::{FrameRate, SourceFrameIndex, SourceMediaIdentifier, SpeedFactor};

    const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);
    const ANOTHER_VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(2);

    fn setup() -> PreviewSetup {
        PreviewSetup {
            frame_rate: FrameRate::new(30, 1).unwrap(),
            picture_width_in_pixels: 64,
            picture_height_in_pixels: 36,
            videos: vec![
                PreviewVideo {
                    source: VIDEO,
                    video_path: PathBuf::from("holiday.mp4"),
                    display_size_in_pixels: (1280, 720),
                },
                PreviewVideo {
                    source: ANOTHER_VIDEO,
                    video_path: PathBuf::from("birthday.mp4"),
                    display_size_in_pixels: (720, 1280),
                },
            ],
        }
    }

    fn pictures() -> (PreviewPictures<ScriptedStarter>, Script) {
        let script = Script::default();
        let mut pictures = PreviewPictures::new(ScriptedStarter(script.clone()));
        pictures.set_setup(Some(setup()));
        (pictures, script)
    }

    fn wanted(track: u32, source: SourceMediaIdentifier, source_frame: u64) -> WantedPicture {
        WantedPicture {
            track: TrackId::new(track),
            source,
            source_frame: SourceFrameIndex::new(source_frame),
            speed_factor: SpeedFactor::NORMAL,
            range_end_source_frame: SourceFrameIndex::new(source_frame + 100),
            pictures_left: 100,
            next_video: None,
        }
    }

    fn shown_frame_number(pictures: &PreviewPictures<ScriptedStarter>, track: u32) -> Option<u8> {
        pictures
            .picture_of(TrackId::new(track))
            .map(|picture| picture.frame.rgba_pixels[0])
    }

    #[test]
    fn each_track_that_wants_a_picture_gets_a_stream_of_its_own_for_its_own_video() {
        let (mut pictures, script) = pictures();
        pictures.supply(
            &[wanted(0, VIDEO, 50), wanted(1, ANOTHER_VIDEO, 7)],
            false,
            Instant::now(),
        );

        let requests = script.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].video_path, PathBuf::from("holiday.mp4"));
        assert_eq!(requests[0].first_source_frame, SourceFrameIndex::new(50));
        assert_eq!(requests[1].video_path, PathBuf::from("birthday.mp4"));
        assert_eq!(requests[1].first_source_frame, SourceFrameIndex::new(7));
    }

    #[test]
    fn the_pictures_of_the_tracks_arrive_and_are_kept_apart() {
        let (mut pictures, script) = pictures();
        let wanted_pictures = [wanted(0, VIDEO, 50), wanted(1, ANOTHER_VIDEO, 7)];
        let now = Instant::now();
        pictures.supply(&wanted_pictures, false, now);
        script.deliver(0, 1);
        script.deliver(1, 1);

        let report = pictures.supply(&wanted_pictures, false, now);

        assert_eq!(shown_frame_number(&pictures, 0), Some(50));
        assert_eq!(shown_frame_number(&pictures, 1), Some(7));
        assert!(report.shows_the_wanted_picture_of(TrackId::new(0)));
        assert!(report.shows_the_wanted_picture_of(TrackId::new(1)));
        assert!(
            !report.shows_the_wanted_picture_of(TrackId::new(2)),
            "nothing was wanted of it"
        );
    }

    #[test]
    fn a_track_whose_picture_has_not_arrived_says_so_while_another_whose_has_does_not_wait_for_it() {
        let (mut pictures, script) = pictures();
        let wanted_pictures = [wanted(0, VIDEO, 50), wanted(1, ANOTHER_VIDEO, 7)];
        let now = Instant::now();
        pictures.supply(&wanted_pictures, false, now);
        script.deliver(1, 1);

        let report = pictures.supply(&wanted_pictures, false, now);

        assert!(!report.shows_the_wanted_picture_of(TrackId::new(0)));
        assert!(report.shows_the_wanted_picture_of(TrackId::new(1)));
        assert_eq!(shown_frame_number(&pictures, 0), None);
    }

    #[test]
    fn a_stream_goes_on_from_one_call_to_the_next_for_a_track_that_still_wants_pictures_from_it() {
        let (mut pictures, script) = pictures();
        let now = Instant::now();
        pictures.supply(&[wanted(0, VIDEO, 50), wanted(1, ANOTHER_VIDEO, 7)], true, now);
        script.deliver(0, 3);
        script.deliver(1, 3);

        pictures.supply(&[wanted(0, VIDEO, 52), wanted(1, ANOTHER_VIDEO, 9)], true, now);

        assert_eq!(
            script.started_count(),
            2,
            "the streams that were started are the ones that went on"
        );
        assert_eq!(shown_frame_number(&pictures, 0), Some(52));
        assert_eq!(shown_frame_number(&pictures, 1), Some(9));
    }

    fn serial_number_of(pictures: &PreviewPictures<ScriptedStarter>, track: u32) -> u64 {
        pictures.picture_of(TrackId::new(track)).unwrap().serial_number
    }

    /// The picture a track has on the graphics card is told from the one before by its serial number,
    /// so two pictures of different tracks, or one from a supplier that was dropped and made again,
    /// must never share one.
    #[test]
    fn no_two_pictures_have_the_same_serial_number_not_for_different_tracks_nor_for_a_supplier_made_again() {
        let (mut pictures, script) = pictures();
        let now = Instant::now();
        let both = [wanted(0, VIDEO, 50), wanted(1, ANOTHER_VIDEO, 7)];
        pictures.supply(&both, false, now);
        script.deliver(0, 1);
        script.deliver(1, 1);
        pictures.supply(&both, false, now);
        let (first, second) = (serial_number_of(&pictures, 0), serial_number_of(&pictures, 1));
        assert_ne!(first, second, "pictures of different tracks");

        // Track 0 wants nothing for a moment, so its supplier goes; then it wants a picture again.
        pictures.supply(&[wanted(1, ANOTHER_VIDEO, 7)], false, now);
        pictures.supply(&[wanted(0, VIDEO, 50), wanted(1, ANOTHER_VIDEO, 7)], false, now);
        script.deliver(2, 1);
        pictures.supply(&[wanted(0, VIDEO, 50), wanted(1, ANOTHER_VIDEO, 7)], false, now);
        let again = serial_number_of(&pictures, 0);
        assert!(again != first && again != second, "{first}, {second} and {again}");
    }

    #[test]
    fn a_track_that_wants_nothing_any_more_lets_go_of_its_picture_and_the_others_keep_theirs() {
        let (mut pictures, script) = pictures();
        let now = Instant::now();
        pictures.supply(&[wanted(0, VIDEO, 50), wanted(1, ANOTHER_VIDEO, 7)], false, now);
        script.deliver(0, 1);
        script.deliver(1, 1);
        pictures.supply(&[wanted(0, VIDEO, 50), wanted(1, ANOTHER_VIDEO, 7)], false, now);

        pictures.supply(&[wanted(1, ANOTHER_VIDEO, 7)], false, now);

        assert_eq!(shown_frame_number(&pictures, 0), None);
        assert_eq!(shown_frame_number(&pictures, 1), Some(7));
        assert_eq!(script.started_count(), 2, "nothing was started again");
    }

    #[test]
    fn nothing_wanted_at_all_lets_go_of_everything() {
        let (mut pictures, script) = pictures();
        let now = Instant::now();
        pictures.supply(&[wanted(0, VIDEO, 50)], false, now);
        script.deliver(0, 1);
        pictures.supply(&[wanted(0, VIDEO, 50)], false, now);
        assert_eq!(shown_frame_number(&pictures, 0), Some(50));

        let report = pictures.supply(&[], false, now);

        assert_eq!(shown_frame_number(&pictures, 0), None);
        assert_eq!(report, PicturesReport::default());
    }

    #[test]
    fn choosing_other_videos_drops_every_picture_and_stream() {
        let (mut pictures, script) = pictures();
        let now = Instant::now();
        pictures.supply(&[wanted(0, VIDEO, 50)], false, now);
        script.deliver(0, 1);
        pictures.supply(&[wanted(0, VIDEO, 50)], false, now);

        pictures.set_setup(Some(PreviewSetup {
            picture_width_in_pixels: 32,
            ..setup()
        }));

        assert_eq!(shown_frame_number(&pictures, 0), None);
        assert_eq!(pictures.setup().unwrap().picture_width_in_pixels, 32);
        pictures.supply(&[wanted(0, VIDEO, 50)], false, now);
        assert_eq!(
            script.requests()[1].picture_width_in_pixels,
            32,
            "a new stream, at the new size"
        );
    }

    #[test]
    fn without_a_setup_nothing_is_started() {
        let script = Script::default();
        let mut pictures = PreviewPictures::new(ScriptedStarter(script.clone()));
        pictures.supply(&[wanted(0, VIDEO, 50)], false, Instant::now());
        assert_eq!(script.started_count(), 0);
        assert!(pictures.setup().is_none());
    }

    #[test]
    fn asking_again_is_for_the_soonest_of_the_tracks_that_put_a_stream_off() {
        let (mut pictures, script) = pictures();
        let start = Instant::now();
        pictures.supply(&[wanted(0, VIDEO, 50)], false, start);
        // Track 0 jumps, 10 ms after it started a stream: too soon, so that is put off by 30 ms.
        let ten_ms_on = start + Duration::from_millis(10);
        let report = pictures.supply(&[wanted(0, VIDEO, 150), wanted(1, ANOTHER_VIDEO, 7)], false, ten_ms_on);
        assert_eq!(report.ask_again_in, Some(Duration::from_millis(30)));
        assert_eq!(
            script.started_count(),
            2,
            "the new track's own stream was started at once"
        );
    }
}
