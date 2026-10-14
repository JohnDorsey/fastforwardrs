"""Checks of what the mutation runner decides without starting a program. Run them with
    py -m unittest discover -s tools/mutation_testing
(from the folder of the project)."""

import pathlib
import tempfile
import textwrap
import unittest

import mutation_core as core
from mutation_core import Mutation, Stage


def mutation(path="src/a.rs", old="x + 1", new="x - 1", cargo_arguments=None):
    return Mutation("a mistake", path, old, new, cargo_arguments)


class PlantingTests(unittest.TestCase):
    def test_the_text_is_changed_once_where_it_is_found_once(self):
        self.assertEqual(core.plant("let y = x + 1;", mutation()), "let y = x - 1;")

    def test_a_text_that_is_not_there_or_is_there_twice_is_not_planted(self):
        self.assertIsNone(core.plant("let y = x;", mutation()))
        self.assertIsNone(core.plant("x + 1; x + 1;", mutation()))
        self.assertEqual(core.how_many_times_the_text_to_change_is_in("x + 1; x + 1;", mutation()), 2)
        self.assertEqual(core.how_many_times_the_text_to_change_is_in("nothing", mutation()), 0)

    def test_a_text_of_several_lines_is_found_in_a_file_with_windows_line_endings_and_keeps_them(self):
        multi = mutation(old="if a {\n    b\n}", new="if !a {\n    b\n}")
        self.assertEqual(core.plant("if a {\r\n    b\r\n}\r\n", multi), "if !a {\r\n    b\r\n}\r\n")
        self.assertEqual(core.plant("if a {\n    b\n}\n", multi), "if !a {\n    b\n}\n")
        self.assertEqual(core.how_many_times_the_text_to_change_is_in("if a {\r\n    b\r\n}\r\n", multi), 1)

    def test_only_the_first_place_is_changed_and_nothing_else_is(self):
        text = "head\nx + 1\ntail\n"
        self.assertEqual(core.plant(text, mutation()), "head\nx - 1\ntail\n")


class LoadingTests(unittest.TestCase):
    def write(self, folder, name, text):
        path = pathlib.Path(folder) / name
        path.write_text(textwrap.dedent(text), encoding="utf-8")
        return path

    def test_mutations_are_read_with_and_without_the_fifth_item(self):
        with tempfile.TemporaryDirectory() as folder:
            path = self.write(
                folder,
                "set.py",
                """
                MUTATIONS = [
                    ("one", "src/a.rs", "x", "y"),
                    ("two", "src/b.rs", "p", "q", "--test journeys"),
                ]
                """,
            )
            self.assertEqual(
                core.load_mutations(path),
                [Mutation("one", "src/a.rs", "x", "y"), Mutation("two", "src/b.rs", "p", "q", "--test journeys")],
            )

    def test_a_file_can_use_the_mutations_of_another(self):
        with tempfile.TemporaryDirectory() as folder:
            self.write(folder, "first.py", 'MUTATIONS = [("one", "src/a.rs", "x", "y"), ("two", "src/a.rs", "p", "q")]\n')
            second = self.write(
                folder,
                "second.py",
                """
                import importlib.util, os
                here = os.path.dirname(os.path.abspath(__file__))
                specification = importlib.util.spec_from_file_location("first", os.path.join(here, "first.py"))
                first = importlib.util.module_from_spec(specification)
                specification.loader.exec_module(first)
                MUTATIONS = first.MUTATIONS[1:]
                """,
            )
            self.assertEqual([m.name for m in core.load_mutations(second)], ["two"])

    def test_a_mutation_that_is_not_what_it_should_be_is_refused_by_name(self):
        with tempfile.TemporaryDirectory() as folder:
            path = self.write(folder, "bad.py", 'MUTATIONS = [("one", "src/a.rs", "x")]\n')
            with self.assertRaisesRegex(ValueError, "mutation number 0"):
                core.load_mutations(path)


class ModulePathTests(unittest.TestCase):
    def test_files_are_the_modules_that_name_the_tests_in_them(self):
        for path, module in [
            ("src/application/clip_moving.rs", "application::clip_moving"),
            ("src/application/clip_moving/group_tests.rs", "application::clip_moving::group_tests"),
            ("src/project/groups/mod.rs", "project::groups"),
            ("src/project/mod.rs", "project"),
            ("src/atomic_file.rs", "atomic_file"),
        ]:
            self.assertEqual(core.module_path_of(path), module, path)

    def test_files_that_are_not_a_module_of_the_library_have_none(self):
        for path in ["src/lib.rs", "src/main.rs", "src/bin/tool.rs", "tests/journeys.rs", "Cargo.toml", "README.md", "src/notes.txt"]:
            self.assertIsNone(core.module_path_of(path), path)


class StageTests(unittest.TestCase):
    def test_a_mistake_in_a_module_is_tried_with_the_tests_of_the_module_first_and_then_with_all_the_others(self):
        self.assertEqual(
            core.stages_for(mutation("src/project/groups/edges.rs")),
            [
                Stage("the tests of its module", ("--lib", "--", "project::groups::edges")),
                Stage("all the other tests", ("--lib", "--", "--skip", "project::groups::edges")),
            ],
        )

    def test_the_tests_that_noticed_other_mistakes_in_the_file_are_run_first_too_and_skipped_afterwards(self):
        stages = core.stages_for(mutation("src/project/groups/edges.rs"), ["a::b::test_one", "a::c::test_two", "project::groups::edges"])
        self.assertEqual(
            stages,
            [
                Stage("the tests of its module", ("--lib", "--", "project::groups::edges", "a::b::test_one", "a::c::test_two")),
                Stage(
                    "all the other tests",
                    ("--lib", "--", "--skip", "project::groups::edges", "--skip", "a::b::test_one", "--skip", "a::c::test_two"),
                ),
            ],
        )

    def test_a_file_that_is_no_module_is_tried_with_all_the_tests_at_once(self):
        self.assertEqual(core.stages_for(mutation("src/lib.rs")), [Stage("all", ("--lib",))])

    def test_without_stages_it_is_all_the_tests_at_once(self):
        self.assertEqual(core.stages_for(mutation("src/project/mod.rs"), in_stages=False), [Stage("all", ("--lib",))])

    def test_a_mistake_that_is_aimed_is_run_as_asked_and_nothing_more(self):
        self.assertEqual(
            core.stages_for(mutation("src/project/mod.rs", cargo_arguments="--lib -- name_one name_two"), ["learned"]),
            [Stage("aimed", ("--lib", "--", "name_one", "name_two"))],
        )

    def test_the_two_stages_are_every_test_exactly_once(self):
        """What libtest does with filters (a test is run if its name contains one of the words, and is not skipped if it contains
        one of the skipped), applied to made-up names: the first stage and the second are a partition."""
        names = ["project::groups::edges::tests::a", "project::groups::edges_more::b", "application::x::c", "project::groups::d", "e"]
        first, second = core.stages_for(mutation("src/project/groups/edges.rs"), ["application::x::c"])

        def run(stage):
            words = [word for word in stage.cargo_arguments[2:] if word not in ("--skip",)]
            if "--skip" in stage.cargo_arguments:
                return [name for name in names if not any(word in name for word in words)]
            return [name for name in names if any(word in name for word in words)]

        self.assertEqual(sorted(run(first) + run(second)), sorted(names))
        self.assertEqual(set(run(first)) & set(run(second)), set())


class ArgumentsToRunExactlyTests(unittest.TestCase):
    def test_the_whole_suite_of_the_library(self):
        self.assertEqual(core.arguments_to_run_exactly(None, ["a::t", "b::t"]), ("--lib", "--", "--exact", "a::t", "b::t"))

    def test_the_program_and_the_flags_of_an_aimed_mistake_are_kept_and_its_names_to_filter_by_are_not(self):
        self.assertEqual(
            core.arguments_to_run_exactly("--test application_journeys", ["t"]),
            ("--test", "application_journeys", "--", "--exact", "t"),
        )
        self.assertEqual(
            core.arguments_to_run_exactly("--lib name_before -- name_after --ignored", ["t"]),
            ("--lib", "--", "--ignored", "--exact", "t"),
        )
        self.assertEqual(core.arguments_to_run_exactly("--lib -- some_name", ["t"]), ("--lib", "--", "--exact", "t"))


class ArgumentsLeavingOutTests(unittest.TestCase):
    def test_nothing_to_leave_out_is_what_was_asked_for(self):
        self.assertEqual(core.arguments_leaving_out(("--lib",), []), ("--lib",))
        self.assertEqual(core.arguments_leaving_out(("--lib", "--", "a"), []), ("--lib", "--", "a"))

    def test_the_tests_are_skipped_by_their_whole_names_after_the_dashes_that_are_put_where_they_are_missing(self):
        self.assertEqual(core.arguments_leaving_out(("--lib",), ["a::t", "b::t"]), ("--lib", "--", "--skip", "a::t", "--skip", "b::t"))

    def test_the_names_to_filter_by_and_the_skips_that_are_there_are_kept_and_a_name_that_is_given_twice_is_skipped_once(self):
        self.assertEqual(
            core.arguments_leaving_out(("--lib", "--", "module", "--skip", "other"), ["t", "t"]),
            ("--lib", "--", "module", "--skip", "other", "--skip", "t"),
        )


GOOD = [
    "   Compiling fastforwardrs v0.1.0 (C:\\work)",
    "    Finished `test` profile [unoptimized] target(s) in 7.23s",
    "     Running unittests src\\lib.rs (target\\debug\\deps\\fastforwardrs-0123456789abcdef.exe)",
    "",
    "running 3 tests",
    "test a::one ... ok",
    "test a::two ... ok",
    "test a::three ... ignored",
    "",
    "test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s",
]


class ClassifyTests(unittest.TestCase):
    def test_all_tests_pass_is_not_caught(self):
        outcome = core.classify(GOOD, 0, core.FINISHED)
        self.assertEqual(outcome.verdict, core.NOT_CAUGHT)
        self.assertEqual(outcome.detail, GOOD[-1])
        self.assertFalse(outcome.is_noticed)

    def test_a_failing_test_is_caught_and_named(self):
        lines = [*GOOD[:6], "test b::broken ... FAILED", "test c::also_broken ... FAILED", "", "test result: FAILED. 1 passed; 2 failed; 0 ignored"]
        outcome = core.classify(lines, 101, core.FINISHED)
        self.assertEqual(outcome.verdict, core.CAUGHT)
        self.assertEqual(outcome.failing_tests, ("b::broken", "c::also_broken"))
        self.assertTrue(outcome.is_noticed)

    def test_a_run_that_was_stopped_at_the_first_failure_has_no_summary_and_is_caught(self):
        lines = [*GOOD[:6], "test b::broken ... FAILED"]
        outcome = core.classify(lines, 1, core.STOPPED_AT_THE_FIRST_FAILURE)
        self.assertEqual((outcome.verdict, outcome.failing_tests), (core.CAUGHT, ("b::broken",)))

    def test_a_failing_test_wins_over_a_run_that_took_too_long(self):
        outcome = core.classify([*GOOD[:6], "test b::broken ... FAILED"], 1, core.TOOK_TOO_LONG)
        self.assertEqual(outcome.verdict, core.CAUGHT)

    def test_a_run_that_took_too_long_with_no_failure_is_hung_and_noticed(self):
        outcome = core.classify(GOOD[:7], None, core.TOOK_TOO_LONG)
        self.assertEqual(outcome.verdict, core.HUNG)
        self.assertTrue(outcome.is_noticed)

    def test_a_program_that_stops_with_no_result_is_a_crash_and_noticed(self):
        lines = [
            *GOOD[:6],
            "error: test failed, to rerun pass `--lib`",
            "Caused by:",
            "  process didn't exit successfully: `target\\debug\\deps\\fastforwardrs.exe` (exit code: 0xc00000fd, STATUS_STACK_OVERFLOW)",
        ]
        outcome = core.classify(lines, 101, core.FINISHED)
        self.assertEqual(outcome.verdict, core.CRASHED)
        self.assertIn("STATUS_STACK_OVERFLOW", outcome.detail)
        self.assertTrue(outcome.is_noticed)

    def test_a_program_that_was_not_built_says_nothing(self):
        lines = [
            "   Compiling fastforwardrs v0.1.0 (C:\\work)",
            "error[E0308]: mismatched types",
            " --> src\\a.rs:3:5",
            "error: could not compile `fastforwardrs` (lib test) due to 1 previous error",
        ]
        outcome = core.classify(lines, 101, core.FINISHED)
        self.assertEqual(outcome.verdict, core.DID_NOT_COMPILE)
        self.assertFalse(outcome.is_noticed)
        self.assertIn("E0308", outcome.detail)

    def test_what_is_said_about_a_build_that_did_not_work_begins_at_its_first_error_not_at_the_summary_of_them(self):
        lines = [
            "   Compiling x",
            "warning: unused variable",
            "  |",
            "error[E0425]: cannot find value `c` in this scope",
            " --> src/a.rs:2:9",
            *["  | line"] * 30,
            "error: could not compile `x`",
        ]
        detail = core.classify(lines, 101, core.FINISHED).detail
        self.assertTrue(detail.startswith("error[E0425]: cannot find value `c`"), detail)
        self.assertEqual(len(detail.splitlines()), 12)

    def test_a_link_error_before_any_test_ran_says_nothing_either(self):
        lines = ["   Compiling x", "error: linking with `link.exe` failed: exit code: 1104", "error: could not compile `x`"]
        self.assertEqual(core.classify(lines, 101, core.FINISHED).verdict, core.DID_NOT_COMPILE)

    def test_a_failure_line_with_other_text_in_front_of_it_is_still_seen(self):
        # (a test that prints to the real stdout from a thread of its own can put something in front of the word "test")
        outcome = core.classify([*GOOD[:5], "noise test b::broken ... FAILED"], 1, core.FINISHED)
        self.assertEqual(outcome.failing_tests, ("b::broken",))

    def test_the_word_failed_alone_is_not_a_failure(self):
        lines = [*GOOD[:5], "test a::checks_what_happens_when_it_FAILED_to_save ... ok", *GOOD[-1:]]
        self.assertEqual(core.classify(lines, 0, core.FINISHED).verdict, core.NOT_CAUGHT)

    def test_the_summary_of_a_failed_run_without_a_named_test_is_still_caught(self):
        lines = [*GOOD[:5], "test result: FAILED. 0 passed; 1 failed"]
        self.assertEqual(core.classify(lines, 101, core.FINISHED).verdict, core.CAUGHT)

    def test_failing_tests_are_named_once_in_the_order_they_were_seen(self):
        lines = ["test b ... FAILED", "test a ... FAILED", "test b ... FAILED"]
        self.assertEqual(core.failing_tests_in(lines), ["b", "a"])

    def test_a_program_that_could_not_be_started_is_not_a_crash_and_says_nothing(self):
        lines = [
            *GOOD[:3],
            "error: could not execute process `target\\debug\\deps\\fastforwardrs-0123456789abcdef.exe` (never executed)",
            "Caused by:",
            "  Access is denied. (os error 5)",
        ]
        outcome = core.classify(lines, 101, core.FINISHED)
        self.assertEqual(outcome.verdict, core.DID_NOT_COMPILE)
        self.assertFalse(outcome.is_noticed)

    def test_the_program_that_cargo_says_it_starts(self):
        self.assertEqual(
            core.program_started_by("     Running unittests src\\lib.rs (target\\debug\\deps\\fastforwardrs-0123456789abcdef.exe)"),
            "target\\debug\\deps\\fastforwardrs-0123456789abcdef.exe",
        )
        self.assertEqual(
            core.program_started_by("     Running tests/journeys.rs (target/debug/deps/journeys-0123456789abcdef)"),
            "target/debug/deps/journeys-0123456789abcdef",
        )
        self.assertIsNone(core.program_started_by("   Compiling fastforwardrs v0.1.0 (C:\\work)"))
        self.assertIsNone(core.program_started_by("test a::b ... ok"))

    def test_what_is_printed_when_a_file_is_held_by_another_program_is_known(self):
        for line in [
            "error: linking with `link.exe` failed: exit code: 1104",
            "  = note: LINK : fatal error LNK1104: cannot open file 'C:\\work\\target\\debug\\deps\\x.exe'",
            "error: failed to remove file `C:\\work\\target\\debug\\deps\\x.exe`: Access is denied. (os error 5)",
            "  The process cannot access the file because it is being used by another process. (os error 32)",
            "error: Text file busy (os error 26)",
        ]:
            self.assertTrue(core.is_a_file_being_in_the_way([line]), line)
        self.assertFalse(core.is_a_file_being_in_the_way(["error[E0425]: cannot find value `c` in this scope"]))
        self.assertFalse(core.is_a_file_being_in_the_way(["error: could not compile `x` (lib test) due to 1 previous error"]))

    def test_the_line_where_the_tests_start(self):
        self.assertTrue(core.is_the_line_where_the_tests_start("     Running unittests src\\lib.rs (target\\debug\\deps\\x.exe)"))
        self.assertTrue(core.is_the_line_where_the_tests_start("     Running tests\\journeys.rs (target\\debug\\deps\\journeys-1.exe)"))
        self.assertFalse(core.is_the_line_where_the_tests_start("   Compiling x"))
        self.assertFalse(core.is_the_line_where_the_tests_start("test running ... ok"))


if __name__ == "__main__":
    unittest.main()
