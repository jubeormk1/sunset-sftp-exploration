#!/bin/bash
# This script has been borrowed from the [embassy-rs examples for std](https://github.com/embassy-rs/embassy/blob/main/examples/std/tap.sh) 
ip tuntap add name tap99 mode tap user $SUDO_USER group $SUDO_USER
ip link set tap99 up
ip addr add 192.168.69.100/24 dev tap99
ip route add 192.168.69.0/24 dev tap99 # Added to explicitly create the route to this broadcast domain to the tap
ip -6 addr add fe80::100/64 dev tap99
ip -6 addr add fdaa::100/64 dev tap99
ip -6 route add fe80::/64 dev tap99
ip -6 route add fdaa::/64 dev tap99
