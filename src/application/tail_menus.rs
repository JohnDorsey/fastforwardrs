// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The menus of phase 12 (docs/engine-phase-12-design.md, sections 5, 6.1 and 7): how much of a track one instance of its effects covers (in the menu of the box of the track), what
//! the tails of clips do at the edge of a group they are not in (in the menu of a clip), what the tails of the members of a group do at its edge (in the menu of the bar of the group),
//! and how long the tail of an effect is (in the menu of the box of the effect).
//!
//! Nothing here is always visible: each is a smaller menu of a menu that was there. The one that is in force is marked, as the preset of an effect is. A choice that cannot be in force
//! where it is made (a track where groups take a window of its effects cannot let a tail go around a group) is stored all the same, and the menu says what is in force.

use egui::Ui;

use crate::application::commands::Command;
use crate::project::{
    ClipId, ClipTailEdge, ClipTailPolicy, EffectDomain, EffectInstanceId, Group, GroupKind, GroupTailEdge,
    GroupTailPolicy, Member, Project, TailDepth, TailLength, Track, TrackSharing,
};

/// What is put before the choice that is in force.
pub const IN_FORCE_MARK: &str = "\u{2713} ";

/// What the smaller menus are called.
pub const TAIL_TOWARD_OTHER_GROUPS_LABEL: &str = "Tail toward other groups";
pub const TAIL_OF_MEMBERS_LABEL: &str = "Tail of members";
pub const SHARING_OF_PICTURE_EFFECTS_LABEL: &str = "Sharing of picture effects";
pub const SHARING_OF_SOUND_EFFECTS_LABEL: &str = "Sharing of sound effects";
pub const TAIL_LENGTH_LABEL: &str = "Tail length";

/// What is said under the choices of a clip whose track gives groups a window of its effects.
pub const A_TRACK_WITH_WINDOWS_NOTE: &str =
    "On this track groups take a window of its effects, so a tail goes into the group's effects whatever is chosen.";

/// What the choices of the length of a tail say, other than a number of milliseconds.
pub const AUTOMATIC_TAIL_LABEL: &str = "Automatic (what the plugin says)";
pub const NO_TAIL_LABEL: &str = "No tail";

/// The lengths that the menu of an effect offers besides the two words, in thousandths of a second.
pub const TAIL_LENGTHS_OFFERED: [u32; 7] = [100, 500, 1_000, 2_000, 5_000, 10_000, 30_000];

/// What a length of tail is called in the menu: `500 ms`, `2 s`.
pub fn tail_length_label(length: TailLength) -> String {
    match length {
        TailLength::Automatic => AUTOMATIC_TAIL_LABEL.to_owned(),
        TailLength::None => NO_TAIL_LABEL.to_owned(),
        TailLength::Milliseconds(milliseconds) if milliseconds % 1_000 == 0 => format!("{} s", milliseconds / 1_000),
        TailLength::Milliseconds(milliseconds) => format!("{milliseconds} ms"),
    }
}

/// One choice of a menu, marked if it is the one in force. `true` when it was clicked, and the menu is closed.
fn choice(ui: &mut Ui, label: &str, is_in_force: bool) -> bool {
    let text = if is_in_force {
        format!("{IN_FORCE_MARK}{label}")
    } else {
        label.to_owned()
    };
    if ui.button(text).clicked() {
        ui.close();
        true
    } else {
        false
    }
}

/// The smaller menu of the menu of `clips` (at least one): what their tails do at the edge of a group that they are not in. What is in force is marked when all of the clips have it.
pub fn show_the_tail_menu_of_clips(ui: &mut Ui, clips: &[ClipId], project: &Project, commands: &mut Vec<Command>) {
    let policies: Vec<ClipTailPolicy> = clips
        .iter()
        .filter_map(|clip| project.clip_tail_policy(*clip))
        .collect();
    let shared = policies
        .first()
        .copied()
        .filter(|first| policies.iter().all(|policy| policy == first));
    let on_a_track_with_windows = clips.iter().any(|clip| {
        project.track_holding(*clip).is_some_and(|track| {
            [EffectDomain::Picture, EffectDomain::Sound]
                .into_iter()
                .any(|domain| track.sharing().of(domain) == TrackSharing::PerTrackWithWindows)
        })
    });
    ui.menu_button(TAIL_TOWARD_OTHER_GROUPS_LABEL, |ui| {
        let depth = shared.map(|policy| policy.depth).unwrap_or_default();
        for edge in ClipTailEdge::ALL {
            if choice(ui, edge.label(), shared.is_some_and(|policy| policy.edge == edge)) {
                commands.push(Command::SetClipTailPolicy {
                    clips: clips.to_vec(),
                    policy: ClipTailPolicy { edge, depth },
                });
            }
        }
        ui.separator();
        let edge = shared.map(|policy| policy.edge).unwrap_or_default();
        for depth in TailDepth::ALL {
            if choice(ui, depth.label(), shared.is_some_and(|policy| policy.depth == depth)) {
                commands.push(Command::SetClipTailPolicy {
                    clips: clips.to_vec(),
                    policy: ClipTailPolicy { edge, depth },
                });
            }
        }
        if on_a_track_with_windows {
            ui.separator();
            ui.weak(A_TRACK_WITH_WINDOWS_NOTE);
        }
    });
}

/// The smaller menu of the menu of the bar of `group`: what the tails of its members do at its edge. A tag has no edge, and no such menu.
pub fn show_the_tail_menu_of_a_group(ui: &mut Ui, group: &Group, commands: &mut Vec<Command>) {
    if group.kind() == GroupKind::Tag {
        return;
    }
    let policy = group.tail_policy();
    ui.menu_button(TAIL_OF_MEMBERS_LABEL, |ui| {
        for edge in GroupTailEdge::ALL {
            if choice(ui, edge.label(), policy.edge == edge) {
                commands.push(Command::SetGroupTailPolicy {
                    group: group.id(),
                    policy: GroupTailPolicy {
                        edge,
                        depth: policy.depth,
                    },
                });
            }
        }
        ui.separator();
        for depth in TailDepth::ALL {
            if choice(ui, depth.label(), policy.depth == depth) {
                commands.push(Command::SetGroupTailPolicy {
                    group: group.id(),
                    policy: GroupTailPolicy {
                        edge: policy.edge,
                        depth,
                    },
                });
            }
        }
    });
}

/// The two smaller menus of the menu of the box of `track`: how much of the track one instance of its picture effects covers, and the same for its sound effects.
pub fn show_the_sharing_menus_of_a_track(ui: &mut Ui, track: &Track, commands: &mut Vec<Command>) {
    for (label, domain) in [
        (SHARING_OF_PICTURE_EFFECTS_LABEL, EffectDomain::Picture),
        (SHARING_OF_SOUND_EFFECTS_LABEL, EffectDomain::Sound),
    ] {
        let now = track.sharing().of(domain);
        ui.menu_button(label, |ui| {
            for sharing in TrackSharing::ALL {
                if choice(ui, sharing.label(), now == sharing) {
                    commands.push(Command::SetTrackSharing {
                        track: track.id(),
                        domain,
                        sharing,
                    });
                }
            }
        });
    }
}

/// The smaller menu of the menu of the box of an effect: how long it goes on after its input ends.
pub fn show_the_tail_length_menu_of_an_effect(
    ui: &mut Ui,
    owner: Member,
    instance: EffectInstanceId,
    now: TailLength,
    commands: &mut Vec<Command>,
) {
    ui.menu_button(TAIL_LENGTH_LABEL, |ui| {
        let mut offered = vec![TailLength::Automatic, TailLength::None];
        offered.extend(TAIL_LENGTHS_OFFERED.into_iter().map(TailLength::Milliseconds));
        // (A length that the file has and the menu does not is shown too, so that what is in force is always there.)
        if !offered.contains(&now) {
            offered.push(now);
        }
        for length in offered {
            if choice(ui, &tail_length_label(length), now == length) {
                commands.push(Command::SetEffectTailLength {
                    owner,
                    instance,
                    length,
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_length_of_tail_is_called_in_seconds_when_it_is_whole_seconds_and_in_milliseconds_otherwise() {
        assert_eq!(tail_length_label(TailLength::Milliseconds(100)), "100 ms");
        assert_eq!(tail_length_label(TailLength::Milliseconds(1_000)), "1 s");
        assert_eq!(tail_length_label(TailLength::Milliseconds(2_500)), "2500 ms");
        assert_eq!(tail_length_label(TailLength::Milliseconds(30_000)), "30 s");
        assert_eq!(tail_length_label(TailLength::None), NO_TAIL_LABEL);
        assert_eq!(tail_length_label(TailLength::Automatic), AUTOMATIC_TAIL_LABEL);
    }
}
