#!/bin/bash -x

# current directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

main() {
    pushd ${SCRIPT_DIR}
    
    # create tun device and change state to 'up'.
    sudo ip tuntap add name tun0 mode tun
    sudo ip link set tun0 up
    
    # save routing table before modifying it.
    sudo iptables-save > iptables.bak
    
    # route everything through tun device.
    sudo ip route add 128.0.0.0/1 dev tun0
    sudo ip route add 0.0.0.0/1 dev tun0

    popd
}

main
