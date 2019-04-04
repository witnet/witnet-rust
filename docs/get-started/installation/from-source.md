# Running `witnet-rust` from source code

## Install compilation dependencies

### Rust 2018 (`stable` channel)

```console
curl https://sh.rustup.rs -sSf | sh
source $HOME/.cargo/env
rustup default stable
```

### Compilation dependencies

```console tab="GNU/Linux (apt)"
apt install -y clang git libssl-dev protobuf-compiler
```

```console tab="GNU/Linux (pacman)"
pacman -S clang git openssl protobuf
```

```console tab="macOS"
xcode-select --install
brew install git openssl protobuf
```

### MkDocs Python packages
_(Optional, only if generating documentation)_

```console
pip install mkdocs
pip install pymdown-extensions
pip install mkdocs-material
```

## Checkout source code from GitHub

```console tab="HTTPS"
git clone https://github.com/witnet/witnet-rust.git
cd witnet-rust
```

```console tab="SSH"
git clone git@github.com:witnet/witnet-rust.git
cd witnet-rust
```

## Run with `cargo`

By default, this line will run a Witnet node and connect to the Testnet using the default configuration:

```console
cargo run node
```

For more `witnet-rust` commands (`cli`, `wallet`, etc.) you can read the [Witnet-rust CLI documentation][CLI].

## Building a release

This one-liner will build a releasable standalone binary compatible with the architecture of your computer's processor:

```console
cargo build --release
```

The resulting binary will be located at `./target/release/witnet`.

If you want to produce binaries for other architectures, please read the [cross compilation instructions][cross-compilation].

[CLI]: /development/#cli
[cross-compilation]: /get-started/installation/cross-compilation