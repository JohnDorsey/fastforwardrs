#!/bin/bash
# The spikes on Linux, on the software Vulkan driver (lavapipe) ONLY: VK_DRIVER_FILES keeps the loader from opening any other driver (in WSL there is also a
# driver for the real GPU). Every program runs under `timeout -s KILL`, which kills exactly the one child it started.
source ~/.cargo/env
export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
export PATH="$HOME/ffmpeg9/bin:$PATH"
cd ~/spikes
mkdir -p out_linux
echo "=== adapters";            timeout -s KILL 30 ./target/debug/adapters
echo "=== race_repro";          timeout -s KILL 60 ./target/debug/race_repro 2>&1 | head -12
echo "=== s1_isf";              timeout -s KILL 120 ./target/debug/s1_isf out_linux shaders 2>&1 | grep -E "^\[|CHECK|adapter" | cut -c1-210
echo "=== s3_lut";             timeout -s KILL 280 ./target/debug/s3_lut out_linux/s3 2>&1 | grep -E "adapter|===|--|GPU \(32|GPU \(8-bit target\) vs ffmpeg|hardware trilinear vs CPU|IGNORED" | cut -c1-230
