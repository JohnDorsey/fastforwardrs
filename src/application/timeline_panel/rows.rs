//! The rows of the timeline: for each track, its title box and its strip, one row under another.
//!
//! The rows are laid out one under another, as tall as their tracks are, which is what makes the rows area as
//! tall as all of them together (and scroll). They are *drawn* where they are laid out, except while tracks are
//! being moved about, when a row is shown somewhere else for as long as it is moving: see [`super::track_reordering`].
//! A track that is being dragged to another place in the list is drawn last, over the others, with a shadow
//! under it.
//!
//! The drawers that are open hang below their rows. Those that pushed the rows under them to open make their rows taller, by as
//! far as they hang, and so push them down; those that were opened with Shift held lie over the rows under them, which do not
//! move. So the rows are drawn from the last to the first, which puts what lies over the rows under a row over them (and the
//! tracks in front over those behind, which is what front means); the rows area has room under its last row for what lies below
//! it.

use egui::{Color32, CornerRadius, Rect, Sense, Shadow, Ui, pos2, vec2};

use super::drawers::Drawers;
use super::strip_view::StripView;
use super::track_reordering::{follow_the_handles, plan_rows};
use super::{TimelineContent, TimelineViewState, drawers, header, strip, track_resize};
use crate::application::commands::Command;
use crate::application::textures::Textures;
use crate::application::theme;
use crate::project::{Project, Track};

/// The shadow under the row of a track that is being dragged, which is lifted over the others.
const SHADOW_UNDER_THE_DRAGGED_ROW: Shadow = Shadow {
    offset: [0, 6],
    blur: 16,
    spread: 0,
    color: Color32::from_black_alpha(150),
};

/// How tall each row is, and how much room there is after the last of them: what [`show`] lays out. It is worked out from the
/// project and the drawers alone, so that how tall the rows are going to be is known before anything is drawn (see
/// [`super::prepare`]), and `show` lays them out by it, so that it is what they are.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RowsLayout {
    /// How tall each row is, in the order of the tracks: as tall as its track, and as the drawers that pushed to open on it need.
    /// (The drawers that lie over the rows under it are not in it.)
    pub(super) heights: Vec<f32>,
    /// Room after the last row for what lies over the rows under its row and hangs below it, so that it can be scrolled to: as much
    /// as the drawer that reaches furthest down goes beyond the bottom of the last row.
    pub(super) room_after_the_last_row: f32,
}

impl RowsLayout {
    pub(super) fn of(project: &Project, drawers: &Drawers) -> Self {
        let tracks = project.tracks();
        let heights: Vec<f32> = tracks
            .iter()
            .map(|track| track.height().points() + drawers.extra_height_of(track, project))
            .collect();
        let mut top = 0.0;
        let mut bottom_of_the_last = 0.0;
        let mut furthest_down = 0.0_f32;
        for (track, height) in tracks.iter().zip(&heights) {
            let bottom = top + height;
            furthest_down = furthest_down.max(bottom + drawers.reach_over_the_rows_under_the_row_of(track, project));
            bottom_of_the_last = bottom;
            top = bottom + theme::SPACE_BETWEEN_TRACKS;
        }
        Self {
            heights,
            room_after_the_last_row: (furthest_down - bottom_of_the_last).max(0.0),
        }
    }

    /// How tall the rows are together, with the room between them and after the last of them. (Not counting what comes after
    /// that: the room for the boundary under the last track to be taken hold of, which is the same whatever the drawers do.)
    pub(super) fn total_height(&self) -> f32 {
        let room_between_rows = self.heights.len().saturating_sub(1) as f32 * theme::SPACE_BETWEEN_TRACKS;
        self.heights.iter().sum::<f32>() + room_between_rows + self.room_after_the_last_row
    }
}

/// Draws the rows in `ui`, which is the inside of the area they scroll in: the rows are as wide as `ui` is, the
/// title boxes at the left of them and then the strips, which are `strips_width` wide and start `strips_offset`
/// from the left edge of the row and look at the timeline the way `ruler_view` does. `rows_area_height` is how tall
/// the rows were together in the last frame. Gives the tracks with the view of the strip of each, in the order of
/// the tracks.
pub(super) fn show<'a>(
    ui: &mut Ui,
    content: &TimelineContent<'a>,
    (ruler_view, strips_offset, strips_width): (StripView, f32, f32),
    rows_area_height: f32,
    state: &mut TimelineViewState,
    textures: &mut Textures,
    commands: &mut Vec<Command>,
) -> Vec<(&'a Track, StripView)> {
    let tracks = content.project.tracks();
    let can_delete = tracks.len() > 1;
    // A row is as tall as its track, and as the drawers that pushed to open on it need under that.
    let layout = RowsLayout::of(content.project, &state.drawers);
    let heights = &layout.heights;
    let laid_out: Vec<Rect> = heights
        .iter()
        .map(|height| {
            ui.allocate_exact_size(vec2(ui.available_width(), *height), Sense::hover())
                .0
        })
        .collect();
    if !laid_out.is_empty() {
        ui.add_space(layout.room_after_the_last_row);
    }

    let rows_top = laid_out.first().map_or_else(|| ui.cursor().top(), |row| row.top());
    let planned = plan_rows(
        &tracks
            .iter()
            .zip(heights)
            .map(|(track, height)| (track.id(), *height))
            .collect::<Vec<_>>(),
        theme::SPACE_BETWEEN_TRACKS,
        state.track_reorder.as_mut(),
        &mut state.row_motion,
        ui.input(|input| input.pointer.interact_pos())
            .map(|pointer| pointer.y - rows_top),
        ui.input(|input| input.stable_dt),
    );
    if planned.is_moving {
        ui.ctx().request_repaint();
    }
    let dragged = state
        .track_reorder
        .and_then(|reorder| tracks.iter().position(|track| track.id() == reorder.track));
    // (From the last row to the first, so that what hangs from a row is over the rows under it.)
    let drawing_order = (0..tracks.len())
        .rev()
        .filter(|position| Some(*position) != dragged)
        .chain(dragged);

    let mut views: Vec<Option<StripView>> = vec![None; tracks.len()];
    for position in drawing_order {
        let track = &tracks[position];
        // The box and the strip are as tall as the track is: what is under them is the room for the drawers that pushed to open.
        let height = track.height().points();
        let top = rows_top + planned.tops[position];
        let row = Rect::from_min_size(
            pos2(laid_out[position].left(), top),
            vec2(laid_out[position].width(), heights[position]),
        );
        let header_rectangle = Rect::from_min_size(row.min, vec2(theme::TRACK_HEADER_WIDTH, height));
        let strip_rectangle =
            Rect::from_min_size(pos2(row.left() + strips_offset, row.top()), vec2(strips_width, height));
        if Some(position) == dragged {
            let whole_row = Rect::from_min_max(row.min, pos2(strip_rectangle.right(), row.bottom()));
            ui.painter()
                .add(SHADOW_UNDER_THE_DRAGGED_ROW.as_shape(whole_row, CornerRadius::same(theme::CLIP_CORNER_RADIUS)));
        }
        let handles = header::show(
            ui,
            header_rectangle,
            track,
            (
                content.can_edit,
                can_delete,
                content.selection.contains_track(track.id()),
            ),
            commands,
        );
        follow_the_handles(
            ui,
            &handles,
            (track.id(), position, top),
            &mut state.track_reorder,
            commands,
        );
        let strip_view = ruler_view.shown_in(strip_rectangle);
        strip::show(ui, &strip_view, track, content, state, textures, commands);
        views[position] = Some(strip_view);
        track_resize::show(
            ui,
            header_rectangle,
            track,
            content.can_edit,
            rows_area_height,
            state,
            commands,
        );
        // (The button of the drawer of the track is over the boundary under the box, which it must be on top of.)
        drawers::operate_the_button_of_a_track(
            ui,
            header_rectangle,
            track,
            handles.box_round_the_title.hovered() || handles.title.hovered(),
            &mut state.drawers,
        );
    }
    tracks
        .iter()
        .zip(views)
        .filter_map(|(track, view)| Some((track, view?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::timeline_panel::drawers::{OpenMode, height_when_open};
    use crate::project::{AnimationTarget, PropertyId, TextObject, TimelineFrameIndex, TrackHeight};

    /// A project with two tracks, each with a text object: the one in front (the first track) and the one behind (the last).
    struct TwoTracks {
        project: Project,
        in_front: AnimationTarget,
        behind: AnimationTarget,
    }

    fn two_tracks() -> TwoTracks {
        let mut project = Project::empty();
        let track_behind = project.tracks()[0].id();
        let track_in_front = project.add_track_in_front();
        let (in_front, _) = project
            .insert_text(track_in_front, TimelineFrameIndex::new(0), TextObject::default(), 30)
            .unwrap();
        let (behind, _) = project
            .insert_text(track_behind, TimelineFrameIndex::new(0), TextObject::default(), 30)
            .unwrap();
        TwoTracks {
            project,
            in_front: AnimationTarget::Clip(in_front),
            behind: AnimationTarget::Clip(behind),
        }
    }

    /// Opens the drawer of `owner` the way `mode` says, and lets it finish sliding.
    fn open(drawers: &mut Drawers, owner: AnimationTarget, mode: OpenMode) {
        drawers.toggle(owner, mode);
        drawers.advance(1.0);
    }

    fn track_height() -> f32 {
        crate::project::TrackHeight::DEFAULT_IN_POINTS
    }

    #[test]
    fn rows_with_every_drawer_shut_are_as_tall_as_their_tracks_with_room_between_them_and_nothing_after_the_last() {
        let tracks = two_tracks();

        let layout = RowsLayout::of(&tracks.project, &Drawers::default());

        assert_eq!(layout.heights, vec![track_height(), track_height()]);
        assert_eq!(layout.room_after_the_last_row, 0.0);
        assert_eq!(
            layout.total_height(),
            2.0 * track_height() + theme::SPACE_BETWEEN_TRACKS
        );
    }

    #[test]
    fn a_drawer_that_pushed_to_open_makes_its_row_taller_by_as_far_as_it_hangs_and_nothing_else() {
        let tracks = two_tracks();
        let hanging = height_when_open(tracks.in_front, &tracks.project);
        assert!(hanging > 10.0, "the setup: a drawer has some height");

        let mut drawers = Drawers::default();
        open(&mut drawers, tracks.in_front, OpenMode::PushingTheRowsUnder);
        let layout = RowsLayout::of(&tracks.project, &drawers);

        assert_eq!(layout.heights, vec![track_height() + hanging, track_height()]);
        assert_eq!(layout.room_after_the_last_row, 0.0);
        let shut = RowsLayout::of(&tracks.project, &Drawers::default());
        assert_eq!(layout.total_height(), shut.total_height() + hanging);
    }

    #[test]
    fn a_drawer_of_the_last_row_that_pushed_to_open_is_in_the_row_and_leaves_no_room_after_it() {
        let tracks = two_tracks();
        let hanging = height_when_open(tracks.behind, &tracks.project);

        let mut drawers = Drawers::default();
        open(&mut drawers, tracks.behind, OpenMode::PushingTheRowsUnder);
        let layout = RowsLayout::of(&tracks.project, &drawers);

        assert_eq!(layout.heights, vec![track_height(), track_height() + hanging]);
        assert_eq!(layout.room_after_the_last_row, 0.0);
    }

    #[test]
    fn a_drawer_of_the_last_row_that_lies_over_what_is_under_it_makes_room_after_the_row_and_no_row_taller() {
        let tracks = two_tracks();
        let hanging = height_when_open(tracks.behind, &tracks.project);

        let mut drawers = Drawers::default();
        open(&mut drawers, tracks.behind, OpenMode::LyingOverTheRowsUnder);
        let layout = RowsLayout::of(&tracks.project, &drawers);

        assert_eq!(layout.heights, vec![track_height(), track_height()]);
        assert_eq!(layout.room_after_the_last_row, hanging, "so that it can be scrolled to");
        let shut = RowsLayout::of(&tracks.project, &Drawers::default());
        assert_eq!(layout.total_height(), shut.total_height() + hanging);
    }

    #[test]
    fn a_drawer_that_lies_over_the_row_under_it_and_goes_beyond_it_makes_room_after_the_last_row_by_as_far_as_it_goes_beyond_it()
     {
        let mut tracks = two_tracks();
        // The row under the one of the drawer is as short as it can be, and the drawer has three racks in it.
        let behind_track = tracks.project.tracks()[1].id();
        tracks
            .project
            .set_track_height(behind_track, TrackHeight::from_points(TrackHeight::SMALLEST_IN_POINTS))
            .unwrap();
        for property in [PropertyId::Position, PropertyId::Size, PropertyId::Rotation] {
            tracks
                .project
                .add_keyframe(tracks.in_front, property, TimelineFrameIndex::new(5))
                .unwrap();
        }
        let hanging = height_when_open(tracks.in_front, &tracks.project);
        // It hangs from the bottom of its row, over the room between the rows and the row under it, and then beyond that.
        let beyond = hanging - theme::SPACE_BETWEEN_TRACKS - TrackHeight::SMALLEST_IN_POINTS;
        assert!(beyond > 0.0, "the setup: it goes beyond the row under it: {hanging}");

        let mut drawers = Drawers::default();
        open(&mut drawers, tracks.in_front, OpenMode::LyingOverTheRowsUnder);
        let layout = RowsLayout::of(&tracks.project, &drawers);

        assert_eq!(
            layout.heights,
            vec![track_height(), TrackHeight::SMALLEST_IN_POINTS],
            "no row is taller, because the drawer lies over the row under it"
        );
        assert_eq!(layout.room_after_the_last_row, beyond);
    }

    #[test]
    fn a_drawer_that_lies_over_the_row_under_it_and_does_not_go_beyond_that_row_changes_nothing() {
        let tracks = two_tracks();

        let mut drawers = Drawers::default();
        open(&mut drawers, tracks.in_front, OpenMode::LyingOverTheRowsUnder);
        let layout = RowsLayout::of(&tracks.project, &drawers);

        assert_eq!(layout, RowsLayout::of(&tracks.project, &Drawers::default()));
    }

    #[test]
    fn a_drawer_that_is_part_of_the_way_open_makes_its_row_that_much_taller_and_the_total_as_much() {
        let tracks = two_tracks();
        let mut drawers = Drawers::default();
        drawers.toggle(tracks.in_front, OpenMode::PushingTheRowsUnder);
        drawers.advance(0.05);
        let part_way = drawers.hanging_height_of(tracks.in_front, &tracks.project);
        assert!(
            part_way > 0.0 && part_way < height_when_open(tracks.in_front, &tracks.project),
            "the setup: it is part of the way"
        );

        let layout = RowsLayout::of(&tracks.project, &drawers);

        assert_eq!(layout.heights[0], track_height() + part_way);
    }

    #[test]
    fn a_layout_of_no_rows_has_no_height() {
        let layout = RowsLayout {
            heights: Vec::new(),
            room_after_the_last_row: 0.0,
        };

        assert_eq!(layout.total_height(), 0.0);
    }
}
