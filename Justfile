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

# run clippy
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# run formatter
fmt:
    cargo +nightly fmt -v --all

# run node
node:
    RUST_LOG=witnet=info cargo run node 

# run local documentation server at localhost:8000
docs-dev:
    mkdocs serve

# compile docs into static files
docs-build:
    mkdocs build

# deploy compiled docs into gh-pages branch
docs-deploy:
    mkdocs gh-deploy

# run travis
travis:
    just install-setup
    just versions
    cargo fmt --all -- --check
    just clippy
    cargo test --all --verbose

# build docker images for all cross compilation targets
docker-image-build-all:
    find ./docker -type d -ls | tail -n +2 | sed -En "s/^(.*)\.\/docker\/(.*)/\2/p" | xargs -n1 just docker-build

# build docker image for a specific compilation target
docker-image-build target:
    docker build -t witnet-rust/{{target}} -f docker/{{target}}/Dockerfile docker

# cross compile witnet-rust for all cross compilation targets
cross-compile-all:
    find ./docker -type d -ls | tail -n +2 | sed -En "s/^(.*)\.\/docker\/(.*)/\2/p" | xargs -n1 just cross-compile

# cross compile witnet-rust for a specific compilation target
cross-compile target profile="release":
    docker run \
    -v $(pwd):/project:ro \
    -v $(pwd)/target:/target \
    -w /project \
    -i witnet-rust/{{target}} \
    bash -c "cargo build --{{profile}} --target={{target}} --target-dir=/target && \$STRIP /target/{{target}}/{{profile}}/witnet"
