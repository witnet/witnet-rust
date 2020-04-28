FROM ubuntu:focal

# Install basic environment dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl-dev \
    curl

# Clean up apt packages so the docker image is as compact as possible
RUN apt-get clean && apt-get autoremove

# Set needed environment variables
ENV RUST_BACKTRACE=1

# Expose server ports
EXPOSE 21337
EXPOSE 21338
EXPOSE 11212

# Set entry point (always gets executed)
ENTRYPOINT ["/witnet/target/debug/witnet"]

# Set working directory
WORKDIR /witnet

# Set default command (can be overriden)
CMD ["-c", "/witnet/witnet.toml", "node", "server"]
