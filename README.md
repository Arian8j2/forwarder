## Forwarder
Lightweight udp forwarder, Forward udp packets via udp, icmp sockets to another port or host  
Mostly used it to combat [DPI](https://en.wikipedia.org/wiki/Deep_packet_inspection)

### Usage
Simply forwarding single local port to another:
```bash
forwarder -l 0.0.0.0:1001 -r 127.0.0.1:1002
```
---
Forwarding udp and encrypting packets via xor encryption:
```sh
forwarder -l 0.0.0.0:1001 -r 127.0.0.1:1050 -p some_secret
```
now, packets delivered to port `1050` are also encrypted via a secret so you need also another forwarder to decrypt packets and forwarder it to actual port (1002):
```sh
forwarder -l 127.0.0.1:1050 -r 127.0.0.1:1002 -p some_secret
```
![Screenshot_2024-01-19_1705682795](https://github.com/Arian8j2/forwarder/assets/56799194/09433d44-48bc-4a27-a7ab-19bd5990a9b6)
---
Forwarding udp packets via icmp:
```sh
forwarder -l 0.0.0.0:1001/udp -r 127.0.0.1:1050/icmp
forwarder -l 127.0.0.1:1050/icmp -r 127.0.0.1:1002/udp
```
![Screenshot_2024-01-19_1705683004](https://github.com/Arian8j2/forwarder/assets/56799194/bafe0681-abec-48cb-8ea7-1651d983c9e6)
