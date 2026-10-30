// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exporting a project from the command line, with no window: `fastforwardrs-export [--adapter best|software] <project file> <video file>`.
//!
//! This is the first thing that uses the picture engine without a window (EN-11): the same export as the editor's File menu (`video_export`), on the best graphics
//! adapter the computer has, with the progress told on the error stream, one line each time another per cent is done. It is a program of its own (and not an option
//! of the editor's) because the editor is a windowed program on Windows, which has no console to write to.
//!
//! The project file is a plain one (`.json`) or an archive (`.ffrs`, which can have its videos in it). The source files are looked for where the project says they
//! are and then next to the project file, as the editor does; the settings are the global ones of the editor (its settings file) with the project's own on top.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::archive_cache::ArchiveCache;
use crate::engine::AdapterChoice;
use crate::media::{FfmpegToolchain, MediaLocation};
use crate::plugin_host::{PluginHost, THE_PLUGINS_IN_A_PROCESS_OF_THEIR_OWN, folders_the_sound_of_a_project_needs};
use crate::plugin_scan::{
    LibraryOutcome, PluginSearch, plugins_found_in, scan_a_library_alone, scanner_program_beside_this_program,
};
use crate::plugin_trust::{ApprovalStore, folder_for_a_person, this_platform, what_the_command_line_refuses};
use crate::project::{InstalledPlugin, Project, Settings, SourceMedia, installed_plugins, set_installed_plugins};
use crate::project_file::archive::entry_place;
use crate::project_file::read_project_file;
use crate::settings_file::{default_settings_file_path, read_settings_file};
use crate::video_export::{
    ExportEvent, ExportOptions, ExportPlan, ExportResult, VideoExport, plan_export, words_about_held_pictures,
};

/// What the program says when it is not given what it needs.
pub const USAGE: &str = "\
Usage: fastforwardrs-export [--adapter best|software] [--settings <file>]
                            [--allow-plugins] [--plugin-approvals <file>]
                            <project file> <video file>

Exports the project in <project file> (a .json project, or a .ffrs project
file with its videos in it) as a finished video in <video file> (.mp4, .mkv
or .mov), with no window. The picture is made on the graphics card, and the
progress is told on the error stream.

  --adapter best        use the best graphics adapter the computer has
                        (the default)
  --adapter software    use a software graphics adapter (slow: for a
                        computer with no graphics card, and for testing)
  --settings <file>     read the settings that are the same for every
                        project from this file and not from the editor's own
                        (a file that is not there means the settings were
                        never changed); the folders that sound plugins are
                        looked for in are read from the file beside it
  --allow-plugins       let the project's plugins (programs that run with all
                        your rights) be run. A command line cannot ask, so
                        without this a project that needs a plugin that is
                        inside the project file, or one in a folder that has
                        not been approved, is refused, and they are listed.
                        With it, a plugin inside the project file is written
                        out, checked against its recorded hash, and run
  --plugin-approvals <file>
                        read the plugins that were approved from this file and
                        not from the editor's own
  --help                say this

The exit status is 0 when the video was made, 1 when the export failed, and
2 when the command line was not understood.";

/// Which graphics adapter the export is made on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    /// The best the computer has.
    Best,
    /// A software adapter.
    Software,
}

impl AdapterKind {
    fn as_a_choice(self) -> AdapterChoice {
        match self {
            Self::Best => AdapterChoice::BestAvailable,
            Self::Software => AdapterChoice::SoftwareOnly,
        }
    }
}

/// What a command line asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportRequest {
    pub project_file: PathBuf,
    pub video_file: PathBuf,
    pub adapter: AdapterKind,
    /// The file the global settings are read from (`None`: there are none, and the defaults are used).
    pub global_settings_file: Option<PathBuf>,
    /// The folder the files that are taken out of a project file are kept in (`None`: the usual place).
    pub archive_cache_folder: Option<PathBuf>,
    /// `--allow-plugins`: the plugins of the project may be run without asking (a command line cannot ask).
    pub allow_plugins: bool,
    /// The file the plugins that were approved are read from (`None`: none were).
    pub plugin_approvals_file: Option<PathBuf>,
    /// How plugins are looked for when the project has effects that are plugins (`None`: they are not looked for, and those effects pass the sound through).
    pub plugin_search: Option<Box<PluginSearch>>,
    /// The folder of the plugins that came with the program (`None`: there is none): they are run without the approval that other folders need.
    pub bundled_plugins: Vec<PathBuf>,
}

/// What the arguments of a command line come to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandLine {
    /// (Boxed: the request is large, and the other variant has nothing in it.)
    Export(Box<ExportRequest>),
    Help,
}

/// Reads the arguments that follow the name of the program. The settings and the cache are the usual ones.
pub fn parse_arguments(arguments: impl IntoIterator<Item = OsString>) -> Result<CommandLine, String> {
    let mut adapter = AdapterKind::Best;
    let mut settings_file = default_settings_file_path();
    let mut allow_plugins = false;
    let mut plugin_approvals_file = ApprovalStore::default_file();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--help" | "-h") => return Ok(CommandLine::Help),
            Some("--adapter") => {
                adapter = match arguments.next().as_deref().and_then(std::ffi::OsStr::to_str) {
                    Some("best") => AdapterKind::Best,
                    Some("software") => AdapterKind::Software,
                    Some(other) => return Err(format!("--adapter has to be best or software, and not {other}")),
                    None => return Err("--adapter has to be followed by best or software".to_owned()),
                };
            }
            Some("--settings") => {
                settings_file = Some(
                    arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or("--settings has to be followed by the name of a file")?,
                );
            }
            Some("--allow-plugins") => allow_plugins = true,
            Some("--plugin-approvals") => {
                plugin_approvals_file = Some(
                    arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or("--plugin-approvals has to be followed by the name of a file")?,
                );
            }
            // (Anything that begins with a dash is an option, and not a file name: a mistyped `-adapter` is told so, and not taken for the project file. A file whose name
            // begins with a dash can be given as `./-name`.)
            Some(option) if option.starts_with('-') => {
                return Err(format!("{option} is not an option of this program"));
            }
            _ => files.push(PathBuf::from(argument)),
        }
    }
    match <[PathBuf; 2]>::try_from(files) {
        Ok([project_file, video_file]) => Ok(CommandLine::Export(Box::new(ExportRequest {
            project_file,
            video_file,
            adapter,
            global_settings_file: settings_file.clone(),
            archive_cache_folder: None,
            allow_plugins,
            plugin_approvals_file,
            bundled_plugins: crate::plugin_scan::bundled_plugin_folders(),
            plugin_search: scanner_program_beside_this_program().map(|program| {
                Box::new(PluginSearch::beside_the_settings_file(
                    program,
                    settings_file.as_deref(),
                ))
            }),
        }))),
        Err(files) => Err(match files.len() {
            0 | 1 => "the project file and the video file are both needed".to_owned(),
            _ => "only the project file and the video file are wanted".to_owned(),
        }),
    }
}

/// What an export reports as it goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Report {
    /// Something that the finished video will not have, or that is not as it should be, and that does not stop the export.
    Warning(String),
    /// How many frames of the video have been written.
    Progress { frames_written: u64, total_frames: u64 },
}

/// The command line, from the arguments after the name of the program to the exit status, writing what is to be said to `error_stream`.
pub fn run(arguments: impl IntoIterator<Item = OsString>, error_stream: &mut dyn Write) -> u8 {
    let request = match parse_arguments(arguments) {
        Ok(CommandLine::Help) => {
            let _ = writeln!(error_stream, "{USAGE}");
            return 0;
        }
        Ok(CommandLine::Export(request)) => *request,
        Err(problem) => {
            let _ = writeln!(error_stream, "{problem}\n\n{USAGE}");
            return 2;
        }
    };
    let mut progress = ProgressTold::default();
    let result = export_project(&request, &mut |report| match report {
        Report::Warning(words) => {
            let _ = writeln!(error_stream, "warning: {words}");
        }
        Report::Progress {
            frames_written,
            total_frames,
        } => {
            if let Some(line) = progress.line_for(frames_written, total_frames) {
                let _ = writeln!(error_stream, "{line}");
            }
        }
    });
    match result {
        Ok(()) => {
            let _ = writeln!(error_stream, "The video was saved to {}", request.video_file.display());
            0
        }
        Err(reason) => {
            let _ = writeln!(error_stream, "The export failed: {reason}");
            1
        }
    }
}

/// Says how far the export is: a line for each per cent that is reached, and none for a per cent that was told already.
#[derive(Default)]
struct ProgressTold {
    last_per_cent: Option<u64>,
}

impl ProgressTold {
    fn line_for(&mut self, frames_written: u64, total_frames: u64) -> Option<String> {
        // (A video of no frames is done; and more frames than there are, which ffmpeg does not say, is not more than all.)
        let per_cent = (frames_written * 100).checked_div(total_frames).unwrap_or(100).min(100);
        if self.last_per_cent == Some(per_cent) {
            return None;
        }
        self.last_per_cent = Some(per_cent);
        Some(format!("{per_cent}% ({frames_written} of {total_frames} frames)"))
    }
}

/// Exports the project in `request.project_file` to `request.video_file`, reporting to `report`; the reason, if it failed.
pub fn export_project(request: &ExportRequest, report: &mut dyn FnMut(Report)) -> Result<(), String> {
    let project = read_project_file(&request.project_file).map_err(|error| error.to_string())?;
    // (Before ffmpeg is looked for: a project that is refused is refused wherever it is asked.)
    refuse_plugins_that_nobody_has_said_yes_to(request, &project)?;
    let plugins = find_and_check_the_plugins_of_the_sound(request, &project)?;
    for words in write_out_and_install_the_plugins_inside_the_project(request, &project, &plugins)? {
        report(Report::Warning(words));
    }
    let toolchain = FfmpegToolchain::locate().map_err(|error| error.to_string())?;
    let plan = plan_export(&project, settings_of_the_export(request, &project)?).map_err(|error| error.to_string())?;
    for words in warnings_about(&project) {
        report(Report::Warning(words));
    }
    let cache = match &request.archive_cache_folder {
        Some(folder) => Some(ArchiveCache::at(folder.clone())),
        None => ArchiveCache::in_the_usual_place(),
    };
    let locations = locate_the_sources(&project, &plan, &request.project_file, cache.as_ref())?;
    let export = VideoExport::start(
        &toolchain,
        &plan,
        &locations,
        &request.video_file,
        export_options_of(request).with_plugins(plugins),
        Arc::new(|| {}),
    )
    .map_err(|error| error.to_string())?;
    loop {
        match export.try_next_event() {
            Some(ExportEvent::Progress { frames_written }) => report(Report::Progress {
                frames_written,
                total_frames: plan.total_frame_count,
            }),
            Some(ExportEvent::Finished(ExportResult::Completed)) => {
                // (The video is made, all of its frames: what is told is that some pictures in it are not the video's own.)
                if let Some(words) = words_about_held_pictures(export.pictures_held()) {
                    report(Report::Warning(words));
                }
                for note in export.sound_notes() {
                    report(Report::Warning(note));
                }
                return Ok(());
            }
            Some(ExportEvent::Finished(ExportResult::Cancelled)) => return Err("the export was cancelled".to_owned()),
            Some(ExportEvent::Finished(ExportResult::Failed { reason })) => return Err(reason),
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// A command line cannot ask a person whether a plugin may be run, and a plugin is a program that runs with all the rights of whoever runs it: so the plugins that the
/// editor would have asked about (the ones inside the project file, and the files that have not been approved) **refuse the export, and are listed**, unless the command
/// line says `--allow-plugins`.
fn refuse_plugins_that_nobody_has_said_yes_to(request: &ExportRequest, project: &Project) -> Result<(), String> {
    if request.allow_plugins {
        return Ok(());
    }
    let approvals = request
        .plugin_approvals_file
        .clone()
        .map_or_else(ApprovalStore::in_memory, ApprovalStore::at);
    let project_directory = request
        .project_file
        .parent()
        .and_then(|folder| std::path::absolute(folder).ok());
    let refused = what_the_command_line_refuses(
        project.plugin_libraries(),
        project_directory.as_deref(),
        &this_platform(),
        &approvals,
    );
    if refused.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the project needs plugins, which are programs that run with all your rights, and a command line cannot ask whether they may be run, so nothing was exported:\n{}\n\
         To export it anyway, say --allow-plugins; to approve the files, open the project in the editor.",
        refused
            .iter()
            .map(|line| format!("  - {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

/// Finds the plugins that the effects of the sound need, if there are effects that are not installed (the editor scans when it starts; a program with no window scans when it
/// is asked to make a sound that needs them), and makes the host that runs them: **a folder that nobody has approved refuses the export and is listed**, as the plugins inside
/// a project file are, unless the command line says `--allow-plugins`.
fn find_and_check_the_plugins_of_the_sound(
    request: &ExportRequest,
    project: &Project,
) -> Result<Arc<PluginHost>, String> {
    let host = Arc::new(PluginHost::new().running_in_a_process_of_its_own(THE_PLUGINS_IN_A_PROCESS_OF_THEIR_OWN));
    // (The plugins that came with the program are the program's own: no question, so no refusal either.)
    for folder in &request.bundled_plugins {
        host.trust_bundled_folder(folder);
    }
    let has_effects_to_find = !super::fast_forward_application::effects_the_export_would_leave_out(project).is_empty();
    if has_effects_to_find && let Some(search) = &request.plugin_search {
        let found = search.scan_and_install();
        host.look_for_dependencies_in(
            found.settings.dependency_folders.clone(),
            found.settings.search_parent_folders,
        );
    }
    if request.allow_plugins {
        host.allow_everything();
        return Ok(host);
    }
    let approvals = request
        .plugin_approvals_file
        .clone()
        .map_or_else(ApprovalStore::in_memory, ApprovalStore::at);
    let (approved, refused): (Vec<_>, Vec<_>) =
        folders_the_sound_of_a_project_needs(project)
            .into_iter()
            .partition(|wanted| {
                approvals.is_folder_approved(&wanted.folder)
                    || host.standing_of(&wanted.folder) == crate::plugin_trust::FolderStanding::Approved
            });
    host.set_approved_folders(approved.into_iter().map(|wanted| wanted.folder));
    if refused.is_empty() {
        return Ok(host);
    }
    Err(format!(
        "the sound of the project needs plugins, which are programs that run with all your rights, and a command line cannot ask whether they may be run, so nothing was exported:
{}
         To export it anyway, say --allow-plugins; to approve the folders, open the project in the editor and say yes to them.",
        refused
            .iter()
            .map(|wanted| format!(
                "  - {} (in the folder {}, which has not been approved)",
                wanted.plugins.join(", "),
                folder_for_a_person(&wanted.folder)
            ))
            .collect::<Vec<_>>()
            .join("
")
    ))
}

/// **With `--allow-plugins`** (the person said yes to every plugin, the one thing a command line can do): the plugin libraries that are inside the project file, and have a build for
/// this computer, are written out (each checked against the hash the project recorded for it: a library that is not what the project says it is **refuses the export**), scanned
/// by the scan program, and their plugins are installed after those of the folders, so that the effects of them are run as the editor runs them after a person has held *Load it*.
/// What could not be scanned is given back as words, for the export to say (what uses it passes the sound through).
fn write_out_and_install_the_plugins_inside_the_project(
    request: &ExportRequest,
    project: &Project,
    host: &PluginHost,
) -> Result<Vec<String>, String> {
    if !request.allow_plugins {
        return Ok(Vec::new());
    }
    let platform = this_platform();
    let wanted: Vec<_> = project
        .plugin_libraries()
        .iter()
        .filter(|library| library.is_embedded() && library.build_for(&platform).is_some())
        .collect();
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let Some(search) = &request.plugin_search else {
        return Ok(vec![
            "the plugins inside the project were not looked at, because there is no program to scan them with"
                .to_owned(),
        ]);
    };
    let mut words = Vec::new();
    let mut reports = Vec::new();
    for library in wanted {
        let file = host
            .write_out_and_trust_an_embedded_plugin(library, &platform)
            .map_err(|why| {
                format!(
                    "the plugin {} inside the project cannot be used, so nothing was exported: {why}",
                    library.name()
                )
            })?;
        match scan_a_library_alone(search.scanner_program.clone(), &file, Vec::new(), false) {
            Ok(report) => {
                if let LibraryOutcome::Failed(failure) = &report.outcome {
                    words.push(format!(
                        "the plugin {}, inside the project, could not be used ({failure}), so the sound goes through it unchanged",
                        library.name()
                    ));
                }
                reports.push(report);
            }
            Err(problem) => words.push(format!(
                "the plugin {}, inside the project, could not be looked at ({problem}), so the sound goes through it unchanged",
                library.name()
            )),
        }
    }
    let mut installed: Vec<InstalledPlugin> = installed_plugins().into_iter().cloned().collect();
    for plugin in plugins_found_in(&reports).effects {
        if !installed
            .iter()
            .any(|already| already.descriptor.key == plugin.descriptor.key)
        {
            installed.push(plugin);
        }
    }
    set_installed_plugins(installed);
    Ok(words)
}

/// The settings that the export of `project` is made with: the global ones, from the file that the request names (the defaults where it names none, and where the file is
/// not there, because then the settings were never changed), with the project's own on top.
fn settings_of_the_export(request: &ExportRequest, project: &Project) -> Result<Settings, String> {
    let global_settings = match &request.global_settings_file {
        Some(path) => read_settings_file(path).map_err(|error| error.to_string())?,
        None => Settings::DEFAULT,
    };
    Ok(project.settings().resolve(global_settings))
}

/// What the export is started with other than the project: the graphics adapter that the request asks for.
fn export_options_of(request: &ExportRequest) -> ExportOptions {
    ExportOptions::new(request.adapter.as_a_choice())
}

/// What the export says before it begins: that the video will not have what an effect does that the program does not have (a project file brought it), for each such
/// effect, once.
fn warnings_about(project: &Project) -> Vec<String> {
    super::fast_forward_application::effects_the_export_would_leave_out(project)
        .into_iter()
        .map(|effect| format!("the effect {effect} is not in this program, so the video does not have what it does"))
        .collect()
}

/// Where the files the export is made from are, in the order the plan lists them: where the project says each is, then next to the project file; a file that is
/// included in a project file, where it is in it (or, if it is compressed there, taken out into the cache). The reason, if any cannot be found.
fn locate_the_sources(
    project: &Project,
    plan: &ExportPlan,
    project_file: &Path,
    cache: Option<&ArchiveCache>,
) -> Result<Vec<MediaLocation>, String> {
    let project_directory = project_file.parent();
    let mut locations = Vec::with_capacity(plan.source_files.len());
    let mut problems = Vec::new();
    for &identifier in &plan.source_files {
        let source: &SourceMedia = project
            .source(identifier)
            .expect("the plan is made of the sources of the project");
        let name = source.link.absolute_path.file_name().map_or_else(
            || source.link.absolute_path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let found = match &source.link.included_as {
            Some(entry_name) => location_of_an_included_file(project_file, entry_name, cache),
            None => source
                .link
                .candidate_paths(project_directory)
                .into_iter()
                .find(|path| path.is_file())
                .map(MediaLocation::of_a_file)
                .ok_or_else(|| "it is not where the project says it is, or next to the project file".to_owned()),
        };
        match found {
            Ok(location) => locations.push(location),
            Err(why) => problems.push(format!("{name}: {why}")),
        }
    }
    if problems.is_empty() {
        Ok(locations)
    } else {
        Err(format!(
            "these files of the project cannot be used:\n  {}",
            problems.join("\n  ")
        ))
    }
}

/// Where the bytes of the file `entry_name` of the project file `archive` are: in the project file where they are stored as they are, and otherwise in the
/// cache, once taken out.
fn location_of_an_included_file(
    archive: &Path,
    entry_name: &str,
    cache: Option<&ArchiveCache>,
) -> Result<MediaLocation, String> {
    let place = entry_place(archive, entry_name).map_err(|error| error.to_string())?;
    if let Some(location) = place.as_a_location_in(archive) {
        return Ok(location);
    }
    match cache {
        Some(cache) => cache
            .take_out(archive, entry_name)
            .map(MediaLocation::of_a_file)
            .map_err(|error| error.to_string()),
        None => Err(
            "it is compressed in the project file, and there is nowhere to keep the files that are taken out of it"
                .to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests;
