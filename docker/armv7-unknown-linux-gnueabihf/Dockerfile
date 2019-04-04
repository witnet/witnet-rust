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
RUN rustup target add armv7-unknown-linux-gnueabihf

# Install cross compilation dependencies
RUN apt-get install -y --no-install-recommends \
    binutils-arm-linux-gnueabihf \
    gcc-arm-linux-gnueabihf \
    libc6-dev-armhf-cross \
    libc6-dev-i386 \
    g++ \
    g++-arm-linux-gnueabihf

# Compile and install OpenSSL for arm
COPY openssl.sh /
RUN bash /openssl.sh linux-armv4 arm-linux-gnueabihf-

# Install common witnet-rust dependencies
RUN apt-get install -y --no-install-recommends \
    clang \
    libssl-dev \
    protobuf-compiler

# Clean up apt packages so the docker image is as compact as possible
RUN apt-get clean && apt-get autoremove

# Set needed environment variables
ENV CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
    CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc \
    CXX_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-g++ \
    OPENSSL_DIR=/openssl \
    OPENSSL_INCLUDE_DIR=/openssl/include \
    OPENSSL_LIB_DIR=/openssl/lib \
    PKG_CONFIG_ALLOW_CROSS=1 \
    RUST_BACKTRACE=1 \
    STRIP=arm-linux-gnueabihf-strip