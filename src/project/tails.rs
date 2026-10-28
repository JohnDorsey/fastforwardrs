// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What phase 12 (docs/engine-phase-12-design.md) asks of the model: how much of a track one instance of its effects covers (the **sharing** of a track), what a **tail** does at the edge
//! of a group (the **tail policies** of a clip and of a group), and how long an effect's tail is when its person says (the **tail length** of an effect).
//!
//! Nothing here changes what is heard or seen: the sound plan and the picture plan read these values in the pieces after this one (12.2 to 12.6). They are kept, edited, saved and shown
//! (12.1), so that the rules about them (a group that has effects cannot take part of a track that is indivisible) are in place before anything depends on them.

use serde::{Deserialize, Serialize};

use super::effects::EffectDomain;

/// How much of a track one instance of its effects covers (section 5 of the design). A track has one for its picture effects and one for its sound effects ([`EffectSharing`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackSharing {
    /// One instance for each clip, which ends with the clip and its tail: a clip never hears another.
    PerClip,
    /// One instance for each **run** of clips (clips one after the other that are in the same groups, with no gap longer than the tail between): a clip hears the clips of its run, and
    /// the edge of a group starts a new instance. The default.
    #[default]
    PerRun,
    /// One instance for the whole track, and each group that has some of its clips takes a **window** of what it makes.
    PerTrackWithWindows,
    /// One instance for the whole track that is never divided: no group that has effects may have only some of its clips.
    PerTrackIndivisible,
}

impl TrackSharing {
    pub const ALL: [Self; 4] = [
        Self::PerClip,
        Self::PerRun,
        Self::PerTrackWithWindows,
        Self::PerTrackIndivisible,
    ];

    /// What the menu of a track calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::PerClip => "Separate for each clip",
            Self::PerRun => "One copy for each run of clips",
            Self::PerTrackWithWindows => "One for the track, groups take a window",
            Self::PerTrackIndivisible => "One for the track, never divided",
        }
    }

    /// Whether the track is one block that no group can take part of.
    pub fn is_indivisible(self) -> bool {
        self == Self::PerTrackIndivisible
    }
}

/// The two sharings of a track: of its picture effects and of its sound effects (D7 of the design).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EffectSharing {
    pub picture: TrackSharing,
    pub sound: TrackSharing,
}

impl EffectSharing {
    /// The sharing for the effects of `domain`.
    pub fn of(self, domain: EffectDomain) -> TrackSharing {
        match domain {
            EffectDomain::Picture => self.picture,
            EffectDomain::Sound => self.sound,
        }
    }

    /// The same, with `sharing` for the effects of `domain`.
    #[must_use]
    pub fn with(mut self, domain: EffectDomain, sharing: TrackSharing) -> Self {
        match domain {
            EffectDomain::Picture => self.picture = sharing,
            EffectDomain::Sound => self.sound = sharing,
        }
        self
    }

    /// Whether both are the default (what a project file leaves out).
    pub fn is_the_default(self) -> bool {
        self == Self::default()
    }
}

/// Where a tail that goes **around** a group is drawn in the picture (a sound has no depth, so it ignores this). A group is one flat layer, so such a tail cannot be between its layers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TailDepth {
    /// Just behind the group: a memory is behind what is live. The default.
    #[default]
    Behind,
    /// Just in front of the group.
    InFront,
}

impl TailDepth {
    pub const ALL: [Self; 2] = [Self::Behind, Self::InFront];

    pub fn label(self) -> &'static str {
        match self {
            Self::Behind => "Drawn behind the group",
            Self::InFront => "Drawn in front of the group",
        }
    }
}

/// What happens to the tail of a clip where it enters a group that the clip is not in (section 7.2 of the design).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipTailEdge {
    /// The tail goes past the group, untouched by its effects. The default.
    #[default]
    Around,
    /// The group adopts the tail for as long as it is inside, and its effects process it.
    Into,
    /// The tail ends at the edge (with the edge fade).
    Cut,
}

impl ClipTailEdge {
    pub const ALL: [Self; 3] = [Self::Around, Self::Into, Self::Cut];

    pub fn label(self) -> &'static str {
        match self {
            Self::Around => "Goes around the group",
            Self::Into => "Goes into the group's effects",
            Self::Cut => "Is cut at the group's edge",
        }
    }
}

/// What happens to the tails of the members of a group where they pass the edge of its rectangle (section 7.4 of the design).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTailEdge {
    /// The group goes on after its last member for the tail, and its effects process it. The default.
    #[default]
    KeepAlive,
    /// The tail leaves at the edge, unprocessed by the group, and is the clip's again.
    Release,
    /// The tail ends at the edge (with the edge fade).
    Cut,
}

impl GroupTailEdge {
    pub const ALL: [Self; 3] = [Self::KeepAlive, Self::Release, Self::Cut];

    pub fn label(self) -> &'static str {
        match self {
            Self::KeepAlive => "Keep going through the group's effects",
            Self::Release => "Leave the group at its edge",
            Self::Cut => "Cut at the group's edge",
        }
    }
}

/// What a clip's tail does at the edge of a group it is not in (and, once a group has let it go, at the next edge).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClipTailPolicy {
    pub edge: ClipTailEdge,
    pub depth: TailDepth,
}

impl ClipTailPolicy {
    pub fn is_the_default(self) -> bool {
        self == Self::default()
    }
}

/// What the tails of the members of a clip group or a track group do at its edge, and where a tail that the group keeps alive is drawn when it lands over clips that are not in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GroupTailPolicy {
    pub edge: GroupTailEdge,
    pub depth: TailDepth,
}

impl GroupTailPolicy {
    pub fn is_the_default(self) -> bool {
        self == Self::default()
    }
}

/// How long the tail of an effect is, when its person has said (section 6.1 of the design). What the plugin says and the defaults come after.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TailLength {
    /// What the plugin says, or the default for the kind of effect. The default.
    #[default]
    Automatic,
    /// No tail: the effect ends where its input does.
    None,
    /// This many thousandths of a second.
    Milliseconds(u32),
}

/// The longest tail a person can ask for: ten minutes.
pub const LONGEST_TAIL_IN_MILLISECONDS: u32 = 600_000;

impl TailLength {
    pub fn is_the_default(self) -> bool {
        self == Self::Automatic
    }

    /// The same, with a length that is past [`LONGEST_TAIL_IN_MILLISECONDS`] brought back to it.
    #[must_use]
    pub fn limited(self) -> Self {
        match self {
            Self::Milliseconds(milliseconds) => Self::Milliseconds(milliseconds.min(LONGEST_TAIL_IN_MILLISECONDS)),
            other => other,
        }
    }
}
