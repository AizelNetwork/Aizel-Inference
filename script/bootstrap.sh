#!/bin/bash

# run face recognition
nohup bash /export/App/rank/aizel-face-recognition/bin/start.sh 2>&1 &
# nohup /python3.7/bin/python3 /export/App/rank/aizel-face-model-service/main.py > face_model_service.log 2>&1 & 
ip=$(curl ifconfig.me)
echo $ip
ntpdate ntp.aliyun.com
retrieve-secret
export PATH=$PATH:/python/bin
inference-node --ip $ip --port 8080
sleep infinity

python3 -m llama_cpp.server --model ~/aizel/models/llama2_7b_chat.Q4_0.gguf-1 --seed -1 --n_threads -1 --n_threads_batch -1 --port 6666