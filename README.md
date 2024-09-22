## Forwarder
Lightweight UDP forwarder and UDP over ICMP software  
Mostly used it to combat [DPI](https://en.wikipedia.org/wiki/Deep_packet_inspection) when using popular VPN protocols such as **Wireguard** and **OpenVPN**

### Usage
Simply forwarding single local port to another:
```bash
forwarder -l 0.0.0.0:1001 -r 127.0.0.1:1002
```
---
Forwarding UDP and encrypting packets via XOR encryption  
(*i'm using forwarder on top of other protocols such as Wireguard that already has strong encryption so the xor is only for confusing DPI*):
```sh
forwarder -l 0.0.0.0:1001 -r 127.0.0.1:1050 -p some_secret
```
now, packets delivered to port `1050` are also encrypted via a secret so you need also another forwarder to decrypt packets and forward it to the actual port (1002):
```sh
forwarder -l 127.0.0.1:1050 -r 127.0.0.1:1002 -p some_secret
```
![Screenshot_2024-01-19_1705682795](https://github.com/Arian8j2/forwarder/assets/56799194/09433d44-48bc-4a27-a7ab-19bd5990a9b6)
---
Forwarding UDP packets over ICMP:
```sh
forwarder -l 0.0.0.0:1001/udp -r 127.0.0.1:1050/icmp
forwarder -l 127.0.0.1:1050/icmp -r 127.0.0.1:1002/udp
```
![Screenshot_2024-01-19_1705683004](https://github.com/Arian8j2/forwarder/assets/56799194/bafe0681-abec-48cb-8ea7-1651d983c9e6)
> [!WARNING]
> UDP over ICMP currently may not work behind NAT or NAPT, because forwarder doesn't try to simulate real icmp handshake (request, reply) and only sends echo request and also the sequence and id of icmp packet is used as source and destination port to avoid further MTU issues, also i'm using forwarder only on servers so the main reason for this behavior is that.
