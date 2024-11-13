FROM witnet/witnet-rust:latest

ARG TARGETPLATFORM

# Use a conditional COPY command based on the target platform
COPY witnet-centralized-ethereum-bridge-x86_64-unknown-linux-gnu /
COPY witnet-centralized-ethereum-bridge-aarch64-unknown-linux-gnu /

RUN if [ "$TARGETPLATFORM" = "linux/amd64" ]; then \
        echo "Current platform linux/amd64"; \
        mv /witnet-centralized-ethereum-bridge-x86_64-unknown-linux-gnu /witnet-centralized-ethereum-bridge; \
        rm /witnet-centralized-ethereum-bridge-aarch64-unknown-linux-gnu; \
    elif [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
        echo "Current platform linux/arm64"; \
        mv /witnet-centralized-ethereum-bridge-aarch64-unknown-linux-gnu /witnet-centralized-ethereum-bridge; \
        rm /witnet-centralized-ethereum-bridge-x86_64-unknown-linux-gnu; \
    else \
        echo "Unsupported platform: $TARGETPLATFORM"; \
        exit 1; \
    fi

ENTRYPOINT ["/witnet-centralized-ethereum-bridge", "--env"]