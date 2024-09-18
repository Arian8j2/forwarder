#!/bin/bash

set -e

cargo b -p forwarder-bench --release
protocol="$1" # either 'udp' or 'icmp'

# run the benchmark with max scheduling priority
sudo nice -n -20 \
    ./target/release/forwarder-bench $protocol
