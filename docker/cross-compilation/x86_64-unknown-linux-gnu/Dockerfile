FROM ubuntu:latest

# Install basic environment dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    binutils \
    ca-certificates \
    cmake \
    curl \
    gcc \
    libc6-dev

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --verbose
ENV PATH=/root/.cargo/bin:$PATH
RUN rustc -vV && cargo -V

# Compile and install OpenSSL for arm
COPY openssl.sh /
RUN bash /openssl.sh linux-x86_64

# Install common witnet-rust dependencies
RUN apt-get install -y --no-install-recommends \
    clang \
    libssl-dev \
    protobuf-compiler

# Clean up apt packages so the docker image is as compact as possible
RUN apt-get clean && apt-get autoremove

# Set needed environment variables
ENV OPENSSL_DIR=/openssl \
    OPENSSL_INCLUDE_DIR=/openssl/include \
    OPENSSL_LIB_DIR=/openssl/lib \
    RUST_BACKTRACE=1 \
    STRIP=strip