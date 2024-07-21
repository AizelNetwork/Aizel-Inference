#!/bin/bash
ip=$(curl ifconfig.me)
echo $ip
retrieve-secret
inference-node --ip $ip --port 8080
sleep infinity