#!/bin/bash
# Runs the whole test suite (library and integration tests) on the Linux copy, with the ffmpeg 9 in ~/ffmpeg9 first on the path, because the
# program is only correct with ffmpeg 9 (see the README, "ffmpeg"). Use from Windows:
#
#     wsl -d Ubuntu --cd <project folder> -- bash tools/linux/run_in_wsl.sh bash tools/linux/test_with_ffmpeg9.sh
set -e
rm -rf target/test_fixtures
export PATH="$HOME/ffmpeg9/bin:$PATH"
ffmpeg -version | head -n 1
FASTFORWARDRS_REQUIRE_FFMPEG=1 cargo test --no-fail-fast "$@"
