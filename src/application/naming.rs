//! What things are called to the person editing: a clip by the kind of thing it is and what tells it from the others of its
//! kind (the name of its file, the beginning of its words), a property of it by its name, and a rack of keyframes by what
//! it animates.

use std::collections::HashMap;

use super::timeline_panel::SourceLook;
use crate::project::{AnimationTarget, Clip, ClipKind, EffectKind, Project, PropertyId, Rack, SourceMediaIdentifier};

/// How many letters of the words of a text object are used to tell it from the others.
const MOST_LETTERS_OF_WORDS: usize = 24;

/// What each of the project's source files looks like, which is where their names are.
pub type SourceLooks<'a> = HashMap<SourceMediaIdentifier, SourceLook<'a>>;

/// The kind of thing a clip is.
pub fn kind_name(kind: ClipKind) -> &'static str {
    match kind {
        ClipKind::Video => "Video clip",
        ClipKind::Audio => "Sound clip",
        ClipKind::Image => "Picture clip",
        ClipKind::Text => "Text object",
        ClipKind::Crate => "Keyframe crate",
    }
}

/// The first line of `words`, cut short if it is long.
pub fn beginning_of(words: &str) -> String {
    let first_line = words.lines().next().unwrap_or("").trim();
    if first_line.chars().count() > MOST_LETTERS_OF_WORDS {
        let cut: String = first_line.chars().take(MOST_LETTERS_OF_WORDS).collect();
        format!("{}...", cut.trim_end())
    } else {
        first_line.to_owned()
    }
}

/// What tells `clip` from the others of its kind: the name of its file, or the beginning of its words. (Nothing, for a clip
/// that has neither.)
pub fn detail_of_a_clip(clip: &Clip, source_looks: &SourceLooks<'_>) -> String {
    match clip {
        Clip::Text(text) => beginning_of(&text.object.text),
        other => other
            .source()
            .and_then(|source| source_looks.get(&source))
            .map(|look| look.file_name.clone())
            .unwrap_or_default(),
    }
}

/// `name` with a capital letter at the start.
fn with_a_capital_letter(name: &str) -> String {
    let mut letters = name.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// What a property is called: its name, with a capital letter.
pub fn what_a_property_is_called(property: PropertyId) -> String {
    with_a_capital_letter(property.name())
}

/// What a kind of effect is called: its name, with a capital letter.
pub fn what_an_effect_is_called(kind: EffectKind) -> String {
    with_a_capital_letter(kind.name())
}

/// What a clip or a track is called, in a few words: a clip by what tells it from the others of its kind, or if nothing does
/// by its kind, and a track by its title. `None` for what is not in the project.
pub fn what_a_target_is_called(
    project: &Project,
    target: AnimationTarget,
    source_looks: &SourceLooks<'_>,
) -> Option<String> {
    match target {
        AnimationTarget::Track(track) => Some(project.track(track)?.title().to_owned()),
        AnimationTarget::Clip(clip) => {
            let placement = project.placement_of(clip)?;
            let detail = detail_of_a_clip(placement.clip, source_looks);
            Some(if detail.is_empty() {
                kind_name(placement.clip.kind()).to_owned()
            } else {
                detail
            })
        }
    }
}

/// What a rack of keyframes is called: the property it animates, and if the rack is not in the drawer of what it animates
/// (`in_the_drawer_of` is whose drawer it is in), what that is.
pub fn what_a_rack_is_called(
    project: &Project,
    rack: &Rack,
    in_the_drawer_of: AnimationTarget,
    source_looks: &SourceLooks<'_>,
) -> String {
    let property = what_a_property_is_called(rack.property());
    if rack.target() == in_the_drawer_of {
        return property;
    }
    match what_a_target_is_called(project, rack.target(), source_looks) {
        Some(target) => format!("{property} of {target}"),
        None => property,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::project::{
        FrameRate, SourceMedia, SourceMediaLink, SourceVideoProperties, TextObject, TimelineFrameIndex, TrackId,
    };

    #[test]
    fn a_property_is_called_by_its_name_with_a_capital() {
        assert_eq!(what_a_property_is_called(PropertyId::Gain), "Gain");
        assert_eq!(what_a_property_is_called(PropertyId::FontSize), "Font size");
        assert_eq!(what_a_property_is_called(PropertyId::Rotation), "Rotation");
    }

    #[test]
    fn every_property_has_a_name_that_starts_with_a_capital_and_is_otherwise_its_own() {
        for property in PropertyId::ALL {
            let called = what_a_property_is_called(property);
            assert!(called.chars().next().unwrap().is_uppercase(), "{called}");
            assert_eq!(called.to_lowercase(), property.name());
        }
    }

    #[test]
    fn the_beginning_of_words_is_their_first_line_and_a_long_one_is_cut() {
        assert_eq!(beginning_of("Hello\nthere"), "Hello");
        assert_eq!(beginning_of("  padded  "), "padded");
        assert_eq!(beginning_of(""), "");
        let long = "abcdefghijklmnopqrstuvwxyz and then some more";
        assert_eq!(beginning_of(long), "abcdefghijklmnopqrstuvwx...");
        assert_eq!(
            beginning_of("abcdefghijklmnopqrstuvwx"),
            "abcdefghijklmnopqrstuvwx",
            "exactly as long as it may be"
        );
    }

    fn project_with_a_video_and_a_text() -> (Project, AnimationTarget, AnimationTarget, TrackId) {
        let mut project = Project::empty();
        let outcome = project
            .import_source(
                SourceMedia::video(
                    SourceMediaIdentifier::new(1),
                    SourceMediaLink::from_absolute_path(PathBuf::from("holiday.mp4")),
                    SourceVideoProperties {
                        frame_rate: FrameRate::new(30, 1).unwrap(),
                        total_frame_count: 100,
                        display_width_in_pixels: 320,
                        display_height_in_pixels: 180,
                        has_audio: true,
                    },
                ),
                TimelineFrameIndex::new(0),
            )
            .unwrap();
        let video = AnimationTarget::Clip(outcome.clip.unwrap());
        let in_front = project.add_track_in_front();
        let (text, _) = project
            .insert_text(
                in_front,
                TimelineFrameIndex::new(10),
                TextObject {
                    text: "Happy birthday\nto you".to_owned(),
                    ..TextObject::default()
                },
                30,
            )
            .unwrap();
        (project, video, AnimationTarget::Clip(text), in_front)
    }

    fn looks_of<'a>() -> SourceLooks<'a> {
        let mut looks = HashMap::new();
        looks.insert(
            SourceMediaIdentifier::new(1),
            SourceLook {
                is_available: true,
                thumbnails: None,
                picture_size_in_pixels: (320, 180),
                picture: None,
                waveform: None,
                file_name: "holiday.mp4".to_owned(),
            },
        );
        looks
    }

    #[test]
    fn a_clip_is_called_by_its_file_or_its_words_and_a_track_by_its_title() {
        let (project, video, text, track) = project_with_a_video_and_a_text();
        let looks = looks_of();
        assert_eq!(
            what_a_target_is_called(&project, video, &looks).as_deref(),
            Some("holiday.mp4")
        );
        assert_eq!(
            what_a_target_is_called(&project, text, &looks).as_deref(),
            Some("Happy birthday")
        );
        assert_eq!(
            what_a_target_is_called(&project, AnimationTarget::Track(track), &looks).as_deref(),
            Some(project.track(track).unwrap().title())
        );
    }

    #[test]
    fn a_clip_that_has_nothing_to_tell_it_by_is_called_by_its_kind_and_what_is_not_there_has_no_name() {
        let (project, video, _, _) = project_with_a_video_and_a_text();
        let no_looks = SourceLooks::new();
        assert_eq!(
            what_a_target_is_called(&project, video, &no_looks).as_deref(),
            Some("Video clip"),
            "the file's name is not known"
        );
        let nobody = AnimationTarget::Clip(crate::project::ClipId::new(TrackId::new(99), 0));
        assert_eq!(what_a_target_is_called(&project, nobody, &no_looks), None);
    }

    #[test]
    fn a_rack_in_the_drawer_of_what_it_animates_is_called_by_its_property_and_in_any_other_drawer_by_what_it_animates_too()
     {
        let (project, video, text, _) = project_with_a_video_and_a_text();
        let looks = looks_of();
        let rack = Rack::new(crate::project::RackId::new(TrackId::new(0), 0), video, PropertyId::Gain);

        assert_eq!(what_a_rack_is_called(&project, &rack, video, &looks), "Gain");
        assert_eq!(
            what_a_rack_is_called(&project, &rack, text, &looks),
            "Gain of holiday.mp4"
        );
        let track_rack = Rack::new(
            crate::project::RackId::new(TrackId::new(0), 1),
            AnimationTarget::Track(project.tracks()[0].id()),
            PropertyId::Muted,
        );
        assert_eq!(
            what_a_rack_is_called(&project, &track_rack, video, &looks),
            format!("Muted of {}", project.tracks()[0].title())
        );
    }
}
