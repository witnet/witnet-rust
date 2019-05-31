FROM ubuntu:latest

# Install basic environment dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    pkg-config

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --verbose
ENV PATH=/root/.cargo/bin:$PATH
RUN rustc -vV && cargo -V

# Install common witnet-rust dependencies
RUN apt-get install -y --no-install-recommends \
    clang \
    libssl-dev \
    protobuf-compiler \
    librocksdb-dev

# Clean up apt packages so the docker image is as compact as possible
RUN apt-get clean && apt-get autoremove

# Set needed environment variables
ENV RUST_BACKTRACE=1 \
    ROCKSDB_LIB_DIR=/usr/lib/

# Get source from context and cd into it
RUN mkdir /source/
COPY / /source/
WORKDIR /source/

# Expose server ports
EXPOSE 21337
EXPOSE 21338
EXPOSE 11212

# Set compilation entry point (always gets executed)
ENTRYPOINT ["cargo"]

# Set default command (can be overriden)
CMD ["run", "--release", "node"]
