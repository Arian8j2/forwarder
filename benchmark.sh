#!/bin/bash

set -e

PROTOCOL=${1:-udp}

cargo b --release
bin_name=./target/release/forwarder

if [ "$PROTOCOL" = "icmp" ]; then
    # icmp uses raw socket and raw socket needs permission
    sudo setcap cap_net_admin,cap_net_raw=eip $bin_name

    # disable automatic icmp reply messages for better and accurate performance
    DEFAULT_ICMP_IGNORE=$(cat /proc/sys/net/ipv4/icmp_echo_ignore_all)
    sudo sh -c "echo 1 > /proc/sys/net/ipv4/icmp_echo_ignore_all"
fi

run_forwarder() {
    taskset --cpu-list 0,1 $bin_name $@
}

run_forwarder -l 127.0.0.1:3536/udp -r 127.0.0.1:4546/$PROTOCOL &
run_forwarder -l 127.0.0.1:4546/$PROTOCOL -r 127.0.0.1:3939/udp &

# we use old iperf because it can run UDP server
iperf -s -p 3939 -u &
iperf_server=$!

sleep 1
iperf -c 127.0.0.1 -p 3536 -u -b 1G -i 1 -e

on_close() {
    if [ "$PROTOCOL" = "icmp" ]; then
        sudo sh -c "echo $DEFAULT_ICMP_IGNORE > /proc/sys/net/ipv4/icmp_echo_ignore_all"
    fi
    kill -9 -$$
}
trap on_close SIGINT SIGTERM EXIT
