FROM ubuntu:noble

# Install needed dependencies
RUN apt update && \
    apt upgrade -y && \
    apt install -y protobuf-compiler wget curl build-essential openssl libssl-dev pkg-config libclang-dev

# Configure openssl
RUN pkg-config openssl

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
RUN echo 'source $HOME/.cargo/env' >> $HOME/.bashrc