#!/bin/bash 
cd aizel-face-recognition
echo -e "Building Aziel Face Recognition Build Image ..."
docker build -t aizel-face-recognition-build:0.0.1 -f Dockerfile.build .
echo -e "Compiling ..."
docker run --rm --name aizel_face_recognition_build -v ./:/root/aizel-face-recognition aizel-face-recognition-build:0.0.1 sh -c "cd /root/aizel-face-recognition && /usr/local/maven/bin/mvn clean install -D maven.test.skip=true"

echo -e "Building Aizel Inference Image ..."
sudo docker build --tag asia-docker.pkg.dev/bionic-mercury-421809/aizel/aizel_inference:0.1.0 .