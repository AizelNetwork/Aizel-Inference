#!/bin/bash
ip=$(curl ifconfig.me)
echo $ip
ntpdate ntp.aliyun.com
retrieve-secret
inference-node --ip $ip --port 8080
sleep infinity