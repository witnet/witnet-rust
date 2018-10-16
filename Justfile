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

# run clippy
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# run formatter
fmt:
    cargo +nightly fmt -v --all

# run server
server:
    RUST_LOG=witnet=trace cargo run node 

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
    just clippy
    cargo test --all --verbose
