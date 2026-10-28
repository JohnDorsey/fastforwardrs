// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings that decide how the program behaves.
//!
//! There are two layers. The **global settings** belong to the program and are the same for every
//! project. Each project can also set any one of them for itself, which is then used instead; a
//! setting a project does not set follows the global one. [`Settings`] holds a value for every
//! setting (the global ones, and also the ones in effect for a project once the two are combined),
//! and [`ProjectSettings`] holds what a project has set for itself.
//!
//! **Adding a setting:** give it a field in [`Settings`] (and its default), a field in
//! [`ProjectSettings`], a line in [`ProjectSettings::resolve`] and a row in [`SETTINGS`]; then add
//! it to the two files that store settings (`project_file` and `settings_file`). If its values are
//! not just on and off, make a type for them that implements [`SettingValue`]. The settings
//! windows are built from the table, and a test that goes through the table makes sure the files
//! keep every setting.
//!
//! **A global-only setting** describes the machine, or how the program is used, and not a project
//! (the settings of the preview's lead-up on a jump, EN-14): it has a field in [`Settings`] and a
//! row in [`GLOBAL_ONLY_SETTINGS`], no field in [`ProjectSettings`], and
//! [`ProjectSettings::resolve`] takes it from the global settings whatever the project says.

use serde::{Deserialize, Serialize};

use crate::colour_space::{StandardPrimaries, TransferFunction};

/// What the sound of a clip that plays faster than normal is like.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpedUpClipSound {
    /// No sound at all: silence for as long as the clip lasts.
    Mute,
    /// The sound is played faster and keeps its pitch, so that a voice still sounds like the same
    /// voice, just hurried.
    #[default]
    MaintainPitch,
    /// The sound is played faster like a tape that is wound on: the pitch rises with the speed.
    IncreasePitch,
}

impl SpedUpClipSound {
    pub const ALL: [Self; 3] = [Self::Mute, Self::MaintainPitch, Self::IncreasePitch];

    /// What the settings windows call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Mute => "Mute",
            Self::MaintainPitch => "Maintain pitch",
            Self::IncreasePitch => "Increase pitch",
        }
    }

    /// What the old yes-or-no setting "mute sped-up clips", which files from earlier versions
    /// still have, meant: yes was [`Self::Mute`], and no left the sound on, at its own pitch.
    pub fn from_the_old_mute_setting(muted: bool) -> Self {
        if muted { Self::Mute } else { Self::MaintainPitch }
    }
}

/// What the screen that shows the preview does with the numbers it is given (CM-04, CM-14): the curve between the codes of the picture and the light of the screen. **The
/// primaries of the screen are taken to be Rec.709/sRGB's** (a wide-gamut screen shows them as they are, as a program that assumes sRGB does).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTransform {
    /// The curve of sRGB: what computer screens do, and what pictures are made for.
    #[default]
    Srgb,
    /// A power of 2.2.
    Gamma22,
    /// A power of 2.4: the display of BT.1886, which a studio monitor shows video with.
    Gamma24,
}

impl DisplayTransform {
    pub const ALL: [Self; 3] = [Self::Srgb, Self::Gamma22, Self::Gamma24];

    /// What the settings windows call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Srgb => "sRGB",
            Self::Gamma22 => "Gamma 2.2",
            Self::Gamma24 => "Gamma 2.4 (BT.1886)",
        }
    }

    /// The curve that the numbers of the picture shown are made with.
    pub fn transfer_function(self) -> TransferFunction {
        match self {
            Self::Srgb => TransferFunction::Srgb,
            Self::Gamma22 => TransferFunction::Gamma22,
            Self::Gamma24 => TransferFunction::Gamma24,
        }
    }
}

/// What the pictures of a project are put together in (CM-02): light, in the primaries of one of the sets that are good to composite in. (Mixing is linear, so the choice does
/// not change a picture that has nothing but mixing in it; it is what effects that are not linear see.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkingColourSpace {
    /// Linear light in the primaries of Rec.709 and sRGB.
    #[default]
    LinearRec709,
    /// ACEScg: linear light in the AP1 primaries of ACES, which hold more colours than the screen shows.
    AcesCg,
}

impl WorkingColourSpace {
    pub const ALL: [Self; 2] = [Self::LinearRec709, Self::AcesCg];

    /// What the settings windows call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::LinearRec709 => "Rec.709 (linear)",
            Self::AcesCg => "ACEScg",
        }
    }

    /// The primaries of the light.
    pub fn primaries(self) -> StandardPrimaries {
        match self {
            Self::LinearRec709 => StandardPrimaries::Rec709,
            Self::AcesCg => StandardPrimaries::AcesCg,
        }
    }
}

/// What the exported video's numbers are (CM-05): the primaries and the curve that the finished picture is encoded with, which the file's tags then say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputColourSpace {
    /// Rec.709 primaries, **encoded as the display transform encodes** the preview, so that the file's numbers are the preview's; tagged `bt709`.
    #[default]
    Rec709AsPreviewed,
    /// Rec.709 primaries, encoded with the power 2.4 of BT.1886; tagged `bt709`.
    Rec709Bt1886,
    /// sRGB: Rec.709 primaries and the sRGB curve; the transfer tagged `iec61966-2-1`.
    Srgb,
}

impl OutputColourSpace {
    pub const ALL: [Self; 3] = [Self::Rec709AsPreviewed, Self::Rec709Bt1886, Self::Srgb];

    /// What the settings windows call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Rec709AsPreviewed => "Rec.709, as the preview shows it",
            Self::Rec709Bt1886 => "Rec.709, BT.1886 (gamma 2.4)",
            Self::Srgb => "sRGB",
        }
    }

    /// The primaries of the numbers of the file.
    pub fn primaries(self) -> StandardPrimaries {
        StandardPrimaries::Rec709
    }

    /// The curve the numbers of the file are made with, when the display transform is `display` (which only the first of the three follows).
    pub fn transfer_function(self, display: DisplayTransform) -> TransferFunction {
        match self {
            Self::Rec709AsPreviewed => display.transfer_function(),
            Self::Rec709Bt1886 => TransferFunction::Gamma24,
            Self::Srgb => TransferFunction::Srgb,
        }
    }

    /// What the file says its transfer characteristics are (ffmpeg's `color_trc` name). `bt709` for the two that are Rec.709 video, whose numbers a player shows as it shows
    /// any such video; `iec61966-2-1` for the one that says that it is sRGB.
    pub fn transfer_tag(self) -> &'static str {
        match self {
            Self::Rec709AsPreviewed | Self::Rec709Bt1886 => "bt709",
            Self::Srgb => "iec61966-2-1",
        }
    }
}

/// How a file that an effect uses (a `.cube` table of colours) is kept when it is added to a project (RS-02, RS-03): in the project file, or by a path. **It changes only the
/// files that are added after it is changed** (a file that is in the project is kept as it is).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceStorage {
    /// The file is put in the project file, so that the project can be moved with everything it uses. (A project that is a plain JSON file has no room for it: the file
    /// is linked, and the program says so.)
    #[default]
    Embed,
    /// The file stays where it is and the project remembers the path, as for a video.
    Link,
    /// The program asks each time.
    Ask,
}

/// What the file of a project is, as far as keeping a resource in it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectFileKind {
    /// The project has not been saved: it will be saved as whichever the person chooses.
    NotSavedYet,
    /// An archive (a `.ffrs` file), which has room for files.
    Archive,
    /// Plain JSON, which has none.
    PlainJson,
}

/// What is done with a file that is added to a project as a resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HowToKeepAResource {
    /// It is put in the project file.
    Embed,
    /// It stays where it is and is linked by its path.
    Link,
    /// It is linked, because the project file is plain JSON and cannot have it in it; the person is told so, and asked once whether to save the project as a `.ffrs` file.
    LinkBecauseTheFileIsPlainJson,
    /// The person is asked which of the others.
    Ask,
}

impl ResourceStorage {
    pub const ALL: [Self; 3] = [Self::Embed, Self::Link, Self::Ask];

    /// What to do with a file that is added as a resource, by this setting, to a project whose file is `file` (RS-02, Q-03). **A project that is plain JSON cannot embed**, so
    /// where the setting says to embed it links, and says so.
    pub fn how_to_keep(self, file: ProjectFileKind) -> HowToKeepAResource {
        match (self, file) {
            (Self::Ask, _) => HowToKeepAResource::Ask,
            (Self::Link, _) => HowToKeepAResource::Link,
            (Self::Embed, ProjectFileKind::PlainJson) => HowToKeepAResource::LinkBecauseTheFileIsPlainJson,
            (Self::Embed, ProjectFileKind::NotSavedYet | ProjectFileKind::Archive) => HowToKeepAResource::Embed,
        }
    }

    /// What the settings windows call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Embed => "Embed in the project file",
            Self::Link => "Link to the file",
            Self::Ask => "Ask each time",
        }
    }
}

/// How long the fade is where a sound meets the edge of a group, so that a sound that goes into a group's effects, or out of them, does not click (phase 12, D5 of
/// docs/engine-phase-12-design.md). It is the length of the crossfade at every such edge, in the preview and in the exported video.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeFade {
    /// No fade: a hard edge.
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "5_ms")]
    Milliseconds5,
    #[default]
    #[serde(rename = "10_ms")]
    Milliseconds10,
    #[serde(rename = "20_ms")]
    Milliseconds20,
    #[serde(rename = "50_ms")]
    Milliseconds50,
}

impl EdgeFade {
    pub const ALL: [Self; 5] = [
        Self::Off,
        Self::Milliseconds5,
        Self::Milliseconds10,
        Self::Milliseconds20,
        Self::Milliseconds50,
    ];

    /// What the settings windows call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "No fade",
            Self::Milliseconds5 => "5 ms",
            Self::Milliseconds10 => "10 ms",
            Self::Milliseconds20 => "20 ms",
            Self::Milliseconds50 => "50 ms",
        }
    }

    /// How many thousandths of a second the fade lasts.
    pub fn milliseconds(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Milliseconds5 => 5,
            Self::Milliseconds10 => 10,
            Self::Milliseconds20 => 20,
            Self::Milliseconds50 => 50,
        }
    }
}

/// How far apart, in frames, the preview keeps a copy of the state of an effect that depends on earlier frames (EN-14): a **checkpoint**. After a jump the preview goes on from
/// the nearest checkpoint before the place it jumped to, so that a shorter interval is less to make again and more memory kept. **A global-only setting.**
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CheckpointInterval {
    #[serde(rename = "every_30_frames")]
    Every30Frames,
    #[default]
    #[serde(rename = "every_60_frames")]
    Every60Frames,
    #[serde(rename = "every_120_frames")]
    Every120Frames,
    #[serde(rename = "every_300_frames")]
    Every300Frames,
}

impl CheckpointInterval {
    pub const ALL: [Self; 4] = [
        Self::Every30Frames,
        Self::Every60Frames,
        Self::Every120Frames,
        Self::Every300Frames,
    ];

    /// What the settings windows call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Every30Frames => "Every 30 frames",
            Self::Every60Frames => "Every 60 frames",
            Self::Every120Frames => "Every 120 frames",
            Self::Every300Frames => "Every 300 frames",
        }
    }

    /// The number of frames between one checkpoint and the next.
    pub fn frames(self) -> u32 {
        match self {
            Self::Every30Frames => 30,
            Self::Every60Frames => 60,
            Self::Every120Frames => 120,
            Self::Every300Frames => 300,
        }
    }
}

/// How much memory all the checkpoints of the preview may take together (EN-14): when they would take more, the oldest are dropped. **A global-only setting.**
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CheckpointMemoryBudget {
    #[serde(rename = "256_mib")]
    Mib256,
    #[serde(rename = "512_mib")]
    Mib512,
    #[default]
    #[serde(rename = "1_gib")]
    Gib1,
    #[serde(rename = "2_gib")]
    Gib2,
    #[serde(rename = "4_gib")]
    Gib4,
}

impl CheckpointMemoryBudget {
    pub const ALL: [Self; 5] = [Self::Mib256, Self::Mib512, Self::Gib1, Self::Gib2, Self::Gib4];

    /// What the settings windows call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Mib256 => "256 MiB",
            Self::Mib512 => "512 MiB",
            Self::Gib1 => "1 GiB",
            Self::Gib2 => "2 GiB",
            Self::Gib4 => "4 GiB",
        }
    }

    /// The number of bytes.
    pub fn bytes(self) -> u64 {
        const MIB: u64 = 1024 * 1024;
        match self {
            Self::Mib256 => 256 * MIB,
            Self::Mib512 => 512 * MIB,
            Self::Gib1 => 1024 * MIB,
            Self::Gib2 => 2 * 1024 * MIB,
            Self::Gib4 => 4 * 1024 * MIB,
        }
    }
}

/// A value for every setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Save the project to its file by itself shortly after every change. (Only for a project that
    /// has a file already: the first save always asks where.)
    pub autosave: bool,
    /// What the sound of clips that play faster than normal is like, in the preview and in the
    /// exported video.
    pub sped_up_clip_sound: SpedUpClipSound,
    /// What the screen that shows the preview does with the numbers it is given.
    pub display_transform: DisplayTransform,
    /// What the pictures of the project are put together in.
    pub working_colour_space: WorkingColourSpace,
    /// What the numbers of the exported video are.
    pub output_colour_space: OutputColourSpace,
    /// How the files that effects use are kept when they are added.
    pub resource_storage: ResourceStorage,
    /// How long the fade is where a sound meets the edge of a group (phase 12).
    pub edge_fade: EdgeFade,
    /// After a jump in the timeline, make the frames that lead up to the playhead in the background, so that an effect that depends on earlier frames shows what the
    /// export shows (EN-14, the View menu). Off: the preview goes on from the nearest checkpoint and says that the picture is approximate. **A global-only setting.**
    pub generate_lead_up_on_seek: bool,
    /// How far apart the checkpoints of the preview are. **A global-only setting.**
    pub checkpoint_interval: CheckpointInterval,
    /// How much memory the checkpoints may take together. **A global-only setting.**
    pub checkpoint_memory_budget: CheckpointMemoryBudget,
}

impl Settings {
    /// What the global settings are before anyone has changed them.
    pub const DEFAULT: Self = Self {
        autosave: false,
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        display_transform: DisplayTransform::Srgb,
        working_colour_space: WorkingColourSpace::LinearRec709,
        output_colour_space: OutputColourSpace::Rec709AsPreviewed,
        resource_storage: ResourceStorage::Embed,
        edge_fade: EdgeFade::Milliseconds10,
        generate_lead_up_on_seek: true,
        checkpoint_interval: CheckpointInterval::Every60Frames,
        checkpoint_memory_budget: CheckpointMemoryBudget::Gib1,
    };
}

impl Default for Settings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The settings a project has set for itself. `None` means the project follows the global setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectSettings {
    pub autosave: Option<bool>,
    pub sped_up_clip_sound: Option<SpedUpClipSound>,
    pub display_transform: Option<DisplayTransform>,
    pub working_colour_space: Option<WorkingColourSpace>,
    pub output_colour_space: Option<OutputColourSpace>,
    pub resource_storage: Option<ResourceStorage>,
    pub edge_fade: Option<EdgeFade>,
}

impl ProjectSettings {
    /// A project that follows every global setting, as a new project does.
    pub const FOLLOW_ALL_GLOBAL_SETTINGS: Self = Self {
        autosave: None,
        sped_up_clip_sound: None,
        display_transform: None,
        working_colour_space: None,
        output_colour_space: None,
        resource_storage: None,
        edge_fade: None,
    };

    /// The settings in effect for the project: its own where it has set them, the global ones
    /// everywhere else (and always for the global-only settings, which a project cannot set).
    pub fn resolve(&self, global: Settings) -> Settings {
        Settings {
            autosave: self.autosave.unwrap_or(global.autosave),
            sped_up_clip_sound: self.sped_up_clip_sound.unwrap_or(global.sped_up_clip_sound),
            display_transform: self.display_transform.unwrap_or(global.display_transform),
            working_colour_space: self.working_colour_space.unwrap_or(global.working_colour_space),
            output_colour_space: self.output_colour_space.unwrap_or(global.output_colour_space),
            resource_storage: self.resource_storage.unwrap_or(global.resource_storage),
            edge_fade: self.edge_fade.unwrap_or(global.edge_fade),
            generate_lead_up_on_seek: global.generate_lead_up_on_seek,
            checkpoint_interval: global.checkpoint_interval,
            checkpoint_memory_budget: global.checkpoint_memory_budget,
        }
    }
}

/// How a setting is shown in the settings windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingControlKind {
    /// A box that is ticked or not (for a setting with two values: off, then on).
    Checkbox,
    /// A drop-down list of the values.
    Dropdown,
}

/// A kind of value a setting can have.
pub trait SettingValue: Copy + PartialEq + 'static {
    /// Every value, in the order they are listed. The first is what an unticked checkbox means.
    const ALL: &'static [Self];
    const CONTROL: SettingControlKind;

    fn label(self) -> &'static str;
}

impl SettingValue for bool {
    const ALL: &'static [Self] = &[false, true];
    const CONTROL: SettingControlKind = SettingControlKind::Checkbox;

    fn label(self) -> &'static str {
        if self { "On" } else { "Off" }
    }
}

impl SettingValue for SpedUpClipSound {
    const ALL: &'static [Self] = &SpedUpClipSound::ALL;
    const CONTROL: SettingControlKind = SettingControlKind::Dropdown;

    fn label(self) -> &'static str {
        SpedUpClipSound::label(self)
    }
}

impl SettingValue for DisplayTransform {
    const ALL: &'static [Self] = &DisplayTransform::ALL;
    const CONTROL: SettingControlKind = SettingControlKind::Dropdown;

    fn label(self) -> &'static str {
        DisplayTransform::label(self)
    }
}

impl SettingValue for WorkingColourSpace {
    const ALL: &'static [Self] = &WorkingColourSpace::ALL;
    const CONTROL: SettingControlKind = SettingControlKind::Dropdown;

    fn label(self) -> &'static str {
        WorkingColourSpace::label(self)
    }
}

impl SettingValue for OutputColourSpace {
    const ALL: &'static [Self] = &OutputColourSpace::ALL;
    const CONTROL: SettingControlKind = SettingControlKind::Dropdown;

    fn label(self) -> &'static str {
        OutputColourSpace::label(self)
    }
}

impl SettingValue for ResourceStorage {
    const ALL: &'static [Self] = &ResourceStorage::ALL;
    const CONTROL: SettingControlKind = SettingControlKind::Dropdown;

    fn label(self) -> &'static str {
        ResourceStorage::label(self)
    }
}

impl SettingValue for EdgeFade {
    const ALL: &'static [Self] = &EdgeFade::ALL;
    const CONTROL: SettingControlKind = SettingControlKind::Dropdown;

    fn label(self) -> &'static str {
        EdgeFade::label(self)
    }
}

impl SettingValue for CheckpointInterval {
    const ALL: &'static [Self] = &CheckpointInterval::ALL;
    const CONTROL: SettingControlKind = SettingControlKind::Dropdown;

    fn label(self) -> &'static str {
        CheckpointInterval::label(self)
    }
}

impl SettingValue for CheckpointMemoryBudget {
    const ALL: &'static [Self] = &CheckpointMemoryBudget::ALL;
    const CONTROL: SettingControlKind = SettingControlKind::Dropdown;

    fn label(self) -> &'static str {
        CheckpointMemoryBudget::label(self)
    }
}

/// One setting, with the way to reach it in each of the two layers.
pub struct Setting<Value: SettingValue> {
    /// What the checkbox, or the drop-down list, is called.
    pub label: &'static str,
    pub global: fn(&mut Settings) -> &mut Value,
    pub project: fn(&mut ProjectSettings) -> &mut Option<Value>,
}

/// One setting that only the global layer has: a project cannot set it for itself (it describes the machine, or how the program is used, and not the project).
pub struct GlobalSetting<Value: SettingValue> {
    /// What the checkbox, or the drop-down list, is called.
    pub label: &'static str,
    pub global: fn(&mut Settings) -> &mut Value,
}

/// A [`Setting`] or a [`GlobalSetting`] of any kind of value, as far as the global layer goes, which is what lets the Global settings window (and the tests of the files) go
/// through all the settings in one list. A value is named by its number in the list of values the setting can have.
pub trait AnyGlobalSetting: Sync {
    fn label(&self) -> &'static str;
    fn control(&self) -> SettingControlKind;
    /// The label of each value, in order.
    fn option_labels(&self) -> Vec<&'static str>;
    /// The number of the value the global setting has.
    fn global_option(&self, settings: Settings) -> usize;
    fn set_global_option(&self, settings: &mut Settings, option: usize);
}

/// A [`Setting`] of any kind of value: one that a project can set for itself as well.
pub trait AnySetting: AnyGlobalSetting {
    /// The number of the value the project has set for itself, or `None` if it follows the global one.
    fn project_option(&self, settings: ProjectSettings) -> Option<usize>;
    fn set_project_option(&self, settings: &mut ProjectSettings, option: Option<usize>);
}

/// The number of `value` in the list of the values of its kind.
fn option_of<Value: SettingValue>(value: Value) -> usize {
    Value::ALL
        .iter()
        .position(|candidate| *candidate == value)
        .expect("every value is in the list of all values")
}

fn labels_of_the_values<Value: SettingValue>() -> Vec<&'static str> {
    Value::ALL.iter().map(|value| value.label()).collect()
}

fn global_option_of<Value: SettingValue>(access: fn(&mut Settings) -> &mut Value, mut settings: Settings) -> usize {
    option_of(*access(&mut settings))
}

fn set_global_option_of<Value: SettingValue>(
    access: fn(&mut Settings) -> &mut Value,
    settings: &mut Settings,
    option: usize,
) {
    if let Some(value) = Value::ALL.get(option) {
        *access(settings) = *value;
    }
}

impl<Value: SettingValue> AnyGlobalSetting for Setting<Value> {
    fn label(&self) -> &'static str {
        self.label
    }

    fn control(&self) -> SettingControlKind {
        Value::CONTROL
    }

    fn option_labels(&self) -> Vec<&'static str> {
        labels_of_the_values::<Value>()
    }

    fn global_option(&self, settings: Settings) -> usize {
        global_option_of(self.global, settings)
    }

    fn set_global_option(&self, settings: &mut Settings, option: usize) {
        set_global_option_of(self.global, settings, option);
    }
}

impl<Value: SettingValue> AnyGlobalSetting for GlobalSetting<Value> {
    fn label(&self) -> &'static str {
        self.label
    }

    fn control(&self) -> SettingControlKind {
        Value::CONTROL
    }

    fn option_labels(&self) -> Vec<&'static str> {
        labels_of_the_values::<Value>()
    }

    fn global_option(&self, settings: Settings) -> usize {
        global_option_of(self.global, settings)
    }

    fn set_global_option(&self, settings: &mut Settings, option: usize) {
        set_global_option_of(self.global, settings, option);
    }
}

impl<Value: SettingValue> AnySetting for Setting<Value> {
    fn project_option(&self, mut settings: ProjectSettings) -> Option<usize> {
        (*(self.project)(&mut settings)).map(option_of)
    }

    fn set_project_option(&self, settings: &mut ProjectSettings, option: Option<usize>) {
        *(self.project)(settings) = option.and_then(|number| Value::ALL.get(number).copied());
    }
}

const AUTOSAVE: Setting<bool> = Setting {
    label: "Autosave",
    global: |settings| &mut settings.autosave,
    project: |settings| &mut settings.autosave,
};

const SPED_UP_CLIP_SOUND: Setting<SpedUpClipSound> = Setting {
    label: "Sound of sped-up clips",
    global: |settings| &mut settings.sped_up_clip_sound,
    project: |settings| &mut settings.sped_up_clip_sound,
};

const DISPLAY_TRANSFORM: Setting<DisplayTransform> = Setting {
    label: "Display (how the screen shows colours)",
    global: |settings| &mut settings.display_transform,
    project: |settings| &mut settings.display_transform,
};

const WORKING_COLOUR_SPACE: Setting<WorkingColourSpace> = Setting {
    label: "Working colour space",
    global: |settings| &mut settings.working_colour_space,
    project: |settings| &mut settings.working_colour_space,
};

const OUTPUT_COLOUR_SPACE: Setting<OutputColourSpace> = Setting {
    label: "Colour space of exported videos",
    global: |settings| &mut settings.output_colour_space,
    project: |settings| &mut settings.output_colour_space,
};

const RESOURCE_STORAGE: Setting<ResourceStorage> = Setting {
    label: "Files that effects use (tables of colours)",
    global: |settings| &mut settings.resource_storage,
    project: |settings| &mut settings.resource_storage,
};

const EDGE_FADE: Setting<EdgeFade> = Setting {
    label: "Fade at the edge of a group (sound)",
    global: |settings| &mut settings.edge_fade,
    project: |settings| &mut settings.edge_fade,
};

const GENERATE_LEAD_UP_ON_SEEK: GlobalSetting<bool> = GlobalSetting {
    label: "Generate lead-up on seek",
    global: |settings| &mut settings.generate_lead_up_on_seek,
};

const CHECKPOINT_INTERVAL: GlobalSetting<CheckpointInterval> = GlobalSetting {
    label: "Checkpoint interval",
    global: |settings| &mut settings.checkpoint_interval,
};

const CHECKPOINT_MEMORY_BUDGET: GlobalSetting<CheckpointMemoryBudget> = GlobalSetting {
    label: "Checkpoint memory budget",
    global: |settings| &mut settings.checkpoint_memory_budget,
};

/// Every setting that a project can set for itself too, in the order the settings windows show them.
pub static SETTINGS: [&dyn AnySetting; 7] = [
    &AUTOSAVE,
    &SPED_UP_CLIP_SOUND,
    &DISPLAY_TRANSFORM,
    &WORKING_COLOUR_SPACE,
    &OUTPUT_COLOUR_SPACE,
    &RESOURCE_STORAGE,
    &EDGE_FADE,
];

/// Every setting that only the global layer has, in the order the Global settings window shows them (after those of [`SETTINGS`]).
pub static GLOBAL_ONLY_SETTINGS: [&dyn AnyGlobalSetting; 3] = [
    &GENERATE_LEAD_UP_ON_SEEK,
    &CHECKPOINT_INTERVAL,
    &CHECKPOINT_MEMORY_BUDGET,
];

/// Every setting that the global layer has: those of [`SETTINGS`], then those of [`GLOBAL_ONLY_SETTINGS`], in the order the Global settings window shows them.
pub fn every_global_setting() -> impl Iterator<Item = &'static dyn AnyGlobalSetting> {
    fn as_a_global_setting(setting: &'static dyn AnySetting) -> &'static dyn AnyGlobalSetting {
        setting
    }
    SETTINGS
        .iter()
        .copied()
        .map(as_a_global_setting)
        .chain(GLOBAL_ONLY_SETTINGS.iter().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every setting at a value that is not its default: the last of its values, except for the checkbox that is on to begin with, which is off.
    fn everything_off_its_default() -> Settings {
        Settings {
            autosave: true,
            sped_up_clip_sound: SpedUpClipSound::IncreasePitch,
            display_transform: DisplayTransform::Gamma24,
            working_colour_space: WorkingColourSpace::AcesCg,
            output_colour_space: OutputColourSpace::Srgb,
            resource_storage: ResourceStorage::Ask,
            edge_fade: EdgeFade::Milliseconds50,
            generate_lead_up_on_seek: false,
            checkpoint_interval: CheckpointInterval::Every300Frames,
            checkpoint_memory_budget: CheckpointMemoryBudget::Gib4,
        }
    }

    /// The number of a value of the setting that is not the one it has by default: the last, or the first where the last is the default (a checkbox that is on to begin with).
    fn an_option_that_is_not_the_default(setting: &dyn AnyGlobalSetting) -> usize {
        let last = setting.option_labels().len() - 1;
        if setting.global_option(Settings::DEFAULT) == last {
            0
        } else {
            last
        }
    }

    #[test]
    fn nothing_is_on_until_someone_turns_it_on_and_sped_up_sound_keeps_its_pitch() {
        assert_eq!(
            Settings::DEFAULT,
            Settings {
                autosave: false,
                sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
                ..Settings::DEFAULT
            },
            "in particular, no autosave, and the sound of fast clips is not muted"
        );
        assert_eq!(Settings::default(), Settings::DEFAULT);
        assert_eq!(SpedUpClipSound::default(), SpedUpClipSound::MaintainPitch);
    }

    #[test]
    fn the_three_kinds_of_sped_up_sound_are_listed_in_the_order_mute_maintain_increase() {
        let labels: Vec<&str> = SpedUpClipSound::ALL.iter().map(|sound| sound.label()).collect();
        assert_eq!(labels, ["Mute", "Maintain pitch", "Increase pitch"]);
    }

    #[test]
    fn a_project_that_follows_everything_gets_the_global_settings() {
        for global in [Settings::DEFAULT, everything_off_its_default()] {
            assert_eq!(ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS.resolve(global), global);
        }
        assert_eq!(ProjectSettings::default(), ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS);
    }

    #[test]
    fn a_setting_the_project_has_set_wins_over_the_global_one_whichever_way_it_differs() {
        let project = ProjectSettings {
            autosave: Some(true),
            sped_up_clip_sound: None,
            ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
        };
        assert_eq!(
            project.resolve(Settings::DEFAULT),
            Settings {
                autosave: true,
                sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
                ..Settings::DEFAULT
            }
        );

        let project = ProjectSettings {
            autosave: Some(false),
            sped_up_clip_sound: Some(SpedUpClipSound::Mute),
            ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
        };
        assert_eq!(
            project.resolve(everything_off_its_default()),
            Settings {
                autosave: false,
                sped_up_clip_sound: SpedUpClipSound::Mute,
                // (What the project does not set follows the global setting: the colour settings are at their last values there.)
                ..everything_off_its_default()
            },
            "the project's own choices beat the global ones"
        );

        let project = ProjectSettings {
            autosave: Some(false),
            sped_up_clip_sound: None,
            ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
        };
        assert_eq!(
            project.resolve(everything_off_its_default()).sped_up_clip_sound,
            SpedUpClipSound::IncreasePitch,
            "the setting it does not set follows the global one"
        );
    }

    #[test]
    fn the_tables_reach_every_setting_in_both_layers_and_each_row_reaches_a_different_one() {
        // Giving one row at a time a value that is not its default, in each layer, must change exactly that much, so
        // that two rows can never point at the same setting, and no setting is missing a row.
        let mut changed_globally = Settings::DEFAULT;
        let mut changed_in_the_project = ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS;
        for setting in SETTINGS {
            let option = an_option_that_is_not_the_default(setting);

            let before = changed_globally;
            setting.set_global_option(&mut changed_globally, option);
            assert_ne!(changed_globally, before, "{} reaches a global setting", setting.label());
            assert_eq!(setting.global_option(changed_globally), option, "{}", setting.label());

            let before = changed_in_the_project;
            setting.set_project_option(&mut changed_in_the_project, Some(option));
            assert_ne!(
                changed_in_the_project,
                before,
                "{} reaches a project setting",
                setting.label()
            );
            assert_eq!(
                setting.project_option(changed_in_the_project),
                Some(option),
                "{}",
                setting.label()
            );
        }
        let everything_a_project_can_set = changed_globally;
        for setting in GLOBAL_ONLY_SETTINGS {
            let option = an_option_that_is_not_the_default(setting);
            let before = changed_globally;
            setting.set_global_option(&mut changed_globally, option);
            assert_ne!(changed_globally, before, "{} reaches a global setting", setting.label());
            assert_eq!(setting.global_option(changed_globally), option, "{}", setting.label());
        }
        assert_eq!(
            changed_globally,
            everything_off_its_default(),
            "every global setting has a row"
        );
        // (What a project sets is never one of the global-only settings: those stay at the global values, here the defaults.)
        assert_eq!(
            changed_in_the_project.resolve(Settings::DEFAULT),
            everything_a_project_can_set,
            "every project setting has a row"
        );
        assert_eq!(
            everything_a_project_can_set,
            Settings {
                generate_lead_up_on_seek: Settings::DEFAULT.generate_lead_up_on_seek,
                checkpoint_interval: Settings::DEFAULT.checkpoint_interval,
                checkpoint_memory_budget: Settings::DEFAULT.checkpoint_memory_budget,
                ..everything_off_its_default()
            },
            "and the rows of the project's table are the first seven"
        );
    }

    #[test]
    fn the_settings_of_the_lead_up_are_on_and_every_60_frames_and_1_gib_to_begin_with_and_no_project_can_set_them() {
        assert!(Settings::default().generate_lead_up_on_seek, "on to begin with");
        assert_eq!(Settings::DEFAULT.checkpoint_interval, CheckpointInterval::Every60Frames);
        assert_eq!(Settings::DEFAULT.checkpoint_interval.frames(), 60);
        assert_eq!(Settings::DEFAULT.checkpoint_memory_budget, CheckpointMemoryBudget::Gib1);
        assert_eq!(Settings::DEFAULT.checkpoint_memory_budget.bytes(), 1 << 30);
        assert_eq!(CheckpointInterval::default(), Settings::DEFAULT.checkpoint_interval);
        assert_eq!(
            CheckpointMemoryBudget::default(),
            Settings::DEFAULT.checkpoint_memory_budget
        );

        // Whatever a project sets for itself, these come from the global settings.
        let everything_set_by_the_project = ProjectSettings {
            autosave: Some(true),
            sped_up_clip_sound: Some(SpedUpClipSound::Mute),
            display_transform: Some(DisplayTransform::Gamma22),
            working_colour_space: Some(WorkingColourSpace::AcesCg),
            output_colour_space: Some(OutputColourSpace::Srgb),
            resource_storage: Some(ResourceStorage::Link),
            edge_fade: Some(EdgeFade::Milliseconds20),
        };
        for global in [Settings::DEFAULT, everything_off_its_default()] {
            let in_effect = everything_set_by_the_project.resolve(global);
            assert_eq!(in_effect.generate_lead_up_on_seek, global.generate_lead_up_on_seek);
            assert_eq!(in_effect.checkpoint_interval, global.checkpoint_interval);
            assert_eq!(in_effect.checkpoint_memory_budget, global.checkpoint_memory_budget);
            assert!(in_effect.autosave, "(and what the project does set is its own)");
        }
    }

    #[test]
    fn the_checkpoint_settings_are_listed_from_the_least_to_the_most_and_say_what_they_mean() {
        assert_eq!(
            CheckpointInterval::ALL.map(CheckpointInterval::label),
            [
                "Every 30 frames",
                "Every 60 frames",
                "Every 120 frames",
                "Every 300 frames"
            ]
        );
        assert_eq!(
            CheckpointInterval::ALL.map(CheckpointInterval::frames),
            [30, 60, 120, 300]
        );
        assert_eq!(
            CheckpointMemoryBudget::ALL.map(CheckpointMemoryBudget::label),
            ["256 MiB", "512 MiB", "1 GiB", "2 GiB", "4 GiB"]
        );
        assert_eq!(
            CheckpointMemoryBudget::ALL.map(CheckpointMemoryBudget::bytes),
            [268_435_456, 536_870_912, 1_073_741_824, 2_147_483_648, 4_294_967_296]
        );
    }

    #[test]
    fn the_global_only_settings_are_one_checkbox_and_two_drop_down_lists_and_are_shown_after_the_others() {
        let controls: Vec<(&str, SettingControlKind)> = GLOBAL_ONLY_SETTINGS
            .iter()
            .map(|setting| (setting.label(), setting.control()))
            .collect();
        assert_eq!(
            controls,
            [
                ("Generate lead-up on seek", SettingControlKind::Checkbox),
                ("Checkpoint interval", SettingControlKind::Dropdown),
                ("Checkpoint memory budget", SettingControlKind::Dropdown),
            ]
        );
        let order: Vec<&str> = every_global_setting().map(|setting| setting.label()).collect();
        assert_eq!(order.len(), SETTINGS.len() + GLOBAL_ONLY_SETTINGS.len());
        assert_eq!(order[0], "Autosave");
        assert_eq!(
            order[SETTINGS.len()..],
            [
                "Generate lead-up on seek",
                "Checkpoint interval",
                "Checkpoint memory budget"
            ]
        );
    }

    #[test]
    fn letting_go_of_a_project_setting_makes_it_follow_the_global_one_again() {
        for setting in SETTINGS {
            let mut project = ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS;
            setting.set_project_option(&mut project, Some(1));
            assert_eq!(setting.project_option(project), Some(1), "{}", setting.label());
            setting.set_project_option(&mut project, None);
            assert_eq!(
                project,
                ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
                "{}",
                setting.label()
            );
        }
    }

    #[test]
    fn a_number_that_is_not_a_value_of_the_setting_changes_nothing() {
        for setting in SETTINGS {
            let mut global = Settings::DEFAULT;
            setting.set_global_option(&mut global, 99);
            assert_eq!(global, Settings::DEFAULT, "{}", setting.label());
            let mut project = ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS;
            setting.set_project_option(&mut project, Some(99));
            assert_eq!(
                project,
                ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
                "{}",
                setting.label()
            );
        }
        for setting in GLOBAL_ONLY_SETTINGS {
            let mut global = Settings::DEFAULT;
            setting.set_global_option(&mut global, 99);
            assert_eq!(global, Settings::DEFAULT, "{}", setting.label());
        }
    }

    #[test]
    fn every_setting_has_its_own_label_and_at_least_two_values_each_with_its_own_label() {
        let mut labels: Vec<&str> = every_global_setting().map(|setting| setting.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), SETTINGS.len() + GLOBAL_ONLY_SETTINGS.len());
        for setting in every_global_setting() {
            let mut option_labels = setting.option_labels();
            assert!(option_labels.len() >= 2, "{}", setting.label());
            option_labels.sort_unstable();
            option_labels.dedup();
            assert_eq!(
                option_labels.len(),
                setting.option_labels().len(),
                "{}",
                setting.label()
            );
        }
    }

    #[test]
    fn checkboxes_are_for_the_settings_with_two_values_and_dropdowns_for_the_others() {
        let controls: Vec<(&str, SettingControlKind)> = SETTINGS
            .iter()
            .map(|setting| (setting.label(), setting.control()))
            .collect();
        assert_eq!(
            controls,
            [
                ("Autosave", SettingControlKind::Checkbox),
                ("Sound of sped-up clips", SettingControlKind::Dropdown),
                ("Display (how the screen shows colours)", SettingControlKind::Dropdown),
                ("Working colour space", SettingControlKind::Dropdown),
                ("Colour space of exported videos", SettingControlKind::Dropdown),
                // (Added on purpose in phase 4: the setting for how the files that effects use are kept.)
                (
                    "Files that effects use (tables of colours)",
                    SettingControlKind::Dropdown
                ),
                // (Added on purpose in phase 12: the length of the fade where a sound meets the edge of a group.)
                ("Fade at the edge of a group (sound)", SettingControlKind::Dropdown),
            ]
        );
    }

    #[test]
    fn a_new_resource_is_kept_as_the_setting_says_except_that_plain_json_has_no_room_to_embed_it() {
        use HowToKeepAResource::{Ask, Embed, Link, LinkBecauseTheFileIsPlainJson};
        use ProjectFileKind::{Archive, NotSavedYet, PlainJson};
        for (storage, file, wanted) in [
            (ResourceStorage::Embed, Archive, Embed),
            (ResourceStorage::Embed, NotSavedYet, Embed),
            (ResourceStorage::Embed, PlainJson, LinkBecauseTheFileIsPlainJson),
            (ResourceStorage::Link, Archive, Link),
            (ResourceStorage::Link, NotSavedYet, Link),
            (ResourceStorage::Link, PlainJson, Link),
            (ResourceStorage::Ask, Archive, Ask),
            (ResourceStorage::Ask, NotSavedYet, Ask),
            (ResourceStorage::Ask, PlainJson, Ask),
        ] {
            assert_eq!(storage.how_to_keep(file), wanted, "{storage:?} in {file:?}");
        }
    }

    #[test]
    fn files_that_effects_use_are_embedded_unless_someone_says_otherwise_and_the_project_can_say_otherwise_for_itself()
    {
        assert_eq!(Settings::DEFAULT.resource_storage, ResourceStorage::Embed);
        assert_eq!(ResourceStorage::default(), ResourceStorage::Embed);
        let labels: Vec<&str> = ResourceStorage::ALL.iter().map(|storage| storage.label()).collect();
        assert_eq!(
            labels,
            ["Embed in the project file", "Link to the file", "Ask each time"]
        );
        let global = Settings {
            resource_storage: ResourceStorage::Link,
            ..Settings::DEFAULT
        };
        assert_eq!(
            ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
                .resolve(global)
                .resource_storage,
            ResourceStorage::Link,
            "a project that says nothing follows the global setting"
        );
        let project = ProjectSettings {
            resource_storage: Some(ResourceStorage::Ask),
            ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
        };
        assert_eq!(
            project.resolve(global).resource_storage,
            ResourceStorage::Ask,
            "the project's beats the global"
        );
        assert_eq!(
            project.resolve(Settings::DEFAULT).resource_storage,
            ResourceStorage::Ask
        );
    }

    #[test]
    fn the_colour_settings_default_to_what_the_program_did_before_them_and_each_value_has_its_label() {
        assert_eq!(Settings::DEFAULT.display_transform, DisplayTransform::Srgb);
        assert_eq!(Settings::DEFAULT.working_colour_space, WorkingColourSpace::LinearRec709);
        assert_eq!(
            Settings::DEFAULT.output_colour_space,
            OutputColourSpace::Rec709AsPreviewed
        );
        assert_eq!(
            DisplayTransform::ALL.map(DisplayTransform::label),
            ["sRGB", "Gamma 2.2", "Gamma 2.4 (BT.1886)"]
        );
        assert_eq!(
            WorkingColourSpace::ALL.map(WorkingColourSpace::label),
            ["Rec.709 (linear)", "ACEScg"]
        );
        assert_eq!(
            OutputColourSpace::ALL.map(OutputColourSpace::label),
            [
                "Rec.709, as the preview shows it",
                "Rec.709, BT.1886 (gamma 2.4)",
                "sRGB"
            ]
        );
    }

    #[test]
    fn the_colour_settings_mean_the_curves_and_primaries_of_the_colour_module() {
        assert_eq!(DisplayTransform::Srgb.transfer_function(), TransferFunction::Srgb);
        assert_eq!(DisplayTransform::Gamma22.transfer_function(), TransferFunction::Gamma22);
        assert_eq!(DisplayTransform::Gamma24.transfer_function(), TransferFunction::Gamma24);
        assert_eq!(WorkingColourSpace::LinearRec709.primaries(), StandardPrimaries::Rec709);
        assert_eq!(WorkingColourSpace::AcesCg.primaries(), StandardPrimaries::AcesCg);
        // "As the preview shows it" follows the display; the other two do not.
        for display in DisplayTransform::ALL {
            assert_eq!(
                OutputColourSpace::Rec709AsPreviewed.transfer_function(display),
                display.transfer_function()
            );
            assert_eq!(
                OutputColourSpace::Rec709Bt1886.transfer_function(display),
                TransferFunction::Gamma24
            );
            assert_eq!(
                OutputColourSpace::Srgb.transfer_function(display),
                TransferFunction::Srgb
            );
        }
        for output in OutputColourSpace::ALL {
            assert_eq!(output.primaries(), StandardPrimaries::Rec709, "{output:?}");
        }
        // What the file says: the two that are Rec.709 video say bt709, and the one that is sRGB says so.
        assert_eq!(OutputColourSpace::Rec709AsPreviewed.transfer_tag(), "bt709");
        assert_eq!(OutputColourSpace::Rec709Bt1886.transfer_tag(), "bt709");
        assert_eq!(OutputColourSpace::Srgb.transfer_tag(), "iec61966-2-1");
    }

    #[test]
    fn the_values_of_a_setting_are_serialised_as_plain_snake_case_words() {
        let words: Vec<String> = SpedUpClipSound::ALL
            .iter()
            .map(|sound| serde_json::to_string(sound).unwrap())
            .collect();
        assert_eq!(words, ["\"mute\"", "\"maintain_pitch\"", "\"increase_pitch\""]);
        assert!(serde_json::from_str::<SpedUpClipSound>("\"loud\"").is_err());
    }

    #[test]
    fn the_values_of_the_checkpoint_settings_are_serialised_as_words_that_say_the_number() {
        let words: Vec<String> = CheckpointInterval::ALL
            .iter()
            .map(|value| serde_json::to_string(value).unwrap())
            .collect();
        assert_eq!(
            words,
            [
                "\"every_30_frames\"",
                "\"every_60_frames\"",
                "\"every_120_frames\"",
                "\"every_300_frames\""
            ]
        );
        let words: Vec<String> = CheckpointMemoryBudget::ALL
            .iter()
            .map(|value| serde_json::to_string(value).unwrap())
            .collect();
        assert_eq!(
            words,
            ["\"256_mib\"", "\"512_mib\"", "\"1_gib\"", "\"2_gib\"", "\"4_gib\""]
        );
        for value in CheckpointInterval::ALL {
            let again: CheckpointInterval = serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
            assert_eq!(again, value);
        }
        for value in CheckpointMemoryBudget::ALL {
            let again: CheckpointMemoryBudget = serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
            assert_eq!(again, value);
        }
        assert!(serde_json::from_str::<CheckpointInterval>("\"every_7_frames\"").is_err());
    }
}
