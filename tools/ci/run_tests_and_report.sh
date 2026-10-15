#!/bin/bash
# Runs a command (the arguments after the first two), keeps what it prints in a file, and when it fails writes the names of the tests that failed and what they said as an
# ANNOTATION of the GitHub job. Annotations can be read without signing in to GitHub (through the public API: /repos/OWNER/REPO/check-runs/ID/annotations); the log cannot.
#
#     tools/ci/run_tests_and_report.sh error   "Tests that failed"         cargo test --no-fail-fast
#     tools/ci/run_tests_and_report.sh warning "Real-time tests that failed" cargo test ...
#
# With `error` the script fails like the command did. With `warning` it reports the failures as a warning and succeeds: for the tests that measure real time (sound
# without a gap, a picture on time), which a shared machine with a few slow processors cannot be asked to pass.
level=$1
title=$2
shift 2
log="${RUNNER_TEMP:-/tmp}/report_$$.log"
set -o pipefail
if "$@" 2>&1 | tee "$log"; then
    exit 0
fi
failed=$(grep -E '^test .* FAILED$' "$log" | head -n 40)
said=$(grep -A8 -E '^---- .* stdout ----$' "$log" | cut -c1-300 | head -n 60)
message=$(printf '%s\n\n%s' "$failed" "$said" | head -c 3500)
message="${message//'%'/'%25'}"
message="${message//$'\r'/}"
message="${message//$'\n'/'%0A'}"
echo "::$level title=$title::$message"
if [ "$level" = "warning" ]; then
    exit 0
fi
exit 1
