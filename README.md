# AizelInference

## Install Dependencies
Ubuntu:
```
sudo apt update && sudo apt upgrade -y
sudo apt install -y protobuf-compiler libprotobuf-dev
```
MacOs
```
brew install protobuf
```

## Build 
```
cargo build
```

## Deploy to Google Confidential Space
```
gcloud compute instances create inference-demo2 \
    --confidential-compute \
    --shielded-secure-boot \
    --scopes=cloud-platform \
    --zone=us-west1-b \
    --image-project=confidential-space-images \
    --image-family=confidential-space-debug \
    --service-account=991449629434-compute@developer.gserviceaccount.com \
    --metadata="^~^tee-image-reference=asia-docker.pkg.dev/bionic-mercury-421809/aizel/aizel_inference:0.1.0~tee-env-CHAIN_ID=Sepolia~tee-env-ENDPOINT=https://sepolia.infura.io/v3/4edd867b9c3c4999bac4b3aaea9842b7~tee-env-PRIVATE_KEY=647fcb49c378e22dc51a5fd43b3b76b28f00f605191ed7d419e1080854711cae~tee-env-CONTRACT_ADDRESS=0x94539d83c832Eb6011A32C2E7aC2BD71fa7FC894~tee-container-log-redirect=true" \
    --machine-type=n2d-standard-4 \
    --min-cpu-platform="AMD Milan" \
    --boot-disk-size=50 \
    --tags=will-dev
```