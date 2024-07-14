FROM rust:1.77.0 AS builder

RUN apt-get update && apt-get -y upgrade \
   && apt-get install -y cmake libclang-dev protobuf-compiler

RUN git clone https://github.com/intel/SGXDataCenterAttestationPrimitives.git && cd SGXDataCenterAttestationPrimitives/QuoteGeneration/quote_wrapper/tdx_attest/linux && make && cp libtdx_attest.so /usr/local/lib/ && cp ../tdx_attest.h /usr/local/include/

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
  && apt-get clean \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/inference-client /usr/local/bin/inference-client
COPY --from=builder /app/target/release/inference-node /usr/local/bin/inference-node
COPY --from=builder2 /app/retrieve-secret /usr/local/bin/retrieve-secret
COPY ./script/bootstrap.sh bootstrap.sh
LABEL "tee.launch_policy.allow_env_override"="ENDPOINT,CHAIN_ID,PRIVATE_KEY,CONTRACT_ADDRESS"
EXPOSE 8080
ENTRYPOINT /bin/bash bootstrap.sh