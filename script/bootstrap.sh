#!/bin/bash

# run face recognition
nohup java -jar /export/App/rank/app.jar --spring.profiles.active=dev > face_model_servoce_java.log 2>&1 &
nohup /python3.7/bin/python3 /export/App/rank/aizel-face-model-service/main.py > face_model_servoce.log 2>&1 & 
ip=$(curl ifconfig.me)
echo $ip
ntpdate ntp.aliyun.com
retrieve-secret
export PATH=$PATH:/python/bin
inference-node --ip $ip --port 8080
sleep infinity