#!/bin/bash

set -e

cargo b --release
taskset --cpu-list 0,1 ./target/release/forwarder -l 127.0.0.1:3536 -r 127.0.0.1:4546 &
taskset --cpu-list 0,1 ./target/release/forwarder -l 127.0.0.1:4546 -r 127.0.0.1:3939 &

# we use old iperf because it can run UDP server
iperf -s -p 3939 -u &
iperf_server=$!

sleep 1
iperf -c 127.0.0.1 -p 3536 -u -b 1G -i 1 -e

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT
