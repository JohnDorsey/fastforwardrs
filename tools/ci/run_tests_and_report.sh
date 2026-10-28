#!/bin/bash
# Runs a command (the arguments after the first two), keeps what it prints in a file, and when it fails writes the names of the tests that failed and what they said as an
# ANNOTATION of the GitHub job. Annotations can be read without signing in to GitHub (through the public API: /repos/OWNER/REPO/check-runs/ID/annotations); the log cannot.
#
#     tools/ci/run_tests_and_report.sh error   "Tests that failed"         cargo test --no-fail-fast
#     tools/ci/run_tests_and_report.sh warning "Real-time tests that failed" cargo test ...
#
# With `error` the script fails like the command did. With `warning` it reports the failures as a warning and succeeds: for the tests that measure real time (sound
# without a gap, a picture on time), which a shared machine with a few slow processors cannot be asked to pass.
#
# Whatever the outcome it also writes what ran as a NOTICE: the "test result" line of each program of tests, and how many lines say SKIPPED. (A test that is skipped passes,
# and the only way to tell it from one that ran is that it says so; the tests print `SKIPPED: why`, which cargo shows only when they are run with `-- --nocapture`. Without
# it the lines are not there to be counted, and the notice says so instead of a count of nought.)
level=$1
title=$2
shift 2
log="${RUNNER_TEMP:-/tmp}/report_$$.log"
set -o pipefail

# Whether the command keeps the output of the tests that pass (cargo shows it only with one of these two).
case " $* " in
    *" --nocapture "* | *" --show-output "*) output_of_passing_tests_is_kept=yes ;;
    *) output_of_passing_tests_is_kept=no ;;
esac

# Writes the message $1 as an annotation of the kind $2 with the title $3.
annotate() {
    local message="$1"
    message="${message//'%'/'%25'}"
    message="${message//$'\r'/}"
    message="${message//$'\n'/'%0A'}"
    echo "::$2 title=$3::$message"
}

what_ran() {
    local summary skipped
    summary=$(grep -E '^test result:' "$log" | sed -E 's/; 0 measured; 0 filtered out//' | head -n 30)
    if [ "$output_of_passing_tests_is_kept" = yes ]; then
        skipped=$(grep -c 'SKIPPED' "$log")
    else
        skipped="not counted, because cargo does not show what the tests that pass print (--nocapture does)"
    fi
    annotate "$(printf '%s\nlines that say SKIPPED: %s' "$summary" "$skipped" | head -c 3000)" notice "$title (what ran)"
}

if "$@" 2>&1 | tee "$log"; then
    what_ran
    exit 0
fi
what_ran
failed=$(grep -E '^test .* FAILED$' "$log" | head -n 40)
said=$(grep -A8 -E '^---- .* stdout ----$' "$log" | cut -c1-300 | head -n 60)
# (When no test is named as failed, the command itself ended badly: a program of tests killed or aborted, a build that failed. Its last lines say how.)
if [ -z "$failed" ] && [ -z "$said" ]; then
    failed="no test is named as failed; what the command printed about how it ended (and its last lines):"
    said=$( { grep -n -B3 -A6 -E "process didn't exit|signal:|overflowed its stack|fatal runtime error|SIGSEGV|SIGABRT|SIGKILL|Killed|Caused by|out of memory|memory allocation|error: test failed" "$log" | head -n 40; echo '--- the last lines:'; tail -n 15 "$log"; } | cut -c1-300)
fi
annotate "$(printf '%s\n\n%s' "$failed" "$said" | head -c 3500)" "$level" "$title"
if [ "$level" = "warning" ]; then
    exit 0
fi
exit 1
