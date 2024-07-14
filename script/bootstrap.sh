#!/bin/bash
retrieve-secret
ip=$(curl ifconfig.me)
inference-node --ip $ip --port 8080