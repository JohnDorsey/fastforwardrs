#!/bin/bash
# The loop cap on Linux, lavapipe ONLY; each program under `timeout -s KILL` (kills exactly the process it started).
source ~/.cargo/env
export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
cd ~/spikes
for case in "infinite_while 20000" "huge_for 20000" "nested_1000_x_1000 20000"; do
    echo "== $case"
    timeout -s KILL 60 ./target/debug/s2_guard $case 2>&1 | tail -2
done
for shader in gaussian_blur kaleidoscope plasma; do
    timeout -s KILL 60 ./target/debug/s2_guard $shader none > /dev/null 2>&1
    timeout -s KILL 60 ./target/debug/s2_guard $shader 20000 > /dev/null 2>&1
    cmp out_s2_guard_${shader}_none.png out_s2_guard_${shader}_20000.png && echo "$shader: identical with and without the cap"
done
