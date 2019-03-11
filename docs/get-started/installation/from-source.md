# Running `witnet-rust` from source code

## Install compilation dependencies

### Rust 2018 (`stable` channel)

```console
curl https://sh.rustup.rs -sSf | sh
source $HOME/.cargo/env
rustup default stable
```

### CLang compiler

```console tab="GNU/Linux (apt)"
apt-get install clang
```

```console tab="GNU/Linux (pacman)"
pacman -S clang
```

```console tab="macOS"
xcode-select --install
```

### Protocol Buffers compiler

```console tab="GNU/Linux (apt)"
apt-get install protobuf-compiler
```

```console tab="GNU/Linux (pacman)"
pacman -S protobuf
```

```console tab="macOS"
brew install protobuf
```

### Git client

```console tab="GNU/Linux (apt)"
apt-get install git
```

```console tab="GNU/Linux (pacman)"
pacman -S git
```

```console tab="macOS"
brew install git
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

[CLI]: /development/#cli