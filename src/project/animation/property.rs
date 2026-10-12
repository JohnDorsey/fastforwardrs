//! What can be animated, and the values it takes: the properties of clips and of tracks that can change over
//! time, each with the kind of value it has.

use crate::project::tracks::{ClipKind, TextColour};

/// A property that can be animated: given keyframes, so that it is not the same at every frame.
///
/// Everything that is a setting of a clip or of a track can be, except what makes the clip last longer or
/// shorter (its speed, the speed of its track, and how long a picture or a text object is shown for), which
/// would make the keyframes move about in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PropertyId {
    /// How loud a clip or a track is, in decibels.
    Gain,
    /// Whether a clip or a track is silent.
    Muted,
    /// Where the middle of a video, a picture or a text object is on the picture.
    Position,
    /// How wide and tall the box of a video, a picture or a text object is.
    Size,
    /// How far a video, a picture or a text object is turned, in degrees.
    Rotation,
    /// What a text object says.
    Text,
    /// The font of a text object.
    FontFamily,
    /// How big the letters of a text object are.
    FontSize,
    /// The colour of the letters of a text object.
    Colour,
}

/// What a property's value is made of, which says how it changes between two keyframes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    /// One number, which changes evenly.
    Number,
    /// Two numbers (across and down, or wide and tall), which change evenly.
    Pair,
    /// A colour, whose red, green, blue and clearness change evenly.
    Colour,
    /// Yes or no, which stays what it was until the next keyframe.
    Flag,
    /// Words, which stay what they were until the next keyframe.
    Words,
}

impl PropertyId {
    /// Every property, in the order they are kept and shown in.
    pub const ALL: [PropertyId; 9] = [
        Self::Gain,
        Self::Muted,
        Self::Position,
        Self::Size,
        Self::Rotation,
        Self::Text,
        Self::FontFamily,
        Self::FontSize,
        Self::Colour,
    ];

    /// What the value of the property is made of.
    pub fn kind(self) -> ValueKind {
        match self {
            Self::Gain | Self::Rotation | Self::FontSize => ValueKind::Number,
            Self::Position | Self::Size => ValueKind::Pair,
            Self::Colour => ValueKind::Colour,
            Self::Muted => ValueKind::Flag,
            Self::Text | Self::FontFamily => ValueKind::Words,
        }
    }

    /// The name the property is saved under in a project file, and which is how it is called to people.
    pub fn name(self) -> &'static str {
        match self {
            Self::Gain => "gain",
            Self::Muted => "muted",
            Self::Position => "position",
            Self::Size => "size",
            Self::Rotation => "rotation",
            Self::Text => "text",
            Self::FontFamily => "font",
            Self::FontSize => "font size",
            Self::Colour => "colour",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|property| property.name() == name)
    }

    /// Whether a track has this property of its own. (How loud it is, and whether it is silent.)
    pub fn applies_to_a_track(self) -> bool {
        self.is_heard()
    }

    /// Whether what the property is can be heard: how loud something is, and whether it is silent.
    pub fn is_heard(self) -> bool {
        matches!(self, Self::Gain | Self::Muted)
    }

    /// Whether what the property is can be seen in the picture: where something is and how it looks.
    pub fn is_seen(self) -> bool {
        !self.is_heard()
    }

    /// Whether a clip of this kind has this property: a video or a sound has its loudness and its mute switch, a
    /// video, a picture and a text object are somewhere on the picture, and a text object has words with a font, a size
    /// and a colour.
    pub fn applies_to_a_clip_of_kind(self, kind: ClipKind) -> bool {
        match self {
            Self::Gain | Self::Muted => matches!(kind, ClipKind::Video | ClipKind::Audio),
            Self::Position | Self::Size | Self::Rotation => !matches!(kind, ClipKind::Audio),
            Self::Text | Self::FontFamily | Self::FontSize | Self::Colour => kind == ClipKind::Text,
        }
    }
}

/// The value of a property at one moment.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Number(f32),
    /// Across and down, or wide and tall, as fractions of the picture.
    Pair(f32, f32),
    Colour(TextColour),
    Flag(bool),
    Words(String),
}

impl PropertyValue {
    /// Feeds the value into `hasher`, so that what it is can be told from another value that it is not equal to. (There is
    /// no `Hash` for values, as there are numbers with fractions in them.)
    pub fn hash_into(&self, hasher: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        match self {
            Self::Number(number) => (0_u8, number.to_bits()).hash(hasher),
            Self::Pair(first, second) => (1_u8, first.to_bits(), second.to_bits()).hash(hasher),
            Self::Colour(colour) => (2_u8, colour).hash(hasher),
            Self::Flag(flag) => (3_u8, flag).hash(hasher),
            Self::Words(words) => (4_u8, words).hash(hasher),
        }
    }

    pub fn kind(&self) -> ValueKind {
        match self {
            Self::Number(_) => ValueKind::Number,
            Self::Pair(..) => ValueKind::Pair,
            Self::Colour(_) => ValueKind::Colour,
            Self::Flag(_) => ValueKind::Flag,
            Self::Words(_) => ValueKind::Words,
        }
    }

    /// The value that is `fraction` (0 to 1) of the way from this one to `other`: what changes evenly does so, and
    /// what cannot (yes or no, words) stays what this value is until `fraction` is all of the way, which is
    /// `other`. (Values of different kinds are not between anything: this one stays.)
    #[must_use]
    pub fn blended_towards(&self, other: &Self, fraction: f32) -> Self {
        let evenly = |from: f32, to: f32| from + (to - from) * fraction;
        match (self, other) {
            (Self::Number(from), Self::Number(to)) => Self::Number(evenly(*from, *to)),
            (Self::Pair(from_first, from_second), Self::Pair(to_first, to_second)) => {
                Self::Pair(evenly(*from_first, *to_first), evenly(*from_second, *to_second))
            }
            (Self::Colour(from), Self::Colour(to)) => {
                let channel = |from: u8, to: u8| evenly(f32::from(from), f32::from(to)).round().clamp(0.0, 255.0) as u8;
                Self::Colour(TextColour::new(
                    channel(from.red, to.red),
                    channel(from.green, to.green),
                    channel(from.blue, to.blue),
                    channel(from.alpha, to.alpha),
                ))
            }
            (Self::Flag(_), Self::Flag(_)) | (Self::Words(_), Self::Words(_)) => {
                if fraction >= 1.0 {
                    other.clone()
                } else {
                    self.clone()
                }
            }
            _ => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_property_has_a_name_that_only_it_has_and_that_it_is_found_by() {
        let mut names: Vec<&str> = PropertyId::ALL.iter().map(|property| property.name()).collect();
        for property in PropertyId::ALL {
            assert_eq!(PropertyId::from_name(property.name()), Some(property));
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PropertyId::ALL.len());
        assert_eq!(PropertyId::from_name("speed"), None, "speeds are not animated");
        assert_eq!(PropertyId::from_name("duration"), None);
    }

    #[test]
    fn each_kind_of_clip_and_a_track_have_the_properties_they_have() {
        let of = |kind: ClipKind| -> Vec<PropertyId> {
            PropertyId::ALL
                .into_iter()
                .filter(|property| property.applies_to_a_clip_of_kind(kind))
                .collect()
        };
        use PropertyId::*;
        assert_eq!(of(ClipKind::Video), vec![Gain, Muted, Position, Size, Rotation]);
        assert_eq!(of(ClipKind::Audio), vec![Gain, Muted]);
        assert_eq!(of(ClipKind::Image), vec![Position, Size, Rotation]);
        assert_eq!(
            of(ClipKind::Text),
            vec![Position, Size, Rotation, Text, FontFamily, FontSize, Colour]
        );
        let of_a_track: Vec<PropertyId> = PropertyId::ALL
            .into_iter()
            .filter(|property| property.applies_to_a_track())
            .collect();
        assert_eq!(of_a_track, vec![Gain, Muted]);
    }

    #[test]
    fn the_list_of_properties_is_in_the_order_they_are_kept_in_and_has_each_once() {
        let mut sorted = PropertyId::ALL;
        sorted.sort();
        assert_eq!(sorted, PropertyId::ALL);
        sorted.windows(2).for_each(|pair| assert_ne!(pair[0], pair[1]));
    }

    #[test]
    fn what_changes_evenly_does_and_yes_or_no_and_words_wait_for_the_next_keyframe() {
        let number = PropertyValue::Number(10.0).blended_towards(&PropertyValue::Number(20.0), 0.25);
        assert_eq!(number, PropertyValue::Number(12.5));
        let pair = PropertyValue::Pair(0.0, 1.0).blended_towards(&PropertyValue::Pair(1.0, 0.0), 0.5);
        assert_eq!(pair, PropertyValue::Pair(0.5, 0.5));
        let off = PropertyValue::Flag(false);
        assert_eq!(
            off.blended_towards(&PropertyValue::Flag(true), 0.99),
            PropertyValue::Flag(false)
        );
        assert_eq!(
            off.blended_towards(&PropertyValue::Flag(true), 1.0),
            PropertyValue::Flag(true)
        );
        let hello = PropertyValue::Words("Hello".to_owned());
        let goodbye = PropertyValue::Words("Goodbye".to_owned());
        assert_eq!(hello.blended_towards(&goodbye, 0.5), hello);
        assert_eq!(hello.blended_towards(&goodbye, 1.0), goodbye);
    }

    #[test]
    fn a_colour_changes_in_each_of_its_channels_and_stays_a_colour() {
        let black = PropertyValue::Colour(TextColour::new(0, 0, 0, 255));
        let white = PropertyValue::Colour(TextColour::new(255, 255, 255, 0));
        assert_eq!(
            black.blended_towards(&white, 0.5),
            PropertyValue::Colour(TextColour::new(128, 128, 128, 128))
        );
        assert_eq!(black.blended_towards(&white, 0.0), black);
        assert_eq!(black.blended_towards(&white, 1.0), white);
    }

    #[test]
    fn values_of_different_kinds_are_not_between_anything() {
        let number = PropertyValue::Number(3.0);
        assert_eq!(number.blended_towards(&PropertyValue::Flag(true), 0.5), number);
        assert_eq!(
            number.blended_towards(&PropertyValue::Flag(true), 1.0),
            number,
            "not even all of the way"
        );
    }

    #[test]
    fn every_value_knows_its_kind_and_the_kind_the_property_it_is_for_has() {
        assert_eq!(PropertyValue::Number(0.0).kind(), PropertyId::Gain.kind());
        assert_eq!(PropertyValue::Pair(0.0, 0.0).kind(), PropertyId::Position.kind());
        assert_eq!(PropertyValue::Pair(0.0, 0.0).kind(), PropertyId::Size.kind());
        assert_eq!(PropertyValue::Number(0.0).kind(), PropertyId::Rotation.kind());
        assert_eq!(PropertyValue::Number(0.0).kind(), PropertyId::FontSize.kind());
        assert_eq!(PropertyValue::Flag(true).kind(), PropertyId::Muted.kind());
        assert_eq!(PropertyValue::Words(String::new()).kind(), PropertyId::Text.kind());
        assert_eq!(
            PropertyValue::Words(String::new()).kind(),
            PropertyId::FontFamily.kind()
        );
        assert_eq!(
            PropertyValue::Colour(TextColour::WHITE).kind(),
            PropertyId::Colour.kind()
        );
    }
}
