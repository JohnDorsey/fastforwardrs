//! The exact shape of a project file on disk (the current version, 9).
//!
//! These types are deliberately separate from the editing model in [`crate::project`]: the file
//! format has to stay readable by future versions, while the model is free to change. Converting
//! between the two is where files from the outside world get validated. (Files of earlier
//! versions have their own types, in [`super::legacy`].)

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::project::{
    FrameRate, Gain, SourceFrameIndex, SourceMediaIdentifier, SpedUpClipSound, SpeedFactor, TextColour, TrackHeight,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectDocument {
    /// Always [`super::FILE_FORMAT_NAME`]; lets us recognise (and refuse) other JSON files.
    pub file_format: String,
    pub file_format_version: u32,
    pub sources: Vec<SourceDocument>,
    /// The tracks, the one in front first.
    pub tracks: Vec<TrackDocument>,
    pub speed_indicator: SpeedIndicatorDocument,
    /// What the project has set for itself. Left out when it follows every global setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ProjectSettingsDocument>,
    /// The frame rate the project was given. Left out while it follows its first video, and in the
    /// files of versions before 5 (in which it always did).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<FrameRate>,
}

/// The settings a project has set for itself. A setting that follows the global one is not in the
/// file at all, which is what "not saved" means for it.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectSettingsDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autosave: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sped_up_clip_sound: Option<SpedUpClipSound>,
    /// What versions that could only mute or not mute wrote instead of `sped_up_clip_sound`. It is
    /// read (when `sped_up_clip_sound` is not there) and never written.
    #[serde(default, skip_serializing)]
    pub mute_sped_up_clips: Option<bool>,
}

/// A file the project is made from.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum SourceDocument {
    Video {
        identifier: SourceMediaIdentifier,
        /// Where the file was, or is: where it was when it was included in the project file, for a file that is.
        absolute_path: String,
        /// Relative to the folder that holds the project file. Always written with `/` separators so
        /// that it means the same thing on every operating system.
        relative_path: Option<String>,
        /// The name of the entry of the project file that has the file in it, if the file is included in the project file
        /// (which has to be a `.ffrs` file, an archive, to have files in it). Left out for a file that is not, and in the files
        /// of versions before 9.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        included_as: Option<String>,
        frame_rate: FrameRate,
        total_frame_count: u64,
        display_width_in_pixels: u32,
        display_height_in_pixels: u32,
        has_audio: bool,
    },
    Audio {
        identifier: SourceMediaIdentifier,
        absolute_path: String,
        relative_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        included_as: Option<String>,
        duration_in_microseconds: u64,
    },
    Image {
        identifier: SourceMediaIdentifier,
        absolute_path: String,
        relative_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        included_as: Option<String>,
        display_width_in_pixels: u32,
        display_height_in_pixels: u32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrackDocument {
    pub title: String,
    pub gain_decibels: Gain,
    /// How much faster than normal everything on the track plays, on top of each clip's own speed.
    pub speed_factor: SpeedFactor,
    pub muted: bool,
    /// How tall the row of the track is on the timeline. Left out for a track of the normal
    /// height, and in the files of versions before 4, which had no heights.
    #[serde(default = "normal_height_in_points", skip_serializing_if = "is_the_normal_height")]
    pub height_in_points: f32,
    /// The keyframes of the track's own properties (how loud it is, and whether it is silent), counted in frames from the
    /// start of the timeline, as the files of versions before 7 had them: a rack of them in the drawer of the track is what is
    /// written now. It is read, and never written.
    #[serde(default, skip_serializing)]
    pub animation: AnimationDocument,
    /// The racks of keyframes that are kept with the track, whatever they are the keyframes of, their frames counted from the
    /// start of the timeline. Left out for a track that has none, and in the files of versions before 7.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawer: Vec<RackDocument>,
    /// The effects the track has, in the order they were put on in. Left out for a track that has none, and in the files of
    /// versions before 8.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectDocument>,
    pub clips: Vec<ClipDocument>,
}

fn normal_height_in_points() -> f32 {
    TrackHeight::DEFAULT_IN_POINTS
}

fn is_the_normal_height(points: &f32) -> bool {
    TrackHeight::from_points(*points).is_default()
}

/// One clip on a track, with how many frames of empty time come before it.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ClipDocument {
    Video {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        source_identifier: SourceMediaIdentifier,
        first_source_frame: SourceFrameIndex,
        /// The frame just after the clip's last frame.
        end_source_frame: SourceFrameIndex,
        speed_factor: SpeedFactor,
        #[serde(default, skip_serializing_if = "is_no_gain")]
        gain_decibels: Gain,
        #[serde(default, skip_serializing_if = "is_false")]
        muted: bool,
        /// Where the video is on the picture, if it was moved, resized or turned. Left out for a
        /// video that is simply fitted inside the picture.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placement: Option<PlacementDocument>,
        /// The keyframes of what is animated, counted in frames from the clip's first frame, as the files of versions
        /// before 7 had them: a rack of them in the drawer of the clip is what is written now. It is read, and never written.
        #[serde(default, skip_serializing)]
        animation: AnimationDocument,
        /// The racks of keyframes that are kept with the clip, whatever they are the keyframes of, their frames counted from
        /// the clip's first frame. Left out for a clip that has none, and in the files of versions before 7.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        /// The effects the video has, in the order they were put on in. Left out for a video that has none, and in the files
        /// of versions before 8.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectDocument>,
    },
    Audio {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        source_identifier: SourceMediaIdentifier,
        first_source_frame: SourceFrameIndex,
        end_source_frame: SourceFrameIndex,
        speed_factor: SpeedFactor,
        #[serde(default, skip_serializing_if = "is_no_gain")]
        gain_decibels: Gain,
        #[serde(default, skip_serializing_if = "is_false")]
        muted: bool,
        #[serde(default, skip_serializing)]
        animation: AnimationDocument,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
    },
    Image {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        source_identifier: SourceMediaIdentifier,
        duration_in_frames: u64,
        /// Where the picture is on the screen, if it was moved, resized or turned. Left out for a
        /// picture that is simply fitted inside the picture.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placement: Option<PlacementDocument>,
        #[serde(default, skip_serializing)]
        animation: AnimationDocument,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectDocument>,
    },
    Text {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        duration_in_frames: u64,
        text: String,
        /// Empty for the program's own default font.
        font_family: String,
        font_size_fraction_of_frame_height: f32,
        colour: ColourDocument,
        area: CanvasAreaDocument,
        /// How far the area is turned, clockwise. Left out for a text that is upright.
        #[serde(default, skip_serializing_if = "is_not_turned")]
        rotation_degrees: f32,
        #[serde(default, skip_serializing)]
        animation: AnimationDocument,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectDocument>,
    },
    /// A keyframe crate: a stretch of the track that has racks of keyframes in its drawer and nothing else.
    Crate {
        #[serde(default, skip_serializing_if = "is_zero")]
        gap_before_frames: u64,
        duration_in_frames: u64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        drawer: Vec<RackDocument>,
    },
}

/// An effect that a clip or a track has.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EffectDocument {
    /// The name of the kind of effect (see [`crate::project::EffectKind::name`]).
    pub effect: String,
    /// What its values are when they are not animated, one for each of the properties of the kind of effect, in their order.
    /// Left out when they are what the effect begins with. (The keyframes of its values are racks, like any other.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<KeyframeValueDocument>,
}

/// The keyframes of one property that are kept together, in a drawer.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RackDocument {
    /// The name of the property (see [`crate::project::PropertyId::name`]).
    pub property: String,
    /// The clip or the track that the keyframes are of. Left out when it is the one whose drawer the rack is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetDocument>,
    /// In the order of their frames, which are counted from the start of what the drawer belongs to (a clip's first frame, or the
    /// start of the timeline for a track).
    pub keyframes: Vec<KeyframeDocument>,
}

/// A clip or a track, by where it is in the file: the number of its track in the list of tracks, and for a clip the number of
/// the clip in the list of that track's clips, both counted from 0.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TargetDocument {
    pub track: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<usize>,
}

/// The keyframes of the properties that a clip or a track has animated, by the name of the property (see
/// [`crate::project::PropertyId::name`]), each property's in the order of their frames. A property that is not animated is
/// not in it.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct AnimationDocument(pub BTreeMap<String, Vec<KeyframeDocument>>);

/// What a property is at a frame.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct KeyframeDocument {
    /// How many frames after the first frame of the clip (or after the start of the timeline, for a track).
    pub frame: u64,
    pub value: KeyframeValueDocument,
}

/// The value of a keyframe, which is the kind of value that the property has: a number (how loud, in decibels; how far turned,
/// in degrees; how big the letters are), two numbers (a place, a size), yes or no, or text (words, the name of a font, a
/// colour as `#RRGGBBAA`). Which of them it has to be is told by the property, and checked when the file is loaded.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum KeyframeValueDocument {
    Flag(bool),
    Number(f32),
    Pair([f32; 2]),
    Text(String),
}

fn is_zero(number: &u64) -> bool {
    *number == 0
}

fn is_no_gain(gain: &Gain) -> bool {
    gain.is_unchanged()
}

fn is_false(flag: &bool) -> bool {
    !*flag
}

fn is_not_turned(degrees: &f32) -> bool {
    *degrees == 0.0
}

/// Where something is on the picture: a rectangle, and how far it is turned about its middle.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlacementDocument {
    pub area: CanvasAreaDocument,
    /// How far the area is turned, clockwise. Left out when it is not.
    #[serde(default, skip_serializing_if = "is_not_turned")]
    pub rotation_degrees: f32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CanvasAreaDocument {
    pub left_fraction_of_frame_width: f32,
    pub top_fraction_of_frame_height: f32,
    pub width_fraction_of_frame_width: f32,
    pub height_fraction_of_frame_height: f32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SpeedIndicatorDocument {
    pub left_fraction_of_frame_width: f32,
    pub top_fraction_of_frame_height: f32,
    pub height_fraction_of_frame_height: f32,
}

/// A colour, written as `#RRGGBBAA`: two hexadecimal digits each for red, green, blue and opacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ColourDocument(pub TextColour);

impl ColourDocument {
    pub(super) fn to_text(self) -> String {
        let TextColour {
            red,
            green,
            blue,
            alpha,
        } = self.0;
        format!("#{red:02X}{green:02X}{blue:02X}{alpha:02X}")
    }

    pub(super) fn from_text(text: &str) -> Result<Self, String> {
        let digits = text
            .strip_prefix('#')
            .filter(|digits| digits.len() == 8 && digits.chars().all(|digit| digit.is_ascii_hexdigit()))
            .ok_or_else(|| {
                format!("\"{text}\" is not a colour; expected \"#RRGGBBAA\", such as \"#FFFFFFFF\" for white")
            })?;
        let part = |position: usize| {
            u8::from_str_radix(&digits[position * 2..position * 2 + 2], 16).expect("the digits were checked")
        };
        Ok(Self(TextColour {
            red: part(0),
            green: part(1),
            blue: part(2),
            alpha: part(3),
        }))
    }
}

impl Serialize for ColourDocument {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_text())
    }
}

impl<'de> Deserialize<'de> for ColourDocument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_text(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_is_written_as_hexadecimal_red_green_blue_and_opacity() {
        assert_eq!(ColourDocument(TextColour::WHITE).to_text(), "#FFFFFFFF");
        assert_eq!(ColourDocument(TextColour::new(1, 2, 255, 0)).to_text(), "#0102FF00");
        assert_eq!(ColourDocument(TextColour::new(0, 0, 0, 128)).to_text(), "#00000080");
    }

    #[test]
    fn a_colour_comes_back_exactly_in_either_case() {
        for colour in [
            TextColour::WHITE,
            TextColour::new(0, 0, 0, 0),
            TextColour::new(18, 52, 86, 120),
            TextColour::new(255, 128, 1, 254),
        ] {
            let text = ColourDocument(colour).to_text();
            assert_eq!(
                ColourDocument::from_text(&text).unwrap(),
                ColourDocument(colour),
                "{text}"
            );
            assert_eq!(
                ColourDocument::from_text(&text.to_lowercase()).unwrap(),
                ColourDocument(colour)
            );
        }
    }

    #[test]
    fn text_that_is_not_a_colour_is_refused_with_what_was_expected() {
        for bad in [
            "",
            "FFFFFFFF",
            "#FFFFFF",
            "#FFFFFFFFF",
            "#GGGGGGGG",
            "#FFFFFF 0",
            "white",
            "#+FFFFFFF",
        ] {
            let reason = ColourDocument::from_text(bad).unwrap_err();
            assert!(reason.contains("#RRGGBBAA"), "{bad:?}: {reason}");
        }
    }
}
