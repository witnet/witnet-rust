#!/bin/bash

set -ex

main() {
    local version=1.0.2p
    local install_dir="/usr/local/openssl"

    # Install dependencies using brew
    brew install \
        m4 \
        make

    # Create temporary directory
    td=$(mktemp -d)
    pushd $td

    # Download and extract OpenSSL
    wget https://www.openssl.org/source/openssl-$version.tar.gz
    tar --strip-components=1 -xzvf openssl-$version.tar.gz

    # Configure OpenSSL for arm64, disabling assembly
    ./Configure \
        --prefix="$install_dir" \
        darwin64-x86_64-cc \
        no-dso \
        no-asm \
        -fPIC \
        ${@:1}

    # Build using all available cores
    KERNEL_BITS=64 make -j$(sysctl -n hw.ncpu)

    # Install
    sudo make install

    popd

    # Cleanup
    rm -rf $td
}

main "${@}"