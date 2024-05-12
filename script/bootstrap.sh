#!/bin/bash
threads=$(nproc)
llama-server -m models/llama-2-7b.Q4_K_M.gguf -c 2048 > llama-server.log -t $threads 2>&1 &
inference-server