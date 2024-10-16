#/bin/bash
sudo docker build --tag asia-docker.pkg.dev/bionic-mercury-421809/aizel/aizel_inference:0.1.0 .
gcloud auth print-access-token | docker login -u oauth2accesstoken --password-stdin https://asia-docker.pkg.dev

docker push asia-docker.pkg.dev/bionic-mercury-421809/aizel/aizel_inference:0.1.0
gcloud compute instances delete inference-demo --zone asia-southeast1-b --quiet || true
gcloud compute instances create inference-demo \
    --confidential-compute \
    --shielded-secure-boot \
    --scopes=cloud-platform \
    --zone=asia-southeast1-b \
    --image-project=confidential-space-images \
    --image-family=confidential-space-debug \
    --service-account=991449629434-compute@developer.gserviceaccount.com \
    --metadata="^~^tee-image-reference=asia-docker.pkg.dev/bionic-mercury-421809/aizel/aizel_inference:0.0.1~tee-container-log-redirect=true~tee-env-CONFIG_NAME='aizel-config'" \
    --machine-type=n2d-standard-16 \
    --min-cpu-platform="AMD Milan" \
    --boot-disk-size=50 \
    --tags=will-dev