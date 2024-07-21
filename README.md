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
1. Create GCP secrets in google secret manager
```shell
gcloud secrets create wallet-sk --replication-policy="automatic"
```

2. Upload secret to gcp secret manager
```shell
echo -n "647fcb49c378e22dc51a5fd43b3b76b28f00f605191ed7d419e1080854711cae" | gcloud secrets versions add wallet-sk --data-file=-
```
3. Create a service account for the confidential space
```

```


Grant the service account permission to access the secret
```shell
```

```shell
gcloud compute instances create inference-demo \
    --confidential-compute \
    --shielded-secure-boot \
    --scopes=cloud-platform \
    --zone=asia-southeast1-b \
    --image-project=confidential-space-images \
    --image-family=confidential-space-debug \
    --service-account=991449629434-compute@developer.gserviceaccount.com \
    --metadata="^~^tee-image-reference=asia-docker.pkg.dev/bionic-mercury-421809/aizel/aizel_inference:0.1.0~tee-env-CHAIN_ID=4369~tee-env-ENDPOINT=http://34.124.144.235:9944~tee-env-PRIVATE_KEY=647fcb49c378e22dc51a5fd43b3b76b28f00f605191ed7d419e1080854711cae~tee-env-CONTRACT_ADDRESS=0x5F9BAe82718B469721C6CD55D6Ab356dc5D60c5B~tee-container-log-redirect=true" \
    --machine-type=n2d-standard-16 \
    --min-cpu-platform="AMD Milan" \
    --boot-disk-size=50 \
    --tags=will-dev
```

## Deploy to AliCloud TDX Encrypted VM
1. Configure your alicloud cli

1. Prepare resources

- Create a bucket to store the image
    - `aliyun oss mb oss://aizel-tdx -c cn-beijing -L oss-cn-beijing` 
- Upload secrets and configs to oss
    - modify the content in the `tdx-data.json` 
    - upload the json file to the oss `aliyun oss cp tdx-data.json oss://aizel-tdx/tdx-data.json`
- Create a RAM role for alicloud deployment
    - `aliyun ram CreateRole --RoleName aizel-inference --Description "Role for Aizel Inference Node" --AssumeRolePolicyDocument "{\"Statement\": [{\"Action\": \"sts:AssumeRole\",\"Effect\": \"Allow\",\"Principal\": {\"Service\": [\"ecs.aliyuncs.com\"]}}],\"Version\": \"1\"}"`
    - `aliyun ram AttachPolicyToRole --PolicyName AliyunOSSReadOnlyAccess --PolicyType System --RoleName aizel-inference`
- Create a keypair for ecs. This keypair is only used to create the ecs instance and it can't be used to ssh login. Sshd service is banned on the Aizel inference.
    - aliyun ecs CreateKeyPair --KeyPairName mock-key
- Prepare your environment variable for your project
    - export VSwitchID=[your switch id]
    - export SecurityGroupId=[your security id]
