//! Moving clips along their tracks by dragging them: how far they can go.
//!
//! Dragging a clip moves it as far as the pointer goes, until it comes to another clip that is not
//! moving along with it (no two clips can be at the same time on one track), or to the start of the
//! timeline. The clips in the way stop it, they are not pushed. Everything here is arithmetic on
//! frames, so that how far is allowed can be tested without a window.

use std::collections::HashSet;

use crate::project::{ClipLocation, Project, TimelineFrameIndex};

/// How many frames, earlier (negative) or later, the clips `moving` can be moved by, when `wanted`
/// is how many the pointer has gone: `wanted` itself, or as much of it as the clips in the way
/// allow. All the clips move by the same amount, so two of them that are moving never come to one
/// another: only the clips that are not moving stop them.
pub fn allowed_move_in_frames(project: &Project, moving: &[ClipLocation], wanted: i64) -> i64 {
    let moving_set: HashSet<ClipLocation> = moving.iter().copied().collect();
    // Never further than the start of the timeline, and never more than a clip that stays (the
    // nearest on either side of each clip that moves) leaves room for.
    let mut furthest_earlier = i64::MIN;
    let mut furthest_later = i64::MAX;
    for clip in moving {
        let Some(track) = project.track(clip.track) else {
            continue;
        };
        let Some(placement) = track.placement_of(clip.clip) else {
            continue;
        };
        let first = to_signed(placement.first_timeline_frame.value());
        let end = to_signed(placement.end_timeline_frame.value());
        furthest_earlier = furthest_earlier.max(-first);
        for other in track.placements() {
            let stays = !moving_set.contains(&ClipLocation {
                track: clip.track,
                clip: other.id,
            });
            if !stays || other.id == clip.clip {
                continue;
            }
            let other_first = to_signed(other.first_timeline_frame.value());
            let other_end = to_signed(other.end_timeline_frame.value());
            if other_first >= end {
                furthest_later = furthest_later.min(other_first - end);
            } else if other_end <= first {
                furthest_earlier = furthest_earlier.max(other_end - first);
            }
        }
    }
    // (The least is never more than nothing and the most never less, so the range is never empty.)
    wanted.clamp(furthest_earlier.min(0), furthest_later.max(0))
}

fn to_signed(frame: u64) -> i64 {
    i64::try_from(frame).unwrap_or(i64::MAX)
}

/// Where each of the clips `moving` starts once they are all moved by `frames` (earlier if
/// negative). A clip that is not on the timeline is left out.
pub fn starts_after_moving_by(
    project: &Project,
    moving: &[ClipLocation],
    frames: i64,
) -> Vec<(ClipLocation, TimelineFrameIndex)> {
    moving
        .iter()
        .filter_map(|clip| {
            let placement = project.placement_of(*clip)?;
            let start = to_signed(placement.first_timeline_frame.value())
                .saturating_add(frames)
                .max(0);
            Some((*clip, TimelineFrameIndex::new(start as u64)))
        })
        .collect()
}

#[cfg(test)]
mod tests;
