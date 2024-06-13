#!/bin/bash
ip=$(curl ifconfig.me)
inference-node --ip $ip --port 8080 --gate-server 10.148.0.4:7878 --data-server 10.148.0.3:9000 --contract-address 0x94539d83c832Eb6011A32C2E7aC2BD71fa7FC894