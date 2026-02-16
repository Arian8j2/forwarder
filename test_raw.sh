#!/bin/bash

set -e

cargo t --no-run
bin_name=$(cargo t --no-run 2>&1 | grep -oP '\(\Ktarget/debug/deps/server-.+(?=\))')
sudo setcap cap_net_admin,cap_net_raw=eip "$bin_name"

echo 1 | sudo tee /proc/sys/net/ipv4/icmp_echo_ignore_all &>/dev/null
echo 1 | sudo tee /proc/sys/net/ipv6/icmp/echo_ignore_all &>/dev/null

./$bin_name --nocapture --color always --ignored test_raw || echo

echo 0 | sudo tee /proc/sys/net/ipv4/icmp_echo_ignore_all &>/dev/null
echo 0 | sudo tee /proc/sys/net/ipv6/icmp/echo_ignore_all &>/dev/null
