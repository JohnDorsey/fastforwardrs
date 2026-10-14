# Mutation testing by hand-written mistakes

The tests of this project are checked by planting **one deliberate mistake at a time** in the code (a `<` that
becomes `<=`, a condition that is left out, a sign that is flipped...) and running the tests: a mistake that no test
notices is a gap in the tests, and each one found so far was closed with a test (or shown to be an *equivalent* mistake,
one that changes nothing a user could see). This folder is the tool that does the planting and the running.

```text
py tools/mutation_testing/mutate.py MUTATIONS_FILE [words] [options]
```

`MUTATIONS_FILE` is a Python file that defines the mistakes:

```python
MUTATIONS = [
    # (name, file in the project, the text to change, what to change it to)
    ("edges: a clip that is cut across is not a cut", "src/project/groups/edges.rs",
     "if overlaps && !is_inside {", "if overlaps && is_inside {"),
    # A fifth item says what to give `cargo test` instead of the library's whole suite. The mistake then counts as noticed
    # only if *that* fails, and nothing else is run: it aims a mistake at one new test, or at a test program of its own.
    ("one test", "src/project/mod.rs", "...", "...", "--lib -- the_name_of_the_test"),
    ("a journey", "src/media/export.rs", "...", "...", "--test application_journeys"),
]
```

The text to change must be in the file **exactly once** (`--check` tests that for the whole file without running anything;
the text has to be what `cargo fmt` made of the code). Words after the file name pick the mistakes whose names contain one
of them. A file of mistakes may import another one to run a part of it again.

## What it prints

For each mistake, as soon as it is known: `caught`, `NOT CAUGHT`, `DID NOT COMPILE (so it says nothing)` or `SKIPPED`
(the text to change was not found once), with the first failing tests, and how long the build and the tests took. At the
end it lists what was not noticed and exits with 1 if there is any. Mistakes that make the tests hang (killed after
`--seconds-for-the-tests`, 240 by default) or crash the program of the tests count as noticed, as a test that does not
finish is a test that fails; a mistake that does not compile says nothing about the tests and is a problem.
`--results FILE` also writes one line of JSON for each, and `--only-not-noticed-in FILE` runs again what that file says
was not noticed (after a test was added for it).

## What it does to be quick

It used to run the whole suite for every mistake, and that took ~75 s for each (a rebuild of 8 s, and 53 s of tests however
soon the first one failed). Now (measured on a 6-core computer with the library's 3464 tests; the same mistakes get the same
verdicts as the old way, see "How it was checked" at the end):

* **It stops at the first failing test**, and kills the run. Nearly every mistake that is noticed is noticed within a few seconds
  (3.9 s of tests on average, after a build of 7.4 s, for the 45 that were noticed in the check at the end); only the mistakes that
  are *not* noticed need all the tests, as they always did (18 s on average). (`--thorough` runs everything for everyone, and says
  how many tests failed, as before.)
* **The tests most likely to notice come first** and the rest after them: first the tests of the module of the file with the
  mistake (a file `src/a/b.rs` is the module `a::b`, whose tests have names that begin with it), plus the tests that noticed
  the last mistakes in the same file (remembered in `learned_tests.json` in the work folder); only if they all pass are *all
  the other tests* run. The two runs are every test exactly once, so the verdict is the whole suite's.
* **The program of the tests is built without debugging information** (`CARGO_PROFILE_DEV_DEBUG=0` for the work copies
  only): the rebuild after a changed line is 6.4 s instead of 8.1 s, and a target folder is 3 GB instead of 12. The checks of the
  debug profile (arithmetic overflow, debug assertions) are all still on. (Other things measured and not used: a different linker
  made no difference, 6.1 s, and `line-tables-only` gave 7.0 s. What is left of the 6.4 s is the compiler: expanding the macros of
  the crate is 1.5 s, linking 1.4 s, code generation 0.8 s, and many things of 0.3 s or less.)
* **Several mistakes at once** (`--workers`, 3 by default), each in a **work copy** of the project with a target folder of its
  own (in the temporary files, `MUTATION_WORK_FOLDER` or `--work-folder`; `--clean` deletes them). So the project itself is never
  changed (an interrupted run cannot leave a mistake in it), and the project can be changed meanwhile: the copies are made the
  same as the project, file by file, when a run begins. A file that is written gets the time of now so that cargo builds it
  again; a file is never put back with an older time.
* **Processes are killed as a tree, by their own pid**, never by the name of a program (the old runner killed every `cargo.exe`
  and `fastforwardrs.exe` when a mistake hung, which would have taken your own `cargo run` window with it).
* **The suite itself is quicker**, which helps every mistake that is not noticed, as those need all the tests: the slowest tests
  were ones that checked many cases one after another (every cut of a project file, the arithmetic of dragging clips against brute
  force, drawing random projects). They now check the same cases at the same time (`test_support::check_every_case_at_once`), and
  the whole library suite takes 26 s and not 53 s.

## What it does so that quick does not mean wrong

* **Two controls come first** (`--no-controls` skips them, for re-running one or two mistakes): a test that always fails is put in
  the library, and must be noticed *by that test*; a test that always passes is put in it, and must not be noticed (this run goes
  through every test, so it is also the proof that all the project's tests pass when nothing is wrong, which "noticed" rests
  on). If either does not do what it should, nothing is run and the runner says so.
* **A test that fails has to fail again, by itself** (`--trust-every-failure` skips it): the moment a test fails, the run is
  stopped, and the tests that failed are run once more, alone, with the mistake still in the program (it is built already, so this
  takes a second or two). Only a test that fails again is what noticed the mistake. One that passes the second time failed
  because the computer was busy (a campaign makes it busy; a test of the real speakers, that expects them to keep to the time to
  within an eighth of a second, did so for one mistake of every six when three workers and a build were going), so it is *left out*,
  the run goes on without it, and the verdict is made by the other tests; the results say which tests were left out
  (`dismissed_failures`). Without this a failing test that has nothing to do with the mistake makes the mistake look noticed, which is
  the one way that a campaign can hide a gap.
* **A final check** (`--no-final-check` skips it): every test that was seen to fail is run again on the project *without any
  mistake*, and must pass. A test that fails by itself (broken, or too dependent on the time or the load of the computer) would
  otherwise make every mistake look noticed.
* **The runner is itself tested**: `py -m unittest discover -s tools/mutation_testing` runs `test_mutation_core.py` (what the
  runner decides from text: planting, which tests first, what the output of cargo means: failed, hung, crashed, did not compile)
  and `test_mutate_end_to_end.py`, which runs the runner with the real cargo on a tiny project that has a slow passing test, so that
  the time a verdict took shows whether the run stopped early or went through everything (about a minute).

## Things to know

* A work copy needs a first full build (a minute for the dependencies, then the project).
* Run mistakes that hang or crash *with the others* now: killing is by tree, so nothing else is disturbed.
* Windows is where this was made and tried. (`taskkill` is used there and `os.killpg` elsewhere, which has not been tried; the
  copies are made with plain Python, so that is the same everywhere.)
* The tests that read files of the project (the README's first JSON block, say) see the copy's files, so the README of the
  project must not be mid-edit when a run begins.

## How it was checked

* **The same verdicts as the old way.** The old way (the whole suite for every mistake, one at a time, a mistake noticed if any test
  failed) and this tool were run on the same 49 mistakes: those of four older sets, two that change nothing, and one that does not
  compile. All 49 verdicts were the same (45 noticed, 3 not noticed, 1 that did not compile). The old way took 56 minutes, with the
  computer shared with other work (75 s each when measured alone). This took, with nothing else running, 650 s with one worker
  (13 s for each mistake) and 315 s with three (6.4 s for each), and gave the same 49 verdicts both times.
* **It was used on about 550 different mistakes** (six campaigns over the code of one round of work). It found tests that could not
  fail (a setup that never reached the code it was written for, an assertion that compared a value with itself, which hid a real
  bug), code that nothing needed, and, as it was first made, a flaw of its own: tests that fail by themselves when the computer is
  busy made nine of the first fifty-five verdicts of one campaign false. That is why a test that fails has to fail again by itself,
  and why there is a final check.
