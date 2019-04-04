FROM ubuntu:latest

# Install basic environment dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    unzip \
    wget

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --verbose
ENV PATH=/root/.cargo/bin:$PATH
RUN rustc -vV && cargo -V

# Install the arm target for Rust
RUN rustup target add arm-unknown-linux-gnueabihf

# Install cross compilation dependencies
# This part is based on https://github.com/tiziano88/rust-raspberry-pi
ARG RASPBERRY_PI_TOOLS_COMMIT_ID=5caa7046982f0539cf5380f94da04b31129ed521
RUN wget https://github.com/raspberrypi/tools/archive/$RASPBERRY_PI_TOOLS_COMMIT_ID.zip -O /root/pi-tools.zip
RUN unzip /root/pi-tools.zip -d /root
RUN mv /root/tools-$RASPBERRY_PI_TOOLS_COMMIT_ID /root/pi-tools
ENV PATH=/root/pi-tools/arm-bcm2708/arm-linux-gnueabihf/bin:$PATH
ENV PATH=/root/pi-tools/arm-bcm2708/arm-linux-gnueabihf/libexec/gcc/arm-linux-gnueabihf/4.8.3:$PATH
RUN apt-get install -y --no-install-recommends \
    libc6-dev-armhf-cross \
    libc6-dev-i386 \
    g++

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
ENV CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
    CC_arm_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc \
    CXX_arm_unknown_linux_gnueabihf=arm-linux-gnueabihf-g++ \
    OPENSSL_DIR=/openssl \
    OPENSSL_INCLUDE_DIR=/openssl/include \
    OPENSSL_LIB_DIR=/openssl/lib \
    PKG_CONFIG_ALLOW_CROSS=1 \
    RUST_BACKTRACE=1 \
    STRIP=arm-linux-gnueabihf-strip
