#!/bin/bash
# Runs a command on a copy of the project in the Linux file system of WSL (building on /mnt/c is slow), for testing the program on Linux
# from a Windows checkout. Run it from Windows with
#
#     wsl -d Ubuntu -- bash tools/linux/run_in_wsl.sh cargo test --lib
#
# (the first time, Ubuntu needs a Rust toolchain and the libraries of .github/workflows/ci.yml, "System libraries (Ubuntu)", plus ffmpeg).
# The copy is made fresh each time, without `target` (which stays in the copy, so builds are incremental) and without `.git`.
set -e
source ~/.cargo/env
here=$(cd "$(dirname "$0")/../.." && pwd)
rsync -a --delete --exclude target --exclude .git "$here/" ~/fastforwardrs/
cd ~/fastforwardrs
"$@"
