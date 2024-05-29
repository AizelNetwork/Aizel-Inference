#!/bin/bash
ip=$(curl ifconfig.me)
inference-node --ip $ip --port 8080 --gate-server 10.148.0.4:7878 --data-server 10.148.0.3:9000