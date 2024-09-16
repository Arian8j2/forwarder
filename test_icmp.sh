set -e

cargo t --no-run
bin_name=$(cargo t --no-run 2>&1 | grep -oP '\(\Ktarget/debug/deps/server-.+(?=\))')
sudo setcap cap_net_admin,cap_net_raw=eip "$bin_name"
./$bin_name --nocapture --color always --ignored icmp
