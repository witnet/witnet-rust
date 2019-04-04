FROM ubuntu:latest

# Install basic environment dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --verbose
ENV PATH=/root/.cargo/bin:$PATH
RUN rustc -vV && cargo -V

# Install the arm target for Rust
RUN rustup target add aarch64-unknown-linux-gnu

# Install cross compilation dependencies
RUN apt-get install -y --no-install-recommends \
    binutils-aarch64-linux-gnu \
    gcc-aarch64-linux-gnu \
    libc6-dev-arm64-cross \
    libc6-dev-i386 \
    g++ \
    g++-aarch64-linux-gnu

# Compile and install OpenSSL for arm
COPY openssl.sh /
RUN bash /openssl.sh linux-aarch64 aarch64-linux-gnu-

# Install common witnet-rust dependencies
RUN apt-get install -y --no-install-recommends \
    clang \
    libssl-dev \
    protobuf-compiler

# Clean up apt packages so the docker image is as compact as possible
RUN apt-get clean && apt-get autoremove

# Set needed environment variables
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++ \
    OPENSSL_DIR=/openssl \
    OPENSSL_INCLUDE_DIR=/openssl/include \
    OPENSSL_LIB_DIR=/openssl/lib \
    PKG_CONFIG_ALLOW_CROSS=1 \
    RUST_BACKTRACE=1 \
    STRIP=aarch64-linux-gnu-strip