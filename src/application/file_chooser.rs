//! Asking the person which file to use.
//!
//! A file dialog makes the program wait until someone answers it. So the program does not open
//! one itself: it asks through a [`FileChooser`]. The real one ([`NativeFileChooser`]) opens the
//! dialog of the operating system. The one the tests are given ([`RefusingFileChooser`]) fails the
//! test at once instead, because a test that reached a dialog would wait for ever for an answer
//! that nobody is there to give, and a mistake in the code in front of a dialog (such as thinking
//! that there are no unsaved changes) would turn from a failing test into a test that hangs.

use std::path::PathBuf;

/// One kind of file to offer, such as "Video files" with the endings `mp4` and `mov`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileKind<'a> {
    pub name: &'a str,
    /// The endings of the files' names, without the dot. `"*"` stands for any file.
    pub extensions: &'a [&'a str],
}

/// What a file dialog asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileQuestion<'a> {
    pub title: &'a str,
    /// The name offered first, when asking where to save.
    pub suggested_file_name: Option<&'a str>,
    /// The kinds of file to offer, the first of them first.
    pub kinds: &'a [FileKind<'a>],
}

/// Something that can ask the person for a file.
pub trait FileChooser {
    /// Asks which existing file to open. `None` if the person chose none.
    fn choose_a_file_to_open(&self, question: &FileQuestion<'_>) -> Option<PathBuf>;

    /// Asks where to save a file, which need not exist yet. `None` if the person chose nowhere.
    fn choose_where_to_save(&self, question: &FileQuestion<'_>) -> Option<PathBuf>;
}

/// Asks with the file dialog of the operating system.
pub struct NativeFileChooser;

impl NativeFileChooser {
    fn dialog_for(question: &FileQuestion<'_>) -> rfd::FileDialog {
        let mut dialog = rfd::FileDialog::new().set_title(question.title);
        if let Some(name) = question.suggested_file_name {
            dialog = dialog.set_file_name(name);
        }
        for kind in question.kinds {
            dialog = dialog.add_filter(kind.name, kind.extensions);
        }
        dialog
    }
}

impl FileChooser for NativeFileChooser {
    fn choose_a_file_to_open(&self, question: &FileQuestion<'_>) -> Option<PathBuf> {
        Self::dialog_for(question).pick_file()
    }

    fn choose_where_to_save(&self, question: &FileQuestion<'_>) -> Option<PathBuf> {
        Self::dialog_for(question).save_file()
    }
}

/// Fails at once when asked anything. What an editor that is not for a person to use (a test's)
/// is given, so that reaching a dialog is a mistake that is reported instead of waited on.
pub struct RefusingFileChooser;

impl FileChooser for RefusingFileChooser {
    fn choose_a_file_to_open(&self, question: &FileQuestion<'_>) -> Option<PathBuf> {
        refuse(question)
    }

    fn choose_where_to_save(&self, question: &FileQuestion<'_>) -> Option<PathBuf> {
        refuse(question)
    }
}

fn refuse(question: &FileQuestion<'_>) -> ! {
    panic!(
        "the editor opened the file dialog \"{}\", which would wait for an answer that nobody is there to give; \
         a test must give the editor a FileChooser that answers",
        question.title
    )
}

#[cfg(test)]
pub use scripted::{AskedQuestion, ScriptedFileChooser};

#[cfg(test)]
mod scripted {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::rc::Rc;

    use super::{FileChooser, FileQuestion};

    /// A question that a [`ScriptedFileChooser`] was asked, kept as plain text for the tests to compare.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct AskedQuestion {
        /// Whether it asked where to save a file (rather than which file to open).
        pub saving: bool,
        pub title: String,
        pub suggested_file_name: Option<String>,
        /// The name of each kind of file offered, with its endings.
        pub kinds: Vec<(String, Vec<String>)>,
    }

    /// A chooser that gives the answers it was told to give, in order, and remembers what it was
    /// asked. (A clone shares the answers and the questions, so a test can keep one and give the
    /// other to the editor.) Asked more than it has answers for, it fails the test.
    #[derive(Clone, Default)]
    pub struct ScriptedFileChooser {
        answers: Rc<RefCell<VecDeque<Option<PathBuf>>>>,
        asked: Rc<RefCell<Vec<AskedQuestion>>>,
    }

    impl ScriptedFileChooser {
        /// Says what the next question that has no answer yet is to be answered with: a file, or
        /// `None` for a person who changed their mind.
        pub fn will_answer(&self, answer: Option<PathBuf>) {
            self.answers.borrow_mut().push_back(answer);
        }

        pub fn questions_asked(&self) -> Vec<AskedQuestion> {
            self.asked.borrow().clone()
        }

        fn answer(&self, saving: bool, question: &FileQuestion<'_>) -> Option<PathBuf> {
            self.asked.borrow_mut().push(AskedQuestion {
                saving,
                title: question.title.to_owned(),
                suggested_file_name: question.suggested_file_name.map(str::to_owned),
                kinds: question
                    .kinds
                    .iter()
                    .map(|kind| {
                        (
                            kind.name.to_owned(),
                            kind.extensions
                                .iter()
                                .map(|extension| (*extension).to_owned())
                                .collect(),
                        )
                    })
                    .collect(),
            });
            self.answers.borrow_mut().pop_front().unwrap_or_else(|| {
                panic!(
                    "the editor asked \"{}\", which the test had no answer for",
                    question.title
                )
            })
        }
    }

    impl FileChooser for ScriptedFileChooser {
        fn choose_a_file_to_open(&self, question: &FileQuestion<'_>) -> Option<PathBuf> {
            self.answer(false, question)
        }

        fn choose_where_to_save(&self, question: &FileQuestion<'_>) -> Option<PathBuf> {
            self.answer(true, question)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: &[FileKind<'static>] = &[
        FileKind {
            name: "Video files",
            extensions: &["mp4", "mov"],
        },
        FileKind {
            name: "All files",
            extensions: &["*"],
        },
    ];

    fn question() -> FileQuestion<'static> {
        FileQuestion {
            title: "Pick one",
            suggested_file_name: Some("name.mp4"),
            kinds: KINDS,
        }
    }

    #[test]
    #[should_panic(expected = "Pick one")]
    fn the_refusing_chooser_fails_when_asked_which_file_to_open_and_says_what_was_asked() {
        RefusingFileChooser.choose_a_file_to_open(&question());
    }

    #[test]
    #[should_panic(expected = "Pick one")]
    fn the_refusing_chooser_fails_when_asked_where_to_save_too() {
        RefusingFileChooser.choose_where_to_save(&question());
    }

    #[test]
    fn the_scripted_chooser_gives_its_answers_in_order_and_remembers_the_questions() {
        let chooser = ScriptedFileChooser::default();
        chooser.will_answer(Some(PathBuf::from("a.mp4")));
        chooser.will_answer(None);
        let shared = chooser.clone();

        assert_eq!(shared.choose_a_file_to_open(&question()), Some(PathBuf::from("a.mp4")));
        assert_eq!(shared.choose_where_to_save(&question()), None);

        let asked = chooser.questions_asked();
        assert_eq!(asked.len(), 2);
        assert!(!asked[0].saving && asked[1].saving);
        assert_eq!(asked[0].title, "Pick one");
        assert_eq!(asked[0].suggested_file_name.as_deref(), Some("name.mp4"));
        assert_eq!(
            asked[0].kinds,
            vec![
                ("Video files".to_owned(), vec!["mp4".to_owned(), "mov".to_owned()]),
                ("All files".to_owned(), vec!["*".to_owned()]),
            ]
        );
    }

    #[test]
    #[should_panic(expected = "no answer")]
    fn the_scripted_chooser_fails_when_asked_more_than_it_was_told_to_answer() {
        ScriptedFileChooser::default().choose_a_file_to_open(&question());
    }
}
