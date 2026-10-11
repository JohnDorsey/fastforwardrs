//! The live preview's engine: which frame of the timeline is current while playing, and which
//! picture of the source video the preview must show for it.
//!
//! * [`PlaybackClock`] says which timeline frame is current (it follows the real time while
//!   playing).
//! * [`wanted_pictures`] says which pictures of which source videos that frame needs: one for each
//!   video that is seen.
//! * [`PictureSupplier`] gets the picture of one video, by running ffmpeg as little as it can, and
//!   [`PreviewPictures`] has one for each track that has a video that is seen.

mod audio_device;
mod audio_splice;
mod audio_supplier;
mod clock;
mod picture_size;
mod preview_pictures;
#[cfg(test)]
mod scripted_streams;
mod supplier;
mod wanted;

pub use audio_device::{AudioDeviceError, DeviceAudioSink, opener_of_the_default_speakers};
pub use audio_splice::splice_waiting_sound;
pub use audio_supplier::{
    AudioContent, AudioSink, AudioSinkOpener, AudioSource, AudioStreamStarter, AudioSupplier, ClockReading,
    FfmpegAudioStreamStarter, LARGEST_DIFFERENCE_TO_CORRECT_IN_SECONDS, MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS,
    SECONDS_OF_SOUND_KEPT_WAITING, SMALLEST_DIFFERENCE_TO_CORRECT_IN_SECONDS,
};
pub use clock::PlaybackClock;
pub use picture_size::{LARGEST_PREVIEW_HEIGHT_IN_PIXELS, LARGEST_PREVIEW_WIDTH_IN_PIXELS, preview_picture_size};
pub use preview_pictures::{PicturesReport, PreviewPictures};
pub use supplier::{
    FfmpegPictureStreamStarter, MINIMUM_TIME_BETWEEN_STREAM_STARTS, MOST_PICTURES_TO_WAIT_FOR,
    PICTURES_BEFORE_THE_END_TO_GET_THE_NEXT_CLIP_READY, PictureStream, PictureStreamStarter, PictureSupplier,
    PreviewSetup, PreviewVideo, SuppliedPicture, SupplyReport,
};
pub use wanted::{UpcomingVideo, WantedPicture, wanted_pictures};
