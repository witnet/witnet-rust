# Development

`witnet-rust` is built using [The Rust Programming language][rust] and following the
[Rust 2018 Edition Guide][rust-2018] as its code styling convention.

## Contributing

We have a [dedicated contributing guide][contributing] that will walk you through the process of making your first contribution to the `witnet-rust` project.

## Installing

We have installation guides for several operating systems:

- [Installing `witnet-rust` on GNU/Linux][install-gnu-linux]
- [Installing `witnet-rust` on macOS][install-macos]
- [Installing `witnet-rust` on Windows][install-windows]
- [Compiling `witnet-rust` from source code][install-from-source]
- [Cross compiling `witnet-rust`][cross-compilation]

## Components

`witnet-rust` comprises many different components that can be run separately but share a single entry point, which can 
be run by executing any of the `witnet` distributable binaries for your operating system or directly using `cargo`
on the source code. 

```console
witnet <component> [<args>]
```
```console
cargo run <component> [<args>]
```

### Node
__[witnet node][node]__ is a fully validating and archival Witnet blockchain node.

```console
witnet node [--config <config_file_path>]
```
```console
witnet node [-c <config_file_path>]
```

Unless disabled in the configuration, `node` will also start a [local JSON-RPC server][json-rpc] that provides many management methods.

The [configuration `.toml` file][toml] is common to all components in the `witnet-rust` project.

### Wallet
__[witnet wallet][wallet]__ is a separate server for managing Witnet keys and abstracting the complexity of creating transactions.
```console
witnet wallet [--config <config_file_path>]
```
```console
witnet wallet [-c <config_file_path>]
```

The `wallet` component acts as a client of the [JSON-RPC server][json-rpc] that the `node` component starts.
This component will in turn start a [WebSockets server][wallet-api] exposing its own [Wallet API][wallet-api].

The [configuration `.toml` file][toml] is common to all components in the `witnet-rust` project.

### CLI
__witnet cli__ is a command line interface for interacting with 
```console
witnet cli <method> [--config <config_file_path>]
```
```console
witnet cli <method> [-c <config_file_path>]
```

The `cli` component acts as a client for:
- the [JSON-RPC server][json-rpc] that the `node` component starts.
- the [WebSockets server][wallet-api] that the `wallet` component starts.

The [configuration `.toml` file][toml] is common to all components in the `witnet-rust` project.

### Libraries
The `witnet-rust` project also comprises many other components in form of libraries that are used as static dependencies:

- __[crypto]__: library implementing all the crypto-related operations used by Witnet, including signatures, hash functions and verifiable random functions.
- __[rad]__: an interpreter for [RADON] scripts included in Witnet data requests.  
- __[storage]__: the convenient local storage solution used by `node` and `wallet`.
- __[p2p]__: modules for managing peer sessions and connections.
- __[data_structures]__: data structures common to all other components.
- __[validations]__: functions that validate Witnet protocol data structures.
- __[schemas]__: Protocol Buffer schemas for the Witnet protocol.

These other related Rust crates are also developed or maintained by members of the `witnet-rust` project:

- __[protobuf-convert]__: macros for convenient serialization of Rust data structures into/from Protocol Buffers.
- __[async-jsonrpc-client]__: event-driven JSON-RPC client with support for multiple transports


## Development Scripts

There are some useful scripts to run with the `just` tool:

- `just ci`: run the same sequence of commands as used for continuous integration ([Travis CI][travis]).
- `just clippy`: run `clippy` style checking.
- `just cross-compile <target>`: cross compile `witnet-rust` for the chosen `target`, which should be [one of the supported targets][supported-targets].
- `just cross-compile-all`: cross compile `witnet-rust` for [all the supported targets][supported-targets].
- `just docker-ci <target>`: run `just docker-ci` inside a docker container named `witnet-rust/<target>` (`target` defaults to `x86_64-unknown-linux-gnu`).
- `just docker-image-build <target>`: create a docker image for one of the [supported targets][supported-targets] in the local docker installation.
- `just docker-image-build-all`: create docker images for all the [supported targets][supported-targets] in the local docker installation.
- `just docs-build`: compile docs into static files.
- `just docs-deploy`: deploy compiled docs into gh-pages branch.
- `just docs-dev`: run local documentation server at `localhost:8000`.
- `just fmt`: run code formatter.
- `just install-clippy`: install `clippy` code quality tool.
- `just install-rustfmt`: install `rustfmt` code formatter tool.
- `just install-setup`: install all deverlopment tools (`clippy` and `rustfmt`).
- `just node`: run witnet-rust's `node` component.
- `just node`: print installed versions of `rustc`, `fmt` and `clippy`. 

!!! tip "Installing the `just` tool"

    `just` is a command runner tool widely used in the Rust ecosystem. You can install it with a single line:

    ```console
    cargo install just
    ```

[rust]: https://rust-lang.org
[rust-2018]: https://rust-lang-nursery.github.io/edition-guide/introduction.html
[install-gnu-linux]: /get-started/installation/gnu-linux
[install-macos]: /get-started/installation/macos
[install-windows]: /get-started/installation/windows
[install-from-source]: /get-started/installation/from-source
[cross-compilation]: /get-started/installation/cross-compilation
[travis]: https://travis-ci.com/witnet/witnet-rust/builds
[supported-targets]: /get-started/installation/cross-compilation/#supported-targets
[contributing]: /get-started/contributing
[json-rpc]: /interface/json-rpc/
[wallet-api]: /interface/wallet/
[node]: https://github.com/witnet/witnet-rust/tree/master/node
[wallet]: https://github.com/witnet/witnet-rust/tree/master/wallet
[crypto]: https://github.com/witnet/witnet-rust/tree/master/crypto
[rad]: https://github.com/witnet/witnet-rust/tree/master/rad
[storage]: https://github.com/witnet/witnet-rust/tree/master/storage
[p2p]: https://github.com/witnet/witnet-rust/tree/master/p2p
[data_structures]: https://github.com/witnet/witnet-rust/tree/master/data_structures
[validations]: https://github.com/witnet/witnet-rust/tree/master/validations
[schemas]: https://github.com/witnet/witnet-rust/blob/master/schemas/witnet/witnet.proto
[protobuf-convert]: https://github.com/witnet/protobuf-convert
[async-jsonrpc-client]: https://github.com/witnet/async-jsonrpc-client
[roadmap]: https://medium.com/witnet/an-updated-witnet-roadmap-to-mainnet-cb8543c534a4
[toml]: /configuration/toml-file/