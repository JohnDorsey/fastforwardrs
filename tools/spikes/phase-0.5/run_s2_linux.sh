#!/bin/bash
# S2 on Linux, lavapipe ONLY. Starts the program in the background, samples its CPU time every 2 s, and kills exactly that process id at the hard limit.
source ~/.cargo/env
export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
cd ~/spikes
limit=${HARD_LIMIT:-70}
echo "##### s2_hang $* (hard limit $limit s)"
S2_LINGER_SECONDS=${S2_LINGER_SECONDS:-0} ./target/debug/s2_hang "$@" > s2_linux.log 2> s2_linux.err &
pid=$!
start=$SECONDS
while kill -0 $pid 2>/dev/null; do
    if [ $((SECONDS - start)) -ge "$limit" ]; then
        echo "HARD LIMIT: killing pid $pid"
        kill -9 $pid
        break
    fi
    echo "  t=$((SECONDS - start))s cpu=$(ps -o cputime= -p $pid 2>/dev/null | tr -d ' ')"
    sleep 2
done
cat s2_linux.log
head -5 s2_linux.err
