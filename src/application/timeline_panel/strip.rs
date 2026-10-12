//! The strip of one track: its clips, and what the mouse does to them.
//!
//! * Clicking a clip selects it (with Ctrl held, it is added to what is selected or let go of), and
//!   clicking an empty part of the strip selects nothing.
//! * Dragging a clip moves it, and the other clips that are selected with it, along the track and to
//!   the other tracks: as far as the pointer goes, or as far as the clips next to it leave room (taking
//!   hold of it is here, the moving is in the `item_dragging` module).
//! * Right-clicking a clip selects it (unless it is one of the clips that are selected already), and the
//!   settings of what is selected are in the properties panel. It also opens a menu to put a copy of the clip (or of all the
//!   selected clips, if it is one of them) beside it, and to delete it (see [`super::clip_menu`]).
//! * Right-clicking an empty part of the strip opens a menu with "New", which has "Text object".
//! * Under the strip hang the drawers that are open: those of its clips, and the one of the track (see [`super::drawers`]).
//!
//! Cutting a track is done with a key (see the `keyboard` module), not with the mouse.

use egui::{PointerButton, Rect, Sense, Ui, WidgetInfo, WidgetType, pos2};

use super::clip_menu;
use super::clip_shape::ClipShape;
use super::clips::{self, rectangles_of};
use super::drawers::{self, drawer_button_label};
use super::item_dragging::keyframes_dragged_on_their_own;
use super::racks;
use super::strip_view::StripView;
use super::view_state::{Grabbed, ItemDrag};
use super::{NewMenuTarget, TimelineContent, TimelineViewState};
use crate::application::commands::Command;
use crate::application::selection::SelectionChange;
use crate::application::start_time_hold::StartTimeMemory;
use crate::application::textures::Textures;
use crate::application::theme;
use crate::project::{AnimationTarget, Clip, ClipId, Project, Track, TrackId};

/// The name the strip of a track is known by to screen readers (and the tests): it says which track.
pub fn strip_label(track_title: &str) -> String {
    format!("Strip of {track_title}")
}

/// The name a clip is known by: its number on its track, and which track.
pub fn clip_label(track_title: &str, clip: crate::project::ClipId) -> String {
    format!("Clip {} of {track_title}", clip.number())
}

/// The command that makes `clip` the only clip that is selected.
fn select_only(clip: ClipId) -> Command {
    Command::select_clips(vec![clip], SelectionChange::Replace)
}

/// Draws the strip of `track` where `view` is, and adds a command for each thing the user does to it.
pub(super) fn show(
    ui: &mut Ui,
    view: &StripView,
    track: &Track,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    textures: &mut Textures,
    commands: &mut Vec<Command>,
) {
    let strip_rectangle = view.rectangle;
    let strip_id = ui.id().with(("track strip", track.id()));
    // Not focusable, so that the keys of the keyboard keep their meanings as shortcuts of the window.
    let background = ui.interact(
        strip_rectangle,
        strip_id,
        if content.can_edit { Sense::CLICK } else { Sense::hover() },
    );
    background.widget_info(|| WidgetInfo::labeled(WidgetType::Other, content.can_edit, strip_label(track.title())));

    let painter = ui.painter_at(strip_rectangle);
    clips::draw_background(&painter, strip_rectangle);
    draw_the_drawers(ui, view, track, content.project, state);
    operate_the_drawer_of_the_track(ui, (view, content), track, (state, textures, commands));
    clips::draw_clips(ui, &painter, track, content, view, textures);
    clips::draw_clip_boundaries(&painter, view, track);

    for placement in track.placements() {
        let rectangles = rectangles_of(&placement, view);
        if rectangles.visible.width() <= 0.0 {
            continue;
        }
        let location = placement.id;
        let response = ui.interact(
            rectangles.visible,
            strip_id.with(("clip", placement.id)),
            if content.can_edit {
                Sense::CLICK | Sense::DRAG
            } else {
                Sense::hover()
            },
        );
        response.widget_info(|| {
            WidgetInfo::labeled(
                WidgetType::Other,
                content.can_edit,
                clip_label(track.title(), placement.id),
            )
        });
        let adding_to_the_selection = ui.input(|input| input.modifiers.command);
        if response.secondary_clicked() {
            // Right-clicking a clip that is one of several selected ones leaves them selected.
            if !content.selection.contains(location) {
                commands.push(select_only(location));
            }
        } else if response.clicked() {
            commands.push(if adding_to_the_selection {
                Command::select_clips(vec![location], SelectionChange::Toggle)
            } else {
                select_only(location)
            });
        }
        if content.can_edit {
            drag_the_clip(
                ui,
                view,
                content,
                state,
                (track.id(), location, &placement),
                &response,
                commands,
            );
            response.context_menu(|ui| {
                let clips = clip_menu::clips_the_menu_is_about(&content.selection.to_vec(), location);
                clip_menu::show(ui, &clips, content.project, commands);
            });
        }
        operate_the_drawer(
            (ui, &painter),
            (view, content),
            (track, &placement, &rectangles),
            response.hovered(),
            (state, textures, commands),
        );
        operate_the_crate(
            ui,
            (view, content),
            (&placement, &rectangles),
            (state, textures, commands),
        );
    }
    // The outlines go over everything, so that the clips beside a selected one cannot cover them.
    for selected in content.selection.iter() {
        if let Some(placement) = track.placement_of(selected) {
            let rectangles = rectangles_of(&placement, view);
            if rectangles.visible.width() > 0.0 {
                clips::draw_selection_outline(&painter, rectangles);
            }
        }
    }

    if content.can_edit {
        if background.clicked() {
            commands.push(Command::Deselect);
        }
        if background.secondary_clicked()
            && let Some(pointer) = background.interact_pointer_pos()
        {
            state.new_menu_target = view.frame_at(pointer.x).map(|frame| NewMenuTarget {
                track: track.id(),
                frame,
            });
        }
        background.context_menu(|ui| {
            ui.menu_button("New", |ui| {
                if ui.button("Text object").clicked() {
                    if let Some(target) = state.new_menu_target {
                        commands.push(Command::NewTextObject {
                            track: target.track,
                            frame: target.frame,
                        });
                    }
                    ui.close();
                }
            });
        });
    }
}

/// Draws the drawers that hang under the strip of `track` and are open (or sliding): the one of the track, and those of its
/// clips, which are behind the clips (they reach up behind them a little: see [`drawers::drawer_shape`]). They are drawn over
/// whatever is under the strip, which is the rows of the tracks behind this one.
fn draw_the_drawers(ui: &Ui, view: &StripView, track: &Track, project: &Project, state: &TimelineViewState) {
    let strip = view.rectangle;
    let room_for_drawers = Rect::from_min_max(
        strip.left_top(),
        pos2(
            strip.right(),
            strip.bottom() + state.drawers.reach_under_the_row_of(track, project),
        ),
    );
    let painter = ui.painter_at(room_for_drawers);
    let of_the_track = AnimationTarget::Track(track.id());
    if let Some(shape) = drawers::track_drawer_shape(
        strip,
        state.drawers.reach_of_the_drawers_of_the_clips_of(track, project),
        state.drawers.openness(of_the_track),
        drawers::height_when_open(of_the_track, project),
    ) {
        drawers::draw_drawer(&painter, shape, theme::DRAWER_BACKGROUND);
    }
    for placement in track.placements() {
        let rectangles = rectangles_of(&placement, view);
        if rectangles.visible.width() <= 0.0 {
            continue;
        }
        let radius = ClipShape::new(rectangles.whole, f32::from(theme::CLIP_CORNER_RADIUS)).radius();
        let owner = AnimationTarget::Clip(placement.id);
        let (shape, colour) = if matches!(placement.clip, Clip::Crate(_)) {
            // A crate that has more racks than fit in it reaches below the strip like a drawer that is open all the way, which is
            // of the colour of the crate.
            let reach = drawers::crate_layout(strip.height(), placement.drawer.len()).reach_below_the_strip;
            (
                drawers::drawer_shape(rectangles.whole, radius, 1.0, reach).filter(|_| reach > 0.0),
                theme::CRATE_CLIP_BACKGROUND,
            )
        } else if drawers::has_an_accessible_drawer(placement.clip) {
            (
                drawers::drawer_shape(
                    rectangles.whole,
                    radius,
                    state.drawers.openness(owner),
                    drawers::height_when_open(owner, project),
                ),
                theme::DRAWER_BACKGROUND,
            )
        } else {
            (None, theme::DRAWER_BACKGROUND)
        };
        if let Some(shape) = shape {
            drawers::draw_drawer(&painter, shape, colour);
        }
    }
}

/// The drawer of the track itself: its room, under the strip (and the drawers of its clips), takes what is done in it and so
/// is not an empty place of the strip, and what is in it (the racks of keyframes) is drawn there. (The button that opens
/// and shuts it is on the box of the track: see [`super::rows`].)
fn operate_the_drawer_of_the_track(
    ui: &Ui,
    (view, content): (&StripView, &TimelineContent<'_>),
    track: &Track,
    (state, textures, commands): (&mut TimelineViewState, &mut Textures, &mut Vec<Command>),
) {
    let owner = AnimationTarget::Track(track.id());
    let openness = state.drawers.openness(owner);
    if openness <= 0.0 {
        return;
    }
    let strip = view.rectangle;
    let top = strip.bottom()
        + state
            .drawers
            .reach_of_the_drawers_of_the_clips_of(track, content.project);
    let hanging = Rect::from_min_max(
        pos2(strip.left(), top),
        pos2(
            strip.right(),
            top + state.drawers.hanging_height_of(owner, content.project),
        ),
    );
    ui.interact(
        hanging,
        ui.id().with(("drawer of track", track.id(), "room")),
        Sense::CLICK | Sense::DRAG,
    );
    state.drawer_bottoms.push((owner, hanging.bottom()));
    let place = racks::DrawerPlace {
        owner,
        from_x: view.screen_x_of_frame(0),
        to_x: f32::INFINITY,
        first_frame: 0,
        top: top + theme::SPACE_ABOVE_THE_RACKS,
        shown_in: hanging,
        is_open_all_the_way: openness >= 1.0,
    };
    racks::show(ui, place, content, view, state, textures, commands);
}

/// What is in a keyframe crate (which is a clip, so that the crate itself is operated as one): its racks, which are drawn in
/// it, and its room, if it reaches below the strip, which takes what is done in it.
fn operate_the_crate(
    ui: &Ui,
    (view, content): (&StripView, &TimelineContent<'_>),
    (placement, rectangles): (&crate::project::ClipPlacement<'_>, &clips::ClipRectangles),
    (state, textures, commands): (&mut TimelineViewState, &mut Textures, &mut Vec<Command>),
) {
    if !matches!(placement.clip, Clip::Crate(_)) {
        return;
    }
    let strip = view.rectangle;
    let layout = drawers::crate_layout(strip.height(), placement.drawer.len());
    let shown_in = Rect::from_min_max(
        rectangles.visible.left_top(),
        pos2(
            rectangles.visible.right(),
            strip.bottom() + layout.reach_below_the_strip,
        ),
    );
    if layout.reach_below_the_strip > 0.0 {
        let below_the_strip =
            Rect::from_min_max(pos2(rectangles.visible.left(), strip.bottom()), shown_in.right_bottom());
        ui.interact(
            below_the_strip,
            ui.id().with(("crate", placement.id, "room")),
            Sense::CLICK | Sense::DRAG,
        );
    }
    racks::show(
        ui,
        racks::DrawerPlace {
            owner: AnimationTarget::Clip(placement.id),
            from_x: rectangles.whole.left(),
            to_x: rectangles.whole.right(),
            first_frame: placement.first_timeline_frame.value(),
            top: strip.top() + layout.first_rack_top,
            shown_in,
            is_open_all_the_way: true,
        },
        content,
        view,
        state,
        textures,
        commands,
    );
}

/// The button at the bottom edge of a clip that opens and shuts its drawer (which is there when the clip is hovered over,
/// when the drawer is open and when there is something in it), and the drawer's own room, which takes what is done in it and
/// so is not an empty place of the strip, and in which what is in the drawer (the racks of keyframes) is drawn.
/// `clip_is_hovered` says whether the clip is hovered over.
fn operate_the_drawer(
    (ui, painter): (&Ui, &egui::Painter),
    (view, content): (&StripView, &TimelineContent<'_>),
    (track, placement, rectangles): (&Track, &crate::project::ClipPlacement<'_>, &clips::ClipRectangles),
    clip_is_hovered: bool,
    (state, textures, commands): (&mut TimelineViewState, &mut Textures, &mut Vec<Command>),
) {
    if !drawers::has_an_accessible_drawer(placement.clip) {
        return;
    }
    let owner = AnimationTarget::Clip(placement.id);
    let drawer_id = ui.id().with(("drawer", placement.id));
    let openness = state.drawers.openness(owner);
    if openness > 0.0 {
        let strip = view.rectangle;
        let hanging = Rect::from_min_max(
            pos2(rectangles.visible.left(), strip.bottom()),
            pos2(
                rectangles.visible.right(),
                strip.bottom() + state.drawers.hanging_height_of(owner, content.project),
            ),
        );
        ui.interact(hanging, drawer_id.with("room"), Sense::CLICK | Sense::DRAG);
        state.drawer_bottoms.push((owner, hanging.bottom()));
        let place = racks::DrawerPlace {
            owner,
            from_x: rectangles.whole.left(),
            to_x: rectangles.whole.right(),
            first_frame: placement.first_timeline_frame.value(),
            top: strip.bottom() + theme::SPACE_ABOVE_THE_RACKS,
            shown_in: hanging,
            is_open_all_the_way: openness >= 1.0,
        };
        racks::show(ui, place, content, view, state, textures, commands);
    }
    let Some(button) = drawers::button_rectangle(rectangles.visible) else {
        return;
    };
    let response = ui.interact(button, drawer_id.with("button"), Sense::CLICK | Sense::DRAG);
    response.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::Button,
            true,
            drawer_button_label(track.title(), placement.id),
        )
    });
    if response.clicked() {
        state.drawers.toggle(
            owner,
            drawers::OpenMode::when_clicked(ui.input(|input| input.modifiers.shift)),
        );
    }
    if drawers::button_is_shown(
        clip_is_hovered || response.hovered(),
        state.drawers.is_to_be_seen(owner),
        !placement.drawer.is_empty(),
    ) {
        drawers::draw_button(painter, button, state.drawers.is_open(owner), response.hovered());
    }
}

/// Takes hold of what is to be dragged when `response` (the response of `clip`) begins to
/// be dragged: the selected clips and the selected keyframes if it is one of the clips, and otherwise it alone (which is
/// selected, with Ctrl added to what was selected). Where they are moved to after that is
/// [`super::item_dragging`]'s.
fn drag_the_clip(
    ui: &Ui,
    view: &StripView,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    (track, location, placement): (TrackId, ClipId, &crate::project::ClipPlacement<'_>),
    response: &egui::Response,
    commands: &mut Vec<Command>,
) {
    let frame_under = |pointer: egui::Pos2| view.frame_at(pointer.x).map_or(0, |frame| frame.value());
    let adding_to_the_selection = ui.input(|input| input.modifiers.command);
    // A drag starts once the pointer has gone a little way from where it was pressed, which is where
    // the clip was taken hold of.
    if response.drag_started_by(PointerButton::Primary)
        && let Some(pointer) = ui.input(|input| input.pointer.press_origin())
    {
        let (clips, keyframes) = if content.selection.contains(location) {
            (content.selection.to_vec(), content.selection.keyframes_to_vec())
        } else {
            commands.push(if adding_to_the_selection {
                Command::select_clips(vec![location], SelectionChange::Toggle)
            } else {
                select_only(location)
            });
            let (mut clips, keyframes) = if adding_to_the_selection {
                (content.selection.to_vec(), content.selection.keyframes_to_vec())
            } else {
                (Vec::new(), Vec::new())
            };
            clips.push(location);
            (clips, keyframes)
        };
        state.item_drag = Some(ItemDrag {
            keyframes: keyframes_dragged_on_their_own(content.project, &clips, &keyframes),
            clips,
            grabbed: Grabbed::Clip {
                clip: location,
                start_time: StartTimeMemory::new(placement.first_timeline_frame.value(), track),
            },
            pointer_frame_at_the_start: frame_under(pointer),
        });
    }
}
