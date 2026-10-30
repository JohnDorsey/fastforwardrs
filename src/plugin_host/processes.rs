// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which plugins the host runs in a process of their own (phase 14, `docs/engine-phase-14-processes-design.md`), and which program that is.

use std::path::PathBuf;

use crate::plugin_process::remote::{
    Launcher, Limits, PROGRAM_NAME, check_that_the_program_is_of_this_build, program_beside_this_program,
};
use crate::project::InstalledPlugin;

/// Which plugins run in a process of their own, and not in the editor's: **a plugin that crashes ends the process it runs in**, and in the editor's that is the editor and the project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InAProcessOfItsOwn {
    /// None: every plugin runs in the editor's process. (What `PluginHost::new()` does; the tests of the host that was made before processes were run in the editor's.)
    #[default]
    Nothing,
    /// The instruments (the plugins that make sound from notes). What the editor and the export program do, because Surge XT, which is one, was measured to crash at random when a patch is
    /// loaded and notes are played (`docs/bundleable-synthesizers.md`).
    Instruments,
    /// Every sound plugin.
    Everything,
}

impl InAProcessOfItsOwn {
    /// Whether `plugin` is one of those that run in a process of their own.
    pub fn includes(self, plugin: &InstalledPlugin) -> bool {
        match self {
            Self::Nothing => false,
            Self::Instruments => plugin.descriptor.makes_sound_from_notes,
            Self::Everything => true,
        }
    }
}

/// **Which plugins the editor and the export program run in a process of their own.** One place to change when more are wanted (the maintainer said, 2026-10-30: instruments now, "other
/// sound plugins later").
pub const THE_PLUGINS_IN_A_PROCESS_OF_THEIR_OWN: InAProcessOfItsOwn = InAProcessOfItsOwn::Instruments;

/// What the host knows about the processes: which plugins, and the program.
#[derive(Default)]
pub(super) struct Processes {
    pub(super) which: InAProcessOfItsOwn,
    /// The program, when it was given and is not to be looked for beside the editor (the tests are given the one that `cargo` built).
    given_program: Option<PathBuf>,
    /// The program, once it was found and checked to be of this build. (A program that was not is looked for again at the next plugin: a person may have built it since.)
    checked_program: Option<PathBuf>,
    /// What the program's environment is given, besides the editor's own and what finds the libraries that a plugin needs (for the tests of what a program does when a plugin misbehaves).
    pub(super) extra_environment: Vec<(String, String)>,
    pub(super) limits: Option<Limits>,
}

impl Processes {
    pub(super) fn given_the_program(&mut self, program: PathBuf) {
        self.given_program = Some(program);
        self.checked_program = None;
    }

    /// The program that runs plugins, found beside the editor (or given) and checked to be built from the code of the editor; or why not.
    fn the_program(&mut self) -> Result<PathBuf, String> {
        if let Some(checked) = &self.checked_program {
            return Ok(checked.clone());
        }
        let program = self.given_program.clone().or_else(program_beside_this_program).ok_or_else(|| {
            format!("the program that runs plugins ({PROGRAM_NAME}) is not beside the editor: build everything together (cargo build --bins)")
        })?;
        check_that_the_program_is_of_this_build(&program)?;
        self.checked_program = Some(program.clone());
        Ok(program)
    }

    /// What starts the program for a plugin, with `environment` (where the libraries that the plugin needs are looked for), or why it cannot be started.
    pub(super) fn launcher(&mut self, environment: Vec<(String, String)>) -> Result<Launcher, String> {
        let mut launcher = Launcher::new(self.the_program()?).with_environment(
            environment
                .into_iter()
                .chain(self.extra_environment.iter().cloned())
                .collect(),
        );
        if let Some(limits) = self.limits {
            launcher = launcher.with_limits(limits);
        }
        Ok(launcher)
    }
}
