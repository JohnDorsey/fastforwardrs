# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Checks of the mutation runner against the real cargo, on a tiny project made for the purpose (it has a test that passes but is slow, so that
the time a verdict took shows whether the run stopped early or went on through all the tests). Run them with
    py -m unittest discover -s tools/mutation_testing
(from the folder of the project). They take about a minute."""

import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import textwrap
import time
import unittest
import unittest.mock

import mutate
import mutation_core as core
from mutation_core import Mutation

SECONDS_THE_SLOW_TEST_TAKES = 6

FILES = {
    "Cargo.toml": """
        [package]
        name = "fixture"
        version = "0.1.0"
        edition = "2021"

        [dependencies]
        """,
    "src/lib.rs": """
        pub mod alpha;
        pub mod beta;

        #[cfg(test)]
        mod gamma_tests;

        #[cfg(test)]
        mod test_support;
        """,
    "src/test_support.rs": """
        // (nothing: only so that the root of the library has the lines that the controls of the runner look for)
        """,
    "src/alpha.rs": """
        pub fn add(a: i32, b: i32) -> i32 {
            a + b
        }

        #[cfg(test)]
        mod tests {
            use super::add;

            #[test]
            fn adds() {
                assert_eq!(add(2, 3), 5);
            }
        }
        """,
    "src/beta.rs": """
        pub fn twice(a: i32) -> i32 {
            a * 2
        }
        """,
    "src/gamma_tests.rs": f"""
        use crate::alpha::add;
        use crate::beta::twice;

        #[test]
        fn twice_doubles() {{
            assert_eq!(twice(4), 8);
        }}

        #[test]
        fn add_is_symmetric() {{
            assert_eq!(add(1, 2), add(2, 1));
        }}

        // Passes, and takes its time, and its name sorts last: a run that goes through everything takes this long, a run that stops
        // at the first failure does not.
        #[test]
        fn zzz_slow_but_passing() {{
            std::thread::sleep(std::time::Duration::from_secs({SECONDS_THE_SLOW_TEST_TAKES}));
        }}
        """,
}


def write_the_fixture(folder: pathlib.Path, extra_files: dict | None = None) -> None:
    for name, text in {**FILES, **(extra_files or {})}.items():
        path = folder / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(textwrap.dedent(text).lstrip("\n"), encoding="utf-8", newline="\n")


ADD = "a + b"
TWICE = "a * 2"


def mistake(name, path, old, new, cargo_arguments=None):
    return Mutation(name, path, old, new, cargo_arguments)


@unittest.skipUnless(shutil.which("cargo") or (pathlib.Path.home() / ".cargo" / "bin").exists(), "cargo is needed")
class EndToEndTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.base = pathlib.Path(tempfile.mkdtemp(prefix="mutation-runner-test-"))
        cls.project = cls.base / "project"
        write_the_fixture(cls.project)
        cls.work = cls.base / "work"

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.base, ignore_errors=True)

    def campaign(self, workers=1, **options):
        said = []
        campaign = mutate.Campaign(self.project, self.work, workers, mutate.Options(**options), say=said.append)
        campaign.said = said
        campaign.learned_tests = {}  # (what an earlier test learned is not what this one is about)
        campaign.prepare_the_copies()
        return campaign

    def try_one(self, mutation, **options):
        campaign = self.campaign(**options)
        trial = mutate.try_mutation(campaign.copies[0], mutation, campaign.options, campaign._learned_for(mutation.path))
        # Whatever happened, the file is as it was.
        self.assertEqual(
            campaign.copies[0].read(mutation.path),
            (self.project / mutation.path).read_bytes().decode("utf-8"),
            "the file was not put back",
        )
        return trial

    # ---- verdicts ------------------------------------------------------------------------------------------------------------

    def test_a_mistake_that_a_test_of_its_own_module_notices_is_caught_in_the_first_stage_and_the_slow_test_is_never_waited_for(self):
        trial = self.try_one(mistake("add becomes subtract", "src/alpha.rs", ADD, "a - b"))
        self.assertEqual(trial.outcome.verdict, core.CAUGHT)
        self.assertEqual(trial.outcome.failing_tests, ("alpha::tests::adds",))
        self.assertEqual(trial.deciding_stage, "the tests of its module")
        self.assertLess(trial.seconds_testing, SECONDS_THE_SLOW_TEST_TAKES - 3)

    def test_a_mistake_that_only_a_test_of_another_module_notices_is_caught_in_the_second_stage_and_still_stops_early(self):
        trial = self.try_one(mistake("twice becomes thrice", "src/beta.rs", TWICE, "a * 3"))
        self.assertEqual(trial.outcome.verdict, core.CAUGHT)
        self.assertEqual(trial.outcome.failing_tests, ("gamma_tests::twice_doubles",))
        self.assertEqual(trial.deciding_stage, "all the other tests")
        self.assertLess(trial.seconds_testing, SECONDS_THE_SLOW_TEST_TAKES - 3)

    def test_a_mistake_that_changes_nothing_is_not_caught_and_all_the_tests_were_run_to_say_so(self):
        trial = self.try_one(mistake("add is written the other way round", "src/alpha.rs", ADD, "b + a"))
        self.assertEqual(trial.outcome.verdict, core.NOT_CAUGHT)
        self.assertFalse(trial.outcome.is_noticed)
        self.assertGreaterEqual(trial.seconds_testing, SECONDS_THE_SLOW_TEST_TAKES - 1)
        # Both stages said "ok", and together they ran every test of the project once: 1 (adds) in the first, 3 in the second.
        self.assertRegex(trial.outcome.detail, r"test result: ok\. 1 passed.*test result: ok\. 3 passed")

    def test_thorough_runs_every_test_whatever_fails_and_says_how_many_failed(self):
        trial = self.try_one(
            mistake("twice becomes thrice", "src/beta.rs", TWICE, "a * 3"), in_stages=False, stop_at_the_first_failure=False
        )
        self.assertEqual(trial.outcome.verdict, core.CAUGHT)
        self.assertRegex(trial.outcome.detail, r"test result: FAILED\. 3 passed; 1 failed")
        self.assertGreaterEqual(trial.seconds_testing, SECONDS_THE_SLOW_TEST_TAKES - 1)

    # ---- a test that fails because the computer is busy ----------------------------------------------------------------------

    def a_project_with_a_test_that_fails_the_first_time_it_is_run(self, name, real_failure_is_slow=False):
        """The project of these tests with one more: a test that fails the first time it is run in a copy of the project, and never after
        (as a test that depends on the time does, when the computer is busy at that moment). The tests are run side by side, so which of two
        that fail is seen to fail first is not certain: with `real_failure_is_slow` the one that fails because of a mistake in `twice` takes a
        second to do it, so that it is certain that the one that fails once is seen first."""
        project = self.base / name
        slow_to_fail = FILES["src/gamma_tests.rs"].replace(
            "fn twice_doubles() {\n", "fn twice_doubles() {\n    std::thread::sleep(std::time::Duration::from_secs(1));\n"
        )
        assert slow_to_fail != FILES["src/gamma_tests.rs"]
        slow = {"src/gamma_tests.rs": slow_to_fail} if real_failure_is_slow else {}
        write_the_fixture(
            project,
            {
                **slow,
                "src/lib.rs": FILES["src/lib.rs"].replace("mod gamma_tests;", "mod flaky_tests;\n\n        #[cfg(test)]\n        mod gamma_tests;"),
                "src/flaky_tests.rs": """
                    // Fails the first time it is run in a folder, and passes in every run after that one.
                    #[test]
                    fn fails_once() {
                        let marker = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("it_has_failed_once");
                        if !marker.exists() {
                            std::fs::write(&marker, "yes").unwrap();
                            panic!("the computer was busy");
                        }
                    }
                """,
            },
        )
        return project

    def try_one_in(self, project, work_name, mutation, **options):
        campaign = mutate.Campaign(project, self.base / work_name, 1, mutate.Options(**options), say=(said := []).append)
        campaign.learned_tests = {}
        campaign.prepare_the_copies()
        trial = mutate.try_mutation(campaign.copies[0], mutation, campaign.options, [])
        trial.said = said
        return trial

    def test_a_test_that_fails_once_and_passes_when_it_is_run_by_itself_does_not_notice_a_mistake_and_the_rest_of_the_tests_are_run_without_it(self):
        project = self.a_project_with_a_test_that_fails_the_first_time_it_is_run("flaky_project_1")
        trial = self.try_one_in(project, "flaky_work_1", mistake("add is written the other way round", "src/alpha.rs", ADD, "b + a"))
        self.assertEqual(trial.outcome.verdict, core.NOT_CAUGHT)
        self.assertEqual(trial.dismissed_failures, ["flaky_tests::fails_once"])
        # The rest of the tests of that stage were run, and passed (the three of gamma_tests, the slow one among them).
        self.assertRegex(trial.outcome.detail, r"test result: ok\. 3 passed")
        self.assertGreaterEqual(trial.seconds_testing, SECONDS_THE_SLOW_TEST_TAKES - 1)
        self.assertIn("left out", mutate.describe(0, 1, trial))

    def test_a_test_that_fails_once_is_left_out_and_a_test_that_really_fails_with_the_mistake_is_what_noticed_it(self):
        project = self.a_project_with_a_test_that_fails_the_first_time_it_is_run("flaky_project_2", real_failure_is_slow=True)
        trial = self.try_one_in(project, "flaky_work_2", mistake("twice becomes thrice", "src/beta.rs", TWICE, "a * 3"))
        self.assertEqual(trial.outcome.verdict, core.CAUGHT)
        self.assertEqual(trial.outcome.failing_tests, ("gamma_tests::twice_doubles",))
        self.assertEqual(trial.dismissed_failures, ["flaky_tests::fails_once"])
        self.assertEqual(mutate.results_as_json(0, trial)["dismissed_failures"], ["flaky_tests::fails_once"])

    def test_with_the_confirmation_off_a_test_that_fails_once_counts_as_noticing_the_mistake(self):
        project = self.a_project_with_a_test_that_fails_the_first_time_it_is_run("flaky_project_3")
        trial = self.try_one_in(
            project, "flaky_work_3", mistake("add is written the other way round", "src/alpha.rs", ADD, "b + a"), confirm_failures=False
        )
        self.assertEqual(trial.outcome.verdict, core.CAUGHT)
        self.assertEqual(trial.outcome.failing_tests, ("flaky_tests::fails_once",))
        self.assertEqual(trial.dismissed_failures, [])

    def test_a_test_that_fails_every_time_is_confirmed_and_costs_one_more_short_run(self):
        trial = self.try_one(mistake("add becomes subtract", "src/alpha.rs", ADD, "a - b"))
        self.assertEqual(trial.outcome.verdict, core.CAUGHT)
        self.assertEqual(trial.outcome.failing_tests, ("alpha::tests::adds",))
        self.assertEqual(trial.dismissed_failures, [])
        self.assertLess(trial.seconds_testing, SECONDS_THE_SLOW_TEST_TAKES - 3)

    def test_a_mistake_that_does_not_compile_says_nothing(self):
        trial = self.try_one(mistake("a name that is not there", "src/alpha.rs", ADD, "a + c"))
        self.assertEqual(trial.outcome.verdict, core.DID_NOT_COMPILE)
        self.assertFalse(trial.outcome.is_noticed)
        self.assertIn("cannot find value `c`", trial.outcome.detail)

    def test_a_mistake_that_hangs_the_tests_is_killed_after_the_time_allowed_and_counts_as_noticed(self):
        started = time.monotonic()
        trial = self.try_one(mistake("twice never ends", "src/beta.rs", TWICE, "loop {}"), seconds_allowed_for_the_tests=4)
        self.assertEqual(trial.outcome.verdict, core.HUNG)
        self.assertTrue(trial.outcome.is_noticed)
        self.assertLess(time.monotonic() - started, 60)
        # Nothing of the hung run is left to hold the program of the tests (a program that runs cannot be written again).
        followed_by = self.try_one(mistake("add becomes subtract", "src/alpha.rs", ADD, "a - b"))
        self.assertEqual(followed_by.outcome.verdict, core.CAUGHT)

    def test_a_mistake_that_makes_the_program_of_the_tests_crash_is_noticed(self):
        trial = self.try_one(mistake("twice aborts", "src/beta.rs", TWICE, "std::process::abort()"))
        self.assertEqual(trial.outcome.verdict, core.CRASHED)
        self.assertTrue(trial.outcome.is_noticed)

    def test_a_lower_priority_is_asked_for_every_run_when_it_is_wanted_and_for_none_when_it_is_not(self):
        flags_of_the_runs = []
        real_popen = subprocess.Popen

        def popen_that_notes_its_flags(*arguments, **keywords):
            if arguments[0][0] == "cargo":  # (not the taskkill that ends it)
                flags_of_the_runs.append(keywords.get("creationflags", 0))
            return real_popen(*arguments, **keywords)

        for wanted in (True, False):
            flags_of_the_runs.clear()
            with unittest.mock.patch.object(mutate.subprocess, "Popen", popen_that_notes_its_flags):
                trial = self.try_one(mistake("add becomes subtract", "src/alpha.rs", ADD, "a - b"), low_priority=wanted)
            self.assertEqual(trial.outcome.verdict, core.CAUGHT, "it works the same")
            self.assertTrue(flags_of_the_runs, "cargo was started")
            if os.name == "nt":
                expected = subprocess.BELOW_NORMAL_PRIORITY_CLASS if wanted else 0
                self.assertEqual(set(flags_of_the_runs), {expected})

    def test_an_aimed_mistake_is_noticed_only_by_what_it_is_aimed_at(self):
        # The test that notices this mistake is in the other module and is not what was asked for: so it is not noticed.
        not_aimed_at_it = self.try_one(mistake("twice becomes thrice", "src/beta.rs", TWICE, "a * 3", "--lib -- alpha::tests::adds"))
        self.assertEqual(not_aimed_at_it.outcome.verdict, core.NOT_CAUGHT)
        self.assertLess(not_aimed_at_it.seconds_testing, SECONDS_THE_SLOW_TEST_TAKES - 3, "only the one test was run")
        aimed_at_it = self.try_one(mistake("twice becomes thrice", "src/beta.rs", TWICE, "a * 3", "--lib -- twice_doubles"))
        self.assertEqual(aimed_at_it.outcome.verdict, core.CAUGHT)
        self.assertEqual(aimed_at_it.deciding_stage, "aimed")

    def test_a_text_that_is_not_in_the_file_exactly_once_is_not_planted_and_the_file_is_not_touched(self):
        for old in ["a text that is not there", "a"]:
            trial = self.try_one(mistake("nothing to change", "src/alpha.rs", old, "z"))
            self.assertIsNone(trial.outcome)
            self.assertIn("not once", trial.problem)

    # ---- campaigns -----------------------------------------------------------------------------------------------------------

    def test_several_workers_give_the_verdicts_of_each_mistake_in_the_order_of_the_mistakes_and_report_each_as_it_is_known(self):
        mistakes = [
            mistake("add becomes subtract", "src/alpha.rs", ADD, "a - b"),
            mistake("twice becomes thrice", "src/beta.rs", TWICE, "a * 3"),
            mistake("add is written the other way round", "src/alpha.rs", ADD, "b + a"),
            mistake("a name that is not there", "src/alpha.rs", ADD, "a + c"),
        ]
        campaign = self.campaign(workers=2)
        reported = []
        trials = campaign.run(mistakes, lambda index, trial: reported.append(index))
        self.assertEqual(sorted(reported), [0, 1, 2, 3])
        self.assertEqual(
            [trial.outcome.verdict for trial in trials],
            [core.CAUGHT, core.CAUGHT, core.NOT_CAUGHT, core.DID_NOT_COMPILE],
        )
        # The tests that noticed a mistake in a file are remembered, to be run first for the next mistake in it.
        self.assertEqual(campaign.learned_tests["src/alpha.rs"], ["alpha::tests::adds"])
        self.assertEqual(campaign.learned_tests["src/beta.rs"], ["gamma_tests::twice_doubles"])
        # And so a second mistake in beta.rs is decided by the first stage already.
        again = self.try_one(mistake("twice becomes four times", "src/beta.rs", TWICE, "a * 4"))
        first_stage = core.stages_for(again.mutation, campaign.learned_tests["src/beta.rs"])[0]
        self.assertIn("gamma_tests::twice_doubles", first_stage.cargo_arguments)

    def test_many_mistakes_in_a_row_that_are_stopped_early_are_all_built_and_none_is_in_the_way_of_the_next(self):
        """The program of the tests that is killed at the first failure is gone a moment later, and the next build writes where it was: it failed
        to link now and then (LNK1104) before the runner waited for the program to be gone."""
        replacements = ["a - b", "a * b", "a / b", "a % b", "a ^ b", "a | b", "a & b", "a + b + 1", "a - 1", "b", "a", "0"]
        mistakes = [mistake(f"add becomes {new}", "src/alpha.rs", ADD, new) for new in replacements]
        campaign = self.campaign(workers=1)
        trials = campaign.run(mistakes, lambda index, trial: None)
        self.assertEqual(
            [trial.outcome.verdict for trial in trials],
            [core.CAUGHT] * len(mistakes),
            [(trial.mutation.name, trial.outcome.detail[:200]) for trial in trials if trial.outcome.verdict != core.CAUGHT],
        )

    def test_what_was_learned_is_used_for_the_first_stage_so_a_mistake_in_another_module_is_decided_in_it(self):
        learned = ["gamma_tests::twice_doubles"]
        campaign = self.campaign()
        trial = mutate.try_mutation(campaign.copies[0], mistake("twice becomes thrice", "src/beta.rs", TWICE, "a * 3"), campaign.options, learned)
        self.assertEqual(trial.outcome.verdict, core.CAUGHT)
        self.assertEqual(trial.deciding_stage, "the tests of its module")

    def test_the_controls_pass_on_a_project_whose_tests_pass(self):
        campaign = self.campaign()
        self.assertTrue(campaign.check_the_controls(), campaign.said)
        self.assertTrue(
            any("controls behave" in line and "aaa_mutation_controls::fails_on_purpose" in line for line in campaign.said), campaign.said
        )

    def test_the_controls_fail_when_the_tests_of_the_project_do_not_pass_without_any_mistake(self):
        broken = self.base / "broken_project"
        write_the_fixture(broken, {"src/alpha.rs": FILES["src/alpha.rs"].replace("assert_eq!(add(2, 3), 5);", "assert_eq!(add(2, 3), 6);")})
        campaign = mutate.Campaign(broken, self.base / "broken_work", 1, mutate.Options(), say=(said := []).append)
        campaign.prepare_the_copies()
        self.assertFalse(campaign.check_the_controls())
        self.assertTrue(any("CONTROLS FAILED" in line for line in said), said)

    def test_the_final_check_passes_when_the_tests_that_noticed_pass_without_the_mistakes_and_fails_when_one_does_not(self):
        campaign = self.campaign()
        caught = mutate.Trial(
            mistake("add becomes subtract", "src/alpha.rs", ADD, "a - b"),
            core.Outcome(core.CAUGHT, ("alpha::tests::adds",), "x"),
        )
        self.assertTrue(campaign.check_that_what_noticed_passes_without_the_mistakes([caught]), campaign.said)
        # A test that does not exist cannot be run: the check says so instead of passing.
        ghost = mutate.Trial(
            mistake("add becomes subtract", "src/alpha.rs", ADD, "a - b"),
            core.Outcome(core.CAUGHT, ("alpha::tests::does_not_exist",), "x"),
        )
        self.assertFalse(campaign.check_that_what_noticed_passes_without_the_mistakes([ghost]))
        # And one that fails with nothing planted is not trustworthy.
        broken = self.base / "broken_project_2"
        write_the_fixture(broken, {"src/alpha.rs": FILES["src/alpha.rs"].replace("assert_eq!(add(2, 3), 5);", "assert_eq!(add(2, 3), 6);")})
        broken_campaign = mutate.Campaign(broken, self.base / "broken_work_2", 1, mutate.Options(), say=(said := []).append)
        broken_campaign.prepare_the_copies()
        self.assertFalse(broken_campaign.check_that_what_noticed_passes_without_the_mistakes([caught]))
        self.assertTrue(any("UNRELIABLE" in line for line in said), said)

    # ---- the work copies -----------------------------------------------------------------------------------------------------

    def test_a_copy_has_the_files_of_the_project_and_nothing_else_and_is_updated_in_place(self):
        project = self.base / "sync_project"
        write_the_fixture(project)
        for left_out in ["target", ".git", "screenshots", "tools"]:
            (project / left_out).mkdir()
            (project / left_out / "file.txt").write_text("not for the copies")
        copy = mutate.WorkCopy(self.base / "sync_copy")
        self.assertEqual(copy.synchronise_with(project), len(FILES))
        self.assertEqual(sorted(p.name for p in copy.folder.iterdir()), ["Cargo.toml", "src"])
        self.assertEqual(copy.synchronise_with(project), 0, "nothing differs: nothing is written")

        (project / "src" / "beta.rs").write_text("pub fn twice(a: i32) -> i32 {\n    a * 5\n}\n", encoding="utf-8", newline="\n")
        (project / "src" / "gamma_tests.rs").unlink()
        (project / "src" / "new_file.rs").write_text("// new\n", encoding="utf-8", newline="\n")
        copy.write("src/alpha.rs", "// a mistake that was left in the copy\n")
        self.assertEqual(copy.synchronise_with(project), 3, "beta.rs, new_file.rs and alpha.rs (which was changed in the copy)")
        self.assertFalse((copy.folder / "src" / "gamma_tests.rs").exists(), "a file that is gone from the project is gone from the copy")
        self.assertEqual(copy.read("src/alpha.rs"), (project / "src" / "alpha.rs").read_bytes().decode("utf-8"))
        self.assertEqual(copy.read("src/beta.rs"), "pub fn twice(a: i32) -> i32 {\n    a * 5\n}\n")

    # ---- the command line ----------------------------------------------------------------------------------------------------

    def test_the_command_line_runs_a_file_of_mistakes_and_writes_the_results_and_says_whether_all_were_noticed(self):
        mutations_file = self.base / "mutations.py"
        mutations_file.write_text(
            textwrap.dedent(
                """
                MUTATIONS = [
                    ("add becomes subtract", "src/alpha.rs", "a + b", "a - b"),
                    ("add is written the other way round", "src/alpha.rs", "a + b", "b + a"),
                ]
                """
            ),
            encoding="utf-8",
        )
        results = self.base / "results.jsonl"
        code = mutate.main(
            [str(mutations_file), "--project", str(self.project), "--work-folder", str(self.work), "--results", str(results), "--workers", "2"]
        )
        self.assertEqual(code, 1, "one mistake was not noticed")
        entries = sorted((json.loads(line) for line in results.read_text(encoding="utf-8").splitlines()), key=lambda e: e["index"])
        self.assertEqual([e["verdict"] for e in entries], [core.CAUGHT, core.NOT_CAUGHT])
        self.assertEqual(entries[0]["failing_tests"], ["alpha::tests::adds"])
        # Only what was not noticed is run again when asked.
        self.assertEqual(mutate.main([str(mutations_file), "--only-not-noticed-in", str(results), "--list"]), 0)
        self.assertEqual(
            mutate.main([str(mutations_file), "--project", str(self.project), "--check"]), 0, "both texts to change are in the project once"
        )


if __name__ == "__main__":
    unittest.main()
