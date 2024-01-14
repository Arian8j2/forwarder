bin_name=$(cargo t --no-run 2>&1 | grep -oP 'target/debug/deps/integration_test\S+(?=\))')
sudo setcap cap_net_admin,cap_net_raw=eip target/debug/forwarder
./$bin_name --nocapture --color always --ignored icmp
