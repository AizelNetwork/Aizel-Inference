#!/bin/bash

# run face recognition
nohup bash /export/App/rank/aizel-face-recognition/bin/start.sh 2>&1 &
nohup /python3.7/bin/python3 /export/App/rank/aizel-face-model-service/main.py > face_model_service.log 2>&1 & 
ip=$(curl ifconfig.me)
echo $ip
ntpdate ntp.aliyun.com
retrieve-secret
export PATH=$PATH:/python/bin
inference-node --ip $ip --port 8080
sleep infinity