# run continuous integration checks
ci +flags="":
    just versions 2>/dev/null || just install-setup
    cargo fmt --all -- --check
    just clippy
    cargo test --all --verbose {{flags}}

# install clippy
install-clippy:
    rustup component add clippy-preview

# install rustfmt
install-rustfmt:
    rustup component add rustfmt-preview

# install dev tools
install-setup:
    rustup update
    just install-clippy
    just install-rustfmt

# print tool versions
versions:
    rustc --version
    cargo fmt -- --version
    cargo clippy -- --version

# additional clippy lints
export CLIPPY_LINTS := '-D warnings
    -D clippy::cast-lossless
    -D clippy::cast-possible-truncation
    -D clippy::cast-possible-wrap
    -D clippy::cast-precision-loss
    -D clippy::cast-sign-loss
    -D clippy::checked-conversions
    -A clippy::upper-case-acronyms
'

# run clippy
clippy +flags="":
    cargo clippy --all --all-features -- $CLIPPY_LINTS {{flags}}
    cargo clippy --all --all-targets --all-features -- $CLIPPY_LINTS -A clippy::many-single-char-names {{flags}}


# run formatter
fmt +flags="":
    cargo fmt -v --all {{flags}}

# run node
node +args="":
    RUST_LOG=witnet=info cargo run node {{args}}

# run local documentation server at localhost:8000
docs-dev:
    mkdocs serve

# compile docs into static files
docs-build:
    mkdocs build

# deploy compiled docs into gh-pages branch
docs-deploy:
    mkdocs gh-deploy

# run continuous integration checks on a different platform using docker
docker-ci target="x86_64-unknown-linux-gnu" +flags="":
    docker run \
        -v $(pwd):/project:rw \
        -v $(pwd)/target:/target \
        -w /project \
        -it witnet-rust/{{target}} \
        just ci --target-dir=/target --target={{target}} {{flags}}

# run latest debug binary inside a docker container
docker-debug log_level="debug" +flags="-c /witnet/witnet.toml node server":
    docker run \
        -e RUST_LOG=witnet={{log_level}} \
        -v `pwd`:/witnet \
        -it witnet/debug-run {{flags}}

# build docker images for all cross compilation targets
docker-image-build-all:
    find ./docker/cross-compilation -type d -ls | tail -n +2 | sed -En "s/^(.*)\.\/docker\/cross-compilation\/(.*)/\2/p" | xargs -n1 just docker-image-build

# build docker image for a specific compilation target
docker-image-build target:
    docker build --no-cache -t witnet-rust/{{target}} -f docker/cross-compilation/{{target}}/Dockerfile docker/cross-compilation

# builds a multi architecture docker image for running a witnet-rust node from released binaries
docker-image-buildx version="latest" +flags="":
    docker buildx build \
        -f docker/witnet-rust/Dockerfile \
        --build-arg WITNET_VERSION={{version}} \
        --platform linux/amd64,linux/arm64,linux/arm/v7 \
        --tag witnet/witnet-rust:{{version}} \
        docker/witnet-rust {{flags}}

docker-python-tester test_name="example":
    docker run \
    --network=host \
    -v `pwd`/docker/python-tester:/tests \
    -v `pwd`/examples:/requests \
    -it witnet/python-tester {{test_name}}.py

# cross compile witnet-rust for all cross compilation targets
cross-compile-all:
    find ./docker/cross-compilation -type d -ls | tail -n +2 | sed -En "s/^(.*)\.\/docker\/cross-compilation\/(.*)/\2/p" | xargs -n1 just cross-compile

# cross compile witnet-rust for a specific compilation target
# - this assumes the container to set the `$STRIP` variable to be the path for binutils `strip` tool
# - if `$STRIP` is unset, the binary will not be stripped and will retain all its symbols
cross-compile target profile="release":
    docker run \
    -v `pwd`:/project:ro \
    -v `pwd`/target:/target \
    -v ~/.cargo:/root/.cargo \
    -w /project \
    -i witnet-rust/{{target}} \
    bash -c "cargo build `[[ {{profile}} == "release" ]] && echo "--release"` --target={{target}} --target-dir=/target \
    && [ ! -z "\$STRIP" ] \
    && \$STRIP /target/{{target}}/{{profile}}/witnet || echo \"No STRIP environment variable is set, passing.\""

# run the latest stable release in the latest testnet
e2e-stable test_name="example" +flags="":
    TEST_NAME={{test_name}} \
    docker-compose \
    -f docker/compose/e2e-stable/docker-compose.yaml \
    up \
    --scale=node=1 \
    --abort-on-container-exit \
    --exit-code-from tester \
    {{flags}}

# run the local debug binary (taken from ./target/debug) in the latest testnet
e2e-debug test_name="example" +flags="":
    cargo build
    TEST_NAME={{test_name}} \
    docker-compose \
    -f docker/compose/e2e-debug/docker-compose.yaml \
    up \
    --abort-on-container-exit \
    --exit-code-from tester \
    {{flags}}
