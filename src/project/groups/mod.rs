//! Groups: things that are put together so that they can be given effects, and can be seen to belong together.
//!
//! There are three kinds, and each is held to different rules (the words in brackets are what the rules of the maintainer call them):
//!
//! * a **tag** (a *scattered* group) holds clips from any tracks, whether or not clips that are not in it are between them, and
//!   groups of any kind. The effects of a tag are not applied to a picture made of its members: each member has them applied to
//!   it, at the place in its own chain of effects where the badge of the tag is (see [`crate::project::Badge`]). A clip or a group
//!   can be in any number of tags;
//! * a **clip group** (a *contiguous* group) holds clips, and clip groups, that fit inside one rectangle of the timeline: so many
//!   frames across and so many tracks down. The rectangle is always the smallest that holds all the members, so the only way to
//!   change it is to add or take away members, and **no clip that is not one of its members, directly or through a clip group
//!   that is, may be inside it**. The effects of a clip group are applied to the picture that its members make together;
//! * a **track group** holds tracks, and track groups, that are next to each other in the list of tracks. It is like a clip group
//!   that reaches from the start of the timeline to the end of it: every clip on the tracks of the group is inside it.
//!
//! A clip, a track or a group has at most one group of the second or third kind that it is directly in (its *structural parent*),
//! and any number of tags. What the group is is one thing, and where the things in it are is another: the rectangle of a group is
//! never kept, it is worked out from where its members are (see [`rectangles`]), so that it cannot be wrong. The rules that have to
//! stay true are in [`rules`], and every edit of the project that can break one is made so that it is undone if it does (see
//! [`upkeep`]).
//!
//! What is in a group is kept in the group, and where its effects are applied to a member is kept in the member: a clip's chain of
//! effects has the badge of each tag it is in, and so has the chain of a group that is in a tag. The two are kept so that they
//! always agree, and the rules say so.

mod colour;
mod edges;
mod editing;
mod effects_of_groups;
mod joining;
mod queries;
mod rectangles;
mod rules;
mod upkeep;

use std::fmt;

pub use colour::GroupColour;
pub use edges::ClipGroupEdge;
pub use effects_of_groups::{AppliedEffect, ChainRow, EffectOwner};
pub use joining::ClipGroupBeforeTheDrag;
pub use rectangles::Rectangle;
pub use rules::GroupProblem;

use super::effects::Effects;
use super::tracks::{ClipId, TrackId};

/// Tells the groups of one project apart. Like the names of clips and of tracks, it is made up when a project is made or loaded
/// and is never written to a project file (the file says what is in a group by where it is).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(u32);

impl GroupId {
    pub const fn new(number: u32) -> Self {
        Self(number)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

impl fmt::Display for GroupId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "group {}", self.0)
    }
}

/// The three kinds of group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GroupKind {
    /// Holds clips from anywhere, and groups of any kind; its effects are applied to each member.
    Tag,
    /// Holds clips and clip groups that fit in one rectangle of the timeline, and nothing else is in the rectangle.
    ClipGroup,
    /// Holds tracks and track groups that are next to each other, and so all that is on them, from the start of the timeline to the end.
    TrackGroup,
}

impl GroupKind {
    /// Every kind, in the order they are listed in.
    pub const ALL: [GroupKind; 3] = [Self::Tag, Self::ClipGroup, Self::TrackGroup];

    /// What the kind is called to the people who use the program, in lower case.
    pub fn name(self) -> &'static str {
        match self {
            Self::Tag => "tag",
            Self::ClipGroup => "clip group",
            Self::TrackGroup => "track group",
        }
    }

    /// What the kind is called at the head of a box: with a capital letter.
    pub fn name_with_a_capital(self) -> &'static str {
        match self {
            Self::Tag => "Tag",
            Self::ClipGroup => "Clip group",
            Self::TrackGroup => "Track group",
        }
    }

    /// Whether a group of this kind can have a clip as a member (directly).
    pub fn can_hold_a_clip(self) -> bool {
        matches!(self, Self::Tag | Self::ClipGroup)
    }

    /// Whether a group of this kind can have a track as a member (directly).
    pub fn can_hold_a_track(self) -> bool {
        matches!(self, Self::TrackGroup)
    }

    /// Whether a group of this kind can have a group of kind `member` as a member (directly): a tag can have any group, and a
    /// clip group or a track group only another of its own kind.
    pub fn can_hold_a_group_of_kind(self, member: GroupKind) -> bool {
        match self {
            Self::Tag => true,
            Self::ClipGroup | Self::TrackGroup => self == member,
        }
    }

    /// Whether what is in a group of this kind that is in a group of this kind is in it by being *inside* it (the rectangle of the
    /// group has it in it), so that the group has one parent of this kind at most. A tag is not like that: things are in a tag by
    /// being put there, and in as many as they like.
    pub fn is_a_shape(self) -> bool {
        !matches!(self, Self::Tag)
    }
}

impl fmt::Display for GroupKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Something that is in a group: a clip, a track or another group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Member {
    Clip(ClipId),
    Track(TrackId),
    Group(GroupId),
}

impl From<ClipId> for Member {
    fn from(clip: ClipId) -> Self {
        Self::Clip(clip)
    }
}

impl From<TrackId> for Member {
    fn from(track: TrackId) -> Self {
        Self::Track(track)
    }
}

impl From<GroupId> for Member {
    fn from(group: GroupId) -> Self {
        Self::Group(group)
    }
}

impl fmt::Display for Member {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clip(clip) => clip.fmt(formatter),
            Self::Track(track) => track.fmt(formatter),
            Self::Group(group) => group.fmt(formatter),
        }
    }
}

/// A group: what kind it is, what it is called and what colour it is drawn in, the effects it has (with the badges of the tags it
/// is in), and what is in it, in the order they were put in.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    id: GroupId,
    kind: GroupKind,
    name: String,
    colour: GroupColour,
    effects: Effects,
    members: Vec<Member>,
}

impl Group {
    /// A group with no members and no effects. (Use [`Self::with_members`] and [`Self::with_effects`] to make it from its parts.)
    pub fn new(id: GroupId, kind: GroupKind, name: impl Into<String>, colour: GroupColour) -> Self {
        Self {
            id,
            kind,
            name: name.into(),
            colour,
            effects: Effects::none(),
            members: Vec::new(),
        }
    }

    /// The same group with `members` for what is in it, in that order. (For making a group from its parts, as when a project file
    /// is loaded: whether they can be in it is checked when the group is given to a project.)
    #[must_use]
    pub fn with_members(mut self, members: impl IntoIterator<Item = Member>) -> Self {
        self.members = members.into_iter().collect();
        self
    }

    /// The same group with `effects` for its own effects, which are used as they are.
    #[must_use]
    pub fn with_effects(mut self, effects: Effects) -> Self {
        self.effects = effects;
        self
    }

    pub fn id(&self) -> GroupId {
        self.id
    }

    pub fn kind(&self) -> GroupKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn colour(&self) -> GroupColour {
        self.colour
    }

    /// The effects the group has, and the badges of the tags it is in, in the order they apply in.
    pub fn effects(&self) -> &Effects {
        &self.effects
    }

    /// What is in the group directly, in the order it was put in.
    pub fn members(&self) -> &[Member] {
        &self.members
    }

    pub fn has_as_a_member(&self, member: Member) -> bool {
        self.members.contains(&member)
    }

    pub(super) fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub(super) fn set_colour(&mut self, colour: GroupColour) {
        self.colour = colour;
    }

    pub(super) fn effects_mut(&mut self) -> &mut Effects {
        &mut self.effects
    }

    pub(super) fn members_mut(&mut self) -> &mut Vec<Member> {
        &mut self.members
    }
}

#[cfg(test)]
pub(crate) mod random_edits_tests;
#[cfg(test)]
pub(crate) mod test_projects;
#[cfg(test)]
mod tests;
