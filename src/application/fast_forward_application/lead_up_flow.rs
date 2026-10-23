// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Making the lead-up on a seek (EN-14, option 1; `docs/engine-phase-6-6-design.md`, section 4). The preview says when it wants one (see `EnginePreview::the_lead_up_that_is_wanted`: the checkbox
//! *Generate lead-up on seek* is on, and the picture on the screen is not exact and has stayed as it is); the window has the project and what it knows of its files, so **it gathers what the thread
//! that describes the frames before the one on the screen needs** (the project as the window shows it, planned; where the videos are and how their colours are read; the picture files, as the
//! window has decoded them) and starts it. What it cannot start it says so, and none is tried again for the frame that stays on the screen.

use std::collections::HashMap;

use super::FastForwardApplication;
use crate::application::preview_panel::LeadUpWanted;
use crate::frame_scene::TextPictures;
use crate::lead_up::{
    PreviewRunStreamStarter, StartedLeadUp, WhatALeadUpIsMadeFrom, frames_of_a_chase_from, frames_of_the_lead_up_to,
    start_a_lead_up,
};
use crate::playback::{VideoReading, preview_picture_size, reading_of_a_video};
use crate::project::TimelineFrameIndex;
use crate::video_export::{
    FontsOfTheComputer, HowTheFramesAreDescribed, RunStreamStarter, plan_export_of_the_files_that_can_be_read,
};

impl FastForwardApplication {
    /// If the preview wants a lead-up made, for the frame that the playhead is at, starts it (or says that it cannot be).
    pub(super) fn start_the_lead_up_that_the_preview_wants(&mut self, current_frame: TimelineFrameIndex) {
        let Some(wanted) = self
            .interface
            .engine_preview
            .as_ref()
            .and_then(|preview| preview.the_lead_up_that_is_wanted())
        else {
            return;
        };
        let starter = PreviewRunStreamStarter::new(self.environment.toolchain.clone());
        let made = self
            .what_the_lead_up_is_made_from(&wanted, current_frame.value(), starter)
            .map(start_a_lead_up);
        self.tell_the_preview_what_came_of_the_lead_up_it_wanted(&wanted, made);
    }

    /// Tells the preview what came of its wish for a lead-up: it is started, or it cannot be made, and then none is wanted again of the frame that stays on the screen (which would be a gathering of
    /// what a lead-up needs, and a failure of it, at every frame that is drawn).
    pub(super) fn tell_the_preview_what_came_of_the_lead_up_it_wanted(
        &mut self,
        wanted: &LeadUpWanted,
        made: Result<StartedLeadUp, String>,
    ) {
        let Some(preview) = self.interface.engine_preview.as_mut() else {
            return;
        };
        match made {
            Ok(StartedLeadUp { items, running }) => preview.start_the_lead_up(wanted, items, running),
            Err(why) => preview.the_lead_up_cannot_be_made(wanted, why),
        }
    }

    /// Everything that the thread that describes the frames of the lead-up that is wanted needs, for the frame `frame`, gathered from the project and from what the window knows of its files.
    /// The frames of a lead-up are those before `frame`; those of **a chase** (the project plays) go on from there to the end of the project, because the frame on the screen moves.
    /// `starter` is what starts the decode process of a run of video (the tests give one that is made up).
    pub(super) fn what_the_lead_up_is_made_from<Starter: RunStreamStarter>(
        &self,
        wanted: &LeadUpWanted,
        frame: u64,
        starter: Starter,
    ) -> Result<WhatALeadUpIsMadeFrom<Starter>, String> {
        let frames = if wanted.is_a_chase {
            frames_of_a_chase_from(
                frame,
                wanted.most_frames_a_state_has_gone_on_for,
                self.document.project.total_frame_count(),
            )
        } else {
            frames_of_the_lead_up_to(frame, wanted.most_frames_a_state_has_gone_on_for)
        };
        if frames.is_empty() {
            return Err("there are no frames before the frame that is shown".to_owned());
        }
        // (The project as the window shows it: a video whose file cannot be read has no pictures.)
        let plan =
            plan_export_of_the_files_that_can_be_read(&self.document.project, self.effective_settings(), |source| {
                self.sources.is_available(source)
            })
            .map_err(|error| error.to_string())?;
        let (picture_width, picture_height) = self.document.project.picture_size();

        // How the colours of each video are read: by the function that the preview asks, from what the window knows of the file.
        let readings: HashMap<_, VideoReading> = plan
            .videos
            .iter()
            .map(|(source, video)| {
                let format = self
                    .sources
                    .files
                    .get(source)
                    .and_then(|state| state.video_format.known.as_ref());
                (
                    *source,
                    reading_of_a_video(
                        format,
                        video.display_size_in_pixels,
                        &video.colour,
                        self.playback.planes_wanted,
                    ),
                )
            })
            .collect();
        let planes = readings
            .iter()
            .filter_map(|(source, reading)| Some((*source, reading.planes?)))
            .collect();
        let images = plan
            .images
            .keys()
            .filter_map(|source| Some((*source, self.sources.files.get(source)?.picture.picture.clone()?)))
            .collect();

        Ok(WhatALeadUpIsMadeFrom {
            frames,
            how: HowTheFramesAreDescribed {
                picture_size: (picture_width, picture_height),
                render_size: wanted.render_size,
                colour: wanted.colour,
            },
            preview_picture_size: preview_picture_size(picture_width, picture_height),
            locations: self.sources.available_locations(),
            planes,
            readings,
            images,
            starter,
            text: Box::new(|| Box::new(FontsOfTheComputer::new()) as Box<dyn TextPictures>),
            plan,
        })
    }
}
