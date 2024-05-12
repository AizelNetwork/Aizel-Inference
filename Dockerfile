FROM rust:1.77.0 AS builder

RUN apt-get update && apt-get -y upgrade \
   && apt-get install -y cmake libclang-dev protobuf-compiler

RUN USER=root cargo new --bin app
WORKDIR /app
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml
COPY ./proto ./proto
COPY ./src ./src
COPY ./build.rs ./build.rs

RUN cargo build --release

COPY ./llama.cpp ./llama.cpp
RUN cd llama.cpp && make server

FROM ubuntu:22.04
RUN apt-get update && apt-get -y upgrade && apt-get install -y --no-install-recommends \
  libssl-dev \
  curl \
  ca-certificates \
  && apt-get clean \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/inference-client /usr/local/bin/inference-client
COPY --from=builder /app/target/release/inference-server /usr/local/bin/inference-server
COPY --from=builder /app/llama.cpp/server /usr/local/bin/llama-server
COPY ./models /app/models
COPY ./script/bootstrap.sh bootstrap.sh

ENTRYPOINT /bin/bash bootstrap.sh