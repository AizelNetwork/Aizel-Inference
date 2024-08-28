FROM rust:1.77.0 AS builder

RUN apt-get update && apt-get -y upgrade \
   && apt-get install -y cmake libclang-dev protobuf-compiler ca-certificates wget gnupg libssl-dev build-essential zlib1g-dev libncurses5-dev libgdbm-dev libnss3-dev libreadline-dev libffi-dev wget tar libbz2-dev liblzma-dev lzma git

RUN echo "ca_directory=/etc/ssl/certs" >> /etc/wgetrc && \
   echo 'deb [arch=amd64] https://download.01.org/intel-sgx/sgx_repo/ubuntu jammy main' | tee /etc/apt/sources.list.d/intel-sgx.list && \
   wget -qO - https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key --no-check-certificate | apt-key add -

RUN apt-get update && apt-get install -y libtdx-attest-dev libsgx-dcap-quote-verify-dev && apt-get clean && rm -rf /var/lib/apt/lists/*    

WORKDIR /python
RUN wget https://www.python.org/ftp/python/3.8.19/Python-3.8.19.tar.xz && tar -xvf Python-3.8.19.tar.xz && cd Python-3.8.19 && ./configure --enable-optimizations --with-ssl --prefix=/python && make -j $(nproc) && make install && rm -rf /python/Python-3.8.19 /python/Python-3.8.19.tar.xz
RUN /python/bin/pip3 install 'llama-cpp-python[server]'

WORKDIR /python3.7
RUN wget https://www.python.org/ftp/python/3.7.16/Python-3.7.16.tar.xz && tar -xvf Python-3.7.16.tar.xz && cd Python-3.7.16 && ./configure --enable-optimizations --with-ssl --prefix=/python3.7 && make -j $(nproc) && make install && rm -rf /python/Python-3.7.16 /python/Python-3.7.16.tar.xz
COPY ./requirements.txt /python/requirements.txt
RUN /python3.7/bin/pip3 install -r /python/requirements.txt
RUN /python3.7/bin/pip3 install dlib==19.24.0

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

FROM golang:1.22 AS builder2
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
  openjdk-8-jdk \
  && apt-get clean \
  && rm -rf /var/lib/apt/lists/*

RUN echo "ca_directory=/etc/ssl/certs" >> /etc/wgetrc && \
  echo 'deb [arch=amd64] https://download.01.org/intel-sgx/sgx_repo/ubuntu jammy main' | tee /etc/apt/sources.list.d/intel-sgx.list && \
  wget -qO - https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key --no-check-certificate | apt-key add -

RUN apt-get update && apt-get -y install libtdx-attest ntpdate strace libgomp1 && apt-get clean && rm -rf /var/lib/apt/lists/*

RUN  mkdir -p /export/App/rank/
RUN  mkdir -p /export/App/rank/aizel-face-model-service
RUN  mkdir -p /export/Logs/spring-boot-admin/
COPY ./aizel-face-model-service /export/App/rank/aizel-face-model-service
COPY ./aizel-face-recognition/target/aizel-face-recognition /export/App/rank/aizel-face-recognition 
WORKDIR /app
COPY --from=builder /app/target/release/inference-client /usr/local/bin/inference-client
COPY --from=builder /app/target/release/inference-node /usr/local/bin/inference-node
COPY --from=builder2 /app/retrieve-secret /usr/local/bin/retrieve-secret
COPY --from=builder /python /python
COPY --from=builder /python3.7 /python3.7
COPY ./script/bootstrap.sh bootstrap.sh
RUN mkdir /root/aizel
EXPOSE 8080
ENTRYPOINT ["/bin/bash", "bootstrap.sh"]