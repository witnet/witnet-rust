# run clippy installation
install-clippy:
    rustup component add clippy-preview

# run rustfmt installation
install-rustfmt:
    rustup component add rustfmt-preview

# run dev tools installation
install-setup:
    rustup update
    just install-clippy
    just install-rustfmt

# run formatter
fmt:
    cargo +nightly fmt -v --all

# run server
server:
    RUST_LOG=witnet=trace cargo run server

# run local documentation server at localhost:8000
docs-dev:
    mkdocs serve

# compile docs into static files
docs-build:
    mkdocs build

# deploy compiled docs into gh-pages branch
docs-deploy:
    mkdocs gh-deploy
