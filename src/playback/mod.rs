//! The live preview's engine: which frame of the timeline is current while playing, and which
//! picture of the source video the preview must show for it.
//!
//! * [`PlaybackClock`] says which timeline frame is current (it follows the real time while
//!   playing).
//! * [`wanted_picture`] says which picture of the source video that frame needs.
//! * [`PictureSupplier`] gets that picture, by running ffmpeg as little as it can.

mod clock;
mod picture_size;
mod supplier;
mod wanted;

pub use clock::PlaybackClock;
pub use picture_size::{LARGEST_PREVIEW_HEIGHT_IN_PIXELS, LARGEST_PREVIEW_WIDTH_IN_PIXELS, preview_picture_size};
pub use supplier::{
    FfmpegPictureStreamStarter, MINIMUM_TIME_BETWEEN_STREAM_STARTS, MOST_PICTURES_TO_WAIT_FOR,
    PICTURES_BEFORE_THE_END_TO_GET_THE_NEXT_CLIP_READY, PictureStream, PictureStreamStarter, PictureSupplier,
    PreviewSource, SuppliedPicture, SupplyReport,
};
pub use wanted::{UpcomingClip, WantedPicture, wanted_picture};
