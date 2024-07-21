#/bin/bash

sudo docker build --tag asia-docker.pkg.dev/bionic-mercury-421809/aizel/aizel_inference:0.1.0 .
gcloud auth print-access-token | docker login -u oauth2accesstoken --password-stdin https://pkg.dev

docker push asia-docker.pkg.dev/bionic-mercury-421809/aizel/aizel_inference:0.1.0

# build and install linuxkit
git clone https://github.com/WillJiang1/linuxkit.git
pushd linuxkit
make
popd

INITRD_LARGE_THAN_4GiB=1
if [ $INITRD_LARGE_THAN_4GiB -eq 1 ]; then
    (cd tools/grub && docker build -f Dockerfile.rhel -t linuxkit-hack/grub .)

    if [ -z $(docker ps -f name='registry' -q) ]; then
      docker run -d -p 5000:5000 --restart=always --name registry registry:2
    fi

    (
      remote_registry="localhost:5000/"
      tag="v0.1"
      cd tools/mkimage-raw-efi-ext4/ && 
      docker build . -t ${remote_registry}mkimage-raw-efi-ext4:$tag && 
      docker push ${remote_registry}mkimage-raw-efi-ext4:$tag
    )
    image_format="raw-efi-ext4"
else
    image_format="raw-efi"
fi

# build linux kernel
(
  cd contrib/foreign-kernels && 
  docker build -f Dockerfile.rpm.anolis.5.10 . -t linuxkit/kernel:5.10-tdx
)

# build a raw-efi image
bin/linuxkit build --docker examples/aizel-inference-tdx.yml -f $image_format

aliyun ecs DeleteImage --RegionId cn-beijing --ImageId $IMAGE_ID

IMAGE_ID=$(bin/linuxkit push alibabacloud aizel-inference-tdx-efi-ext4.img --bucket aizel-tdx --access-key-id $ACCESS_KEYID --access-key-secret $ACCESS_KEYSECRET --region-id cn-beijing --nvme)

aliyun ecs RunInstances \
  --SecurityOptions.ConfidentialComputingMode TDX \
  --RegionId cn-beijing \
  --ZoneId cn-beijing-i \
  --SystemDisk.Category cloud_essd \
  --ImageId $IMAGE_ID \
  --InstanceType 'ecs.g8i.xlarge' \
  --SecurityGroupId $SecurityGroupId \
  --VSwitchId $VSwitchID \
  --KeyPairName mock-key \
  --InternetChargeType PayByTraffic \
  --InternetMaxBandwidthOut 10 \
  --RamRoleName aizel-inference


ctr -n services.linuxkit tasks exec --tty --exec-id 1 aizel-inference bash