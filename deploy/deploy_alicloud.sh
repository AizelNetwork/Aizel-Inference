#/bin/bash

sudo docker build --tag asia-docker.pkg.dev/bionic-mercury-421809/aizel/aizel_inference:0.1.0 .
gcloud auth print-access-token | docker login -u oauth2accesstoken --password-stdin https://pkg.dev

docker push asia-docker.pkg.dev/bionic-mercury-421809/aizel/aizel_inference:0.1.0