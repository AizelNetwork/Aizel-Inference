FROM rust:1.77.0 AS builder

RUN apt-get update && apt-get -y upgrade \
   && apt-get install -y cmake libclang-dev protobuf-compiler ca-certificates wget gnupg

RUN echo "ca_directory=/etc/ssl/certs" >> /etc/wgetrc && \
   echo 'deb [arch=amd64] https://download.01.org/intel-sgx/sgx_repo/ubuntu jammy main' | tee /etc/apt/sources.list.d/intel-sgx.list && \
   wget -qO - https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key --no-check-certificate | apt-key add -

RUN apt-get update && apt-get install -y libtdx-attest-dev libsgx-dcap-quote-verify-dev && apt-get clean && rm -rf /var/lib/apt/lists/*    

WORKDIR /python
RUN wget https://www.python.org/ftp/python/3.8.19/Python-3.8.19.tar.xz && tar -xvf Python-3.8.19.tar.xz && cd Python-3.8.19 && ./configure --enable-optimizations && make -j $(nproc) && make install && pip3 install llama-cpp-python 'llama-cpp-python[server]' 

RUN USER=root cargo new --bin app
WORKDIR /app
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml
COPY ./proto ./proto
COPY ./src ./src
COPY ./build.rs ./build.rs
COPY ./common ./common
COPY ./verifier ./verifier
COPY ./tdx ./tdx

RUN cargo build --release

FROM golang:1.22 as builder2
WORKDIR /app
COPY ./secret ./secret
RUN cd secret && go build -o ../retrieve-secret

FROM ubuntu:22.04

RUN apt-get update && apt-get -y upgrade && apt-get install -y --no-install-recommends \
  libssl-dev \
  curl \
  ca-certificates \ 
  wget \ 
  gnupg \
  && apt-get clean \
  && rm -rf /var/lib/apt/lists/*

RUN echo "ca_directory=/etc/ssl/certs" >> /etc/wgetrc && \
  echo 'deb [arch=amd64] https://download.01.org/intel-sgx/sgx_repo/ubuntu jammy main' | tee /etc/apt/sources.list.d/intel-sgx.list && \
  wget -qO - https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key --no-check-certificate | apt-key add -

RUN apt-get update && apt-get -y install libtdx-attest ntpdate strace && apt-get clean && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/inference-client /usr/local/bin/inference-client
COPY --from=builder /app/target/release/inference-node /usr/local/bin/inference-node
COPY --from=builder2 /app/retrieve-secret /usr/local/bin/retrieve-secret
COPY ./script/bootstrap.sh bootstrap.sh
LABEL "tee.launch_policy.allow_env_override"="ENDPOINT,CHAIN_ID,PRIVATE_KEY,INFERENCE_CONTRACT,INFERENCE_REGISTRY_CONTRACT,DATA_REGISTRY_CONTRACT,DATA_NODE_ID,INITAIL_STAKE_AMOUNT"
EXPOSE 8080
ENTRYPOINT /bin/bash bootstrap.sh