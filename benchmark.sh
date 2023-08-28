#!/bin/bash

set -e

cargo b --release
./target/release/forwarder -l 127.0.0.1:3536 -r 127.0.0.1:8080 &
forwarder_pid=$!

nice -n 15 bash -c "while true; do sleep 1 && export cpu_usage=\$(ps -ef -o pid,pcpu | grep $forwarder_pid | awk '{print \$2}') && echo \"------> cpu usage forwarder: \$cpu_usage\"; done" &
cpu_usage_reporter_pid=$!

goben &
goben_sv_pid=$!

sleep 1
goben -hosts 127.0.0.1:3536 -udp

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT
# trap "kill -9 $goben_sv_pid $forwarder_pid $cpu_usage_reporter_pid" SIGINT SIGTERM EXIT
