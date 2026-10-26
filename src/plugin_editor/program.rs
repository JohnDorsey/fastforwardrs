// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The inside of the editor-window program (`fastforwardrs-plugin-editor`, which is only this, called from `main`): **it loads the plugin library it is given**, makes an
//! [`clap_host::Editor`] of a plugin, and shows the plugin's window; what a person does in the window it says on its standard output, and what the editor says (the values the
//! project has, the plugin's state) it reads on its standard input (see [`super::protocol`]). It is a program of its own so that a plugin whose window crashes does not take the
//! editor and the project with it, and so that the plugin's window has the main thread of a program that has a message loop, on every system (P1 of the design).
//!
//! The window is **embedded** (in a window of this program, made with `winit`) when the plugin can do that, and **floating** (the plugin's own) when it cannot. With `--headless`
//! there is no window at all: the plugin is asked for a floating one and nothing is drawn, so that everything else can be tested where there is no screen.

use std::ffi::OsString;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use clap_host::{Editor, EditorEvent, Library};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use super::protocol::{FromProgram, ToProgram, line_of, message_in};
use crate::base64;

/// What the program says when it is used wrongly.
pub const USAGE: &str =
    "Usage: fastforwardrs-plugin-editor --library <file> --id <plugin id> [--title <words>] [--headless]

Shows the window of a CLAP plugin. It is started by the editor, which talks to it in lines on its
standard input and output (see plugin_editor::protocol); it ends when the window is closed.
";

/// How long the program waits for the editor's first message (`Begin`) before it gives up. (The editor says it at once.)
const HOW_LONG_TO_WAIT_FOR_BEGIN: Duration = Duration::from_secs(20);

/// How often the plugin is looked at for what it did (edits, a change of state, a request for another size): a window that is dragged needs the main thread often.
const HOW_OFTEN_TO_LOOK: Duration = Duration::from_millis(8);

/// The size that the window of this program is to be made, if the plugin says its window is `said` and the window was last made `told`: the one the plugin says, when it is another
/// and it is a size at all.
fn the_size_to_follow(told: Option<(u32, u32)>, said: Option<(u32, u32)>) -> Option<(u32, u32)> {
    said.filter(|size| Some(*size) != told && size.0 > 0 && size.1 > 0)
}

/// How often the program asks the plugin how big its window is, to follow it when it changes it by itself (see `follow_the_size_the_plugin_says`).
const HOW_OFTEN_TO_ASK_THE_SIZE: Duration = Duration::from_millis(100);

/// What the program was started with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arguments {
    pub library: PathBuf,
    pub id: String,
    pub title: String,
    pub headless: bool,
}

/// Reads what the program was started with (not its name).
pub fn parse_arguments(arguments: impl IntoIterator<Item = OsString>) -> Result<Arguments, String> {
    let (mut library, mut id, mut title, mut headless) = (None, None, None, false);
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| arguments.next().ok_or_else(|| format!("{name} needs a value after it"));
        match argument.to_string_lossy().as_ref() {
            "--library" => library = Some(PathBuf::from(value("--library")?)),
            "--id" => id = Some(value("--id")?.to_string_lossy().into_owned()),
            "--title" => title = Some(value("--title")?.to_string_lossy().into_owned()),
            "--headless" => headless = true,
            other => return Err(format!("{other:?} is not something this program is told")),
        }
    }
    let id = id.ok_or("--id is needed")?;
    Ok(Arguments {
        library: library.ok_or("--library is needed")?,
        title: title.unwrap_or_else(|| id.clone()),
        id,
        headless,
    })
}

/// The kind of window of this system that a plugin is asked for.
fn the_kind_of_window() -> &'static str {
    if cfg!(target_os = "windows") {
        "win32"
    } else if cfg!(target_os = "macos") {
        "cocoa"
    } else {
        "x11"
    }
}

/// What comes in: a message from the editor, or the end of the lines (the editor is gone).
enum Wake {
    Message(ToProgram),
    InputEnded,
}

/// Reads the lines of `input` on a thread of its own and sends what they say. (Noise is thrown away.)
fn read_lines(input: impl BufRead + Send + 'static, send: impl Fn(Wake) + Send + 'static) {
    std::thread::spawn(move || {
        for line in input.lines() {
            let Ok(line) = line else { break };
            if let Some(message) = message_in::<ToProgram>(&line) {
                send(Wake::Message(message));
            }
        }
        send(Wake::InputEnded);
    });
}

/// The plugin, and what is said about it.
struct Session {
    editor: Editor,
    out: Box<dyn Write>,
    /// Whether the plugin said that its state is not what was last said (`StateChanged`), and the editor has not asked for it since: the state is said when the program ends, so that what a
    /// person did in the window last is not lost to the project.
    state_is_not_what_was_said: bool,
}

impl Session {
    /// Says a message on its own line (a plugin may have printed something that does not end in one).
    fn say(&mut self, message: &FromProgram) {
        let _ = write!(self.out, "\n{}\n", line_of(message));
        let _ = self.out.flush();
    }

    fn fail(&mut self, reason: impl Into<String>) -> ! {
        self.say(&FromProgram::Failed { reason: reason.into() });
        std::process::exit(1)
    }

    /// Ends the program: the window is destroyed first, and the library is not unloaded (some crash when they are).
    fn end(mut self) -> ! {
        if self.state_is_not_what_was_said
            && let Ok(bytes) = self.editor.save_the_state()
        {
            self.say(&FromProgram::State {
                state: base64::encode(&bytes),
            });
        }
        self.editor.destroy_the_window();
        self.say(&FromProgram::Closed);
        // (Dropping the editor would be the plugin's `destroy`; the process ends here and now.)
        std::process::exit(0)
    }

    /// Does what the first message says: the state, and then the values.
    fn begin(&mut self, values: &[(u32, f64)], state: Option<&str>) {
        if let Some(state) = state {
            self.load_the_state(state);
        }
        // (What the plugin says back to a value it was told is thrown away.)
        let _ = self.editor.tell_the_values(values);
    }

    fn load_the_state(&mut self, state: &str) {
        match base64::decode(state) {
            Ok(bytes) => {
                // (A plugin that cannot take a state it was given is not a reason to close the window: the values are what the sound has.)
                let _ = self.editor.load_the_state(&bytes);
                // (A plugin that says that its state changed because it was given one has not changed by a person's hand: the editor would take it for an edit that nobody made.)
                self.editor.forget_that_the_state_changed();
            }
            Err(_) => { /* a state that is not base 64 is not given to the plugin */ }
        }
    }

    /// Does one message of the editor after the first. Whether to go on.
    fn obey(&mut self, message: ToProgram) -> bool {
        match message {
            ToProgram::Begin { .. } => {}
            ToProgram::Values { values } => {
                let _ = self.editor.tell_the_values(&values);
            }
            ToProgram::LoadState { state } => self.load_the_state(&state),
            ToProgram::SaveState => match self.editor.save_the_state() {
                Ok(bytes) => {
                    self.state_is_not_what_was_said = false;
                    self.say(&FromProgram::State {
                        state: base64::encode(&bytes),
                    });
                }
                Err(error) => self.say(&FromProgram::Failed {
                    reason: error.to_string(),
                }),
            },
            ToProgram::Close => return false,
        }
        true
    }

    /// Looks at what the plugin did and says it. What is about the window (a request for another size, to show, to hide, a close) is given back to be done by whoever has the window.
    fn look(&mut self) -> Vec<EditorEvent> {
        let mut for_the_window = Vec::new();
        for event in self.editor.poll() {
            match event {
                EditorEvent::Edit(edit) => self.say(&FromProgram::Edit { edit }),
                EditorEvent::StateChanged => {
                    self.state_is_not_what_was_said = true;
                    self.say(&FromProgram::StateChanged);
                }
                EditorEvent::ParametersChanged => self.say(&FromProgram::ParametersChanged),
                other => for_the_window.push(other),
            }
        }
        for_the_window
    }
}

/// Makes the editor of the plugin, or says why not and ends.
fn the_editor_or_end(arguments: &Arguments, out: &mut dyn Write) -> Editor {
    let fail = |out: &mut dyn Write, reason: String| -> ! {
        let _ = write!(out, "\n{}\n", line_of(&FromProgram::Failed { reason }));
        let _ = out.flush();
        std::process::exit(1)
    };
    let library = match Library::open(&arguments.library) {
        Ok(library) => library,
        Err(error) => fail(out, format!("the library could not be loaded: {error}")),
    };
    let mut editor = match Editor::new(&library, &arguments.id) {
        Ok(editor) => editor,
        Err(error) => fail(out, error.to_string()),
    };
    if !editor.has_a_window() {
        fail(out, "the plugin has no window".to_owned());
    }
    // (The library stays loaded for as long as the program does.)
    std::mem::forget(library);
    editor
}

/// Runs the program: **this is `main`** of `fastforwardrs-plugin-editor`, which does not return (it ends with the window).
pub fn run(arguments: Arguments, input: impl BufRead + Send + 'static, out: Box<dyn Write>) -> ! {
    // (Stops the system from asking a person what to do about a crash: the editor is told, by the end of this program's output.)
    ladspa_host::prepare_to_crash_quietly();
    let mut out = out;
    let editor = the_editor_or_end(&arguments, &mut *out);
    let session = Session {
        editor,
        out,
        state_is_not_what_was_said: false,
    };
    if arguments.headless {
        run_without_a_window(session, input)
    } else {
        run_with_a_window(arguments, session, input)
    }
}

/// With no window: the plugin is asked for a floating window, which it makes and shows without a screen (the test plugin draws nothing).
fn run_without_a_window(mut session: Session, input: impl BufRead + Send + 'static) -> ! {
    let (sender, receiver): (Sender<Wake>, Receiver<Wake>) = mpsc::channel();
    read_lines(input, move |wake| {
        let _ = sender.send(wake);
    });
    let Ok(Wake::Message(ToProgram::Begin { values, state })) = receiver.recv_timeout(HOW_LONG_TO_WAIT_FOR_BEGIN)
    else {
        session.fail("the editor did not say how to begin")
    };
    session.begin(&values, state.as_deref());
    let api = the_kind_of_window();
    let size = match session.editor.make_the_window(api, true, 1.0) {
        Ok(size) => size,
        Err(error) => session.fail(error.to_string()),
    };
    if let Err(error) = session.editor.show() {
        session.fail(error.to_string());
    }
    session.say(&FromProgram::Ready {
        width: size.width,
        height: size.height,
        can_resize: size.can_resize,
        embedded: false,
        has_state: session.editor.has_state(),
    });
    loop {
        for event in session.look() {
            if matches!(event, EditorEvent::Closed { .. }) {
                session.end();
            }
        }
        match receiver.recv_timeout(HOW_OFTEN_TO_LOOK) {
            Ok(Wake::Message(message)) => {
                if !session.obey(message) {
                    session.end();
                }
            }
            Ok(Wake::InputEnded) | Err(RecvTimeoutError::Disconnected) => session.end(),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// With a window: `winit` has the main thread and its message loop, which the plugin's window needs too.
fn run_with_a_window(arguments: Arguments, session: Session, input: impl BufRead + Send + 'static) -> ! {
    let event_loop = match EventLoop::<Wake>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            let mut session = session;
            session.fail(format!("the program could not start its message loop: {error}"))
        }
    };
    let proxy: EventLoopProxy<Wake> = event_loop.create_proxy();
    read_lines(input, move |wake| {
        let _ = proxy.send_event(wake);
    });
    let mut app = WindowedProgram {
        arguments,
        session: Some(session),
        window: None,
        began: false,
        last_size: None,
        can_resize: false,
        size_asked_at: Instant::now(),
        wake_by: Instant::now() + HOW_LONG_TO_WAIT_FOR_BEGIN,
    };
    let _ = event_loop.run_app(&mut app);
    // (The loop ended without the program ending: it is over all the same.)
    match app.session.take() {
        Some(session) => session.end(),
        None => std::process::exit(0),
    }
}

struct WindowedProgram {
    arguments: Arguments,
    session: Option<Session>,
    /// The window of this program that the plugin's window is in, if it is embedded.
    window: Option<Arc<Window>>,
    began: bool,
    /// The size (in the pixels of the window) that the plugin was last told, so that a resize that this program asked for is not taken for one that a person made.
    last_size: Option<(u32, u32)>,
    can_resize: bool,
    /// When the size that the plugin says its window is was last asked.
    size_asked_at: Instant,
    /// When the program gives up waiting for the editor's first message.
    wake_by: Instant,
}

impl WindowedProgram {
    /// Makes the window: embedded in a window of this program if the plugin can, floating (its own) if not.
    fn make_the_window(&mut self, event_loop: &ActiveEventLoop) {
        let api = the_kind_of_window();
        let title = self.arguments.title.clone();
        let session = self
            .session
            .as_mut()
            .expect("the session is there until the program ends");
        let mut said_why_not = String::new();
        if session.editor.can_make_a_window(api, false) {
            match event_loop.create_window(
                Window::default_attributes()
                    .with_title(title.clone())
                    .with_visible(false),
            ) {
                Ok(window) => {
                    let window = Arc::new(window);
                    match session.editor.make_the_window(api, false, window.scale_factor()) {
                        Ok(size) => {
                            self.can_resize = size.can_resize;
                            window.set_resizable(size.can_resize);
                            let wanted = if api == "cocoa" {
                                let logical = LogicalSize::new(size.width, size.height);
                                logical.to_physical::<u32>(window.scale_factor())
                            } else {
                                PhysicalSize::new(size.width, size.height)
                            };
                            let _ = window.request_inner_size(wanted);
                            self.last_size = Some((wanted.width, wanted.height));
                            if session.editor.put_the_window_in_a_window(Arc::clone(&window)).is_ok() {
                                window.set_visible(true);
                                match session.editor.show() {
                                    Ok(()) => {
                                        let has_state = session.editor.has_state();
                                        session.say(&FromProgram::Ready {
                                            width: size.width,
                                            height: size.height,
                                            can_resize: size.can_resize,
                                            embedded: true,
                                            has_state,
                                        });
                                        self.window = Some(window);
                                        return;
                                    }
                                    Err(error) => said_why_not = error.to_string(),
                                }
                            } else {
                                said_why_not =
                                    "the plugin could not put its window in the window of the program".to_owned();
                            }
                            // (It did not work: the window that was made is taken down, and a floating one is tried.)
                            session.editor.destroy_the_window();
                        }
                        Err(error) => said_why_not = error.to_string(),
                    }
                }
                Err(error) => said_why_not = format!("the program could not make a window: {error}"),
            }
        }
        if !session.editor.can_make_a_window(api, true) {
            let why = if said_why_not.is_empty() {
                format!("the plugin cannot make a window of the kind {api}")
            } else {
                said_why_not
            };
            session.fail(why);
        }
        match session.editor.make_the_window(api, true, 1.0) {
            Ok(size) => {
                session.editor.suggest_a_title(&title);
                if let Err(error) = session.editor.show() {
                    session.fail(error.to_string());
                }
                let has_state = session.editor.has_state();
                session.say(&FromProgram::Ready {
                    width: size.width,
                    height: size.height,
                    can_resize: size.can_resize,
                    embedded: false,
                    has_state,
                });
            }
            Err(error) => session.fail(error.to_string()),
        }
    }

    /// Does what the plugin asked about its window.
    fn do_what_the_window_was_asked(&mut self, events: Vec<EditorEvent>) {
        for event in events {
            match event {
                EditorEvent::ResizeRequested { width, height } => {
                    if let Some(window) = &self.window {
                        let wanted = PhysicalSize::new(width, height);
                        self.last_size = Some((width, height));
                        let _ = window.request_inner_size(wanted);
                    }
                }
                EditorEvent::ShowRequested => {
                    if let Some(window) = &self.window {
                        window.set_visible(true);
                    }
                }
                EditorEvent::HideRequested => {
                    if let Some(window) = &self.window {
                        window.set_visible(false);
                    }
                }
                EditorEvent::Closed { .. } => self.end(),
                _ => {}
            }
        }
    }

    /// Makes the window of this program as big as the plugin says its window is, when that is not what it was. **Some plugins change the size of their window after they have shown it,
    /// without asking** (Surge XT applies a zoom of its own a moment later): a host that only believes what it was asked would show them cut off.
    fn follow_the_size_the_plugin_says(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        if self.size_asked_at.elapsed() < HOW_OFTEN_TO_ASK_THE_SIZE {
            return;
        }
        self.size_asked_at = Instant::now();
        let session = self
            .session
            .as_mut()
            .expect("the session is there until the program ends");
        if let Some(size) = the_size_to_follow(self.last_size, session.editor.size_of_the_window()) {
            self.last_size = Some(size);
            let _ = window.request_inner_size(PhysicalSize::new(size.0, size.1));
        }
    }

    fn end(&mut self) -> ! {
        // (The plugin's window is destroyed before the window it is in is: the session does that, and this program's window goes with the process.)
        let session = self
            .session
            .take()
            .expect("the session is there until the program ends");
        session.end()
    }
}

impl ApplicationHandler<Wake> for WindowedProgram {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + HOW_OFTEN_TO_LOOK));
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, wake: Wake) {
        match wake {
            Wake::InputEnded => self.end(),
            Wake::Message(ToProgram::Begin { values, state }) if !self.began => {
                self.began = true;
                self.session
                    .as_mut()
                    .expect("the session is there until the program ends")
                    .begin(&values, state.as_deref());
                self.make_the_window(event_loop);
            }
            Wake::Message(message) => {
                let go_on = self
                    .session
                    .as_mut()
                    .expect("the session is there until the program ends")
                    .obey(message);
                if !go_on {
                    self.end();
                }
            }
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _window: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.end(),
            WindowEvent::Resized(size) => {
                let (width, height) = (size.width, size.height);
                if !self.can_resize || width == 0 || height == 0 || self.last_size == Some((width, height)) {
                    return;
                }
                let session = self
                    .session
                    .as_mut()
                    .expect("the session is there until the program ends");
                // (The plugin says what size it can be nearest to what the person made it; if that is another, the window is made that.)
                let (width, height) = session.editor.adjust_the_size(width, height).unwrap_or((width, height));
                session.editor.set_the_size(width, height);
                if (width, height) != (size.width, size.height)
                    && let Some(window) = &self.window
                {
                    let _ = window.request_inner_size(PhysicalSize::new(width, height));
                }
                self.last_size = Some((width, height));
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(session) = self.session.as_mut() {
                    session.editor.set_the_scale(scale_factor);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.session.is_none() {
            return;
        }
        if !self.began && Instant::now() > self.wake_by {
            self.session
                .as_mut()
                .expect("the session is there until the program ends")
                .fail("the editor did not say how to begin");
        }
        self.follow_the_size_the_plugin_says();
        let events = self
            .session
            .as_mut()
            .expect("the session is there until the program ends")
            .look();
        self.do_what_the_window_was_asked(events);
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + HOW_OFTEN_TO_LOOK));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments_of(words: &[&str]) -> Result<Arguments, String> {
        parse_arguments(words.iter().map(OsString::from))
    }

    #[test]
    fn the_arguments_are_a_library_an_id_and_optionally_a_title_and_headless() {
        assert_eq!(
            arguments_of(&[
                "--library",
                "a.clap",
                "--id",
                "x.y",
                "--title",
                "Hello there",
                "--headless"
            ]),
            Ok(Arguments {
                library: PathBuf::from("a.clap"),
                id: "x.y".to_owned(),
                title: "Hello there".to_owned(),
                headless: true,
            })
        );
        let plain = arguments_of(&["--id", "x.y", "--library", "a.clap"]).unwrap();
        assert_eq!(plain.title, "x.y", "the title is the id when none is given");
        assert!(!plain.headless);
    }

    #[test]
    fn the_window_follows_the_size_that_the_plugin_says_when_it_is_another_and_a_size() {
        assert_eq!(the_size_to_follow(Some((600, 490)), Some((735, 600))), Some((735, 600)));
        assert_eq!(
            the_size_to_follow(None, Some((735, 600))),
            Some((735, 600)),
            "none told yet"
        );
        assert_eq!(the_size_to_follow(Some((600, 490)), Some((600, 490))), None, "the same");
        assert_eq!(
            the_size_to_follow(Some((600, 490)), None),
            None,
            "the plugin says nothing"
        );
        assert_eq!(the_size_to_follow(Some((600, 490)), Some((0, 490))), None, "not a size");
        assert_eq!(the_size_to_follow(Some((600, 490)), Some((600, 0))), None, "not a size");
    }

    #[test]
    fn what_is_missing_or_not_known_is_said() {
        assert!(arguments_of(&["--id", "x"]).unwrap_err().contains("--library"));
        assert!(arguments_of(&["--library", "a"]).unwrap_err().contains("--id"));
        assert!(arguments_of(&["--library"]).unwrap_err().contains("needs a value"));
        assert!(arguments_of(&["--what"]).unwrap_err().contains("--what"));
    }
}
