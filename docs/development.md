# Development

`witnet-rust` is built using [The Rust Programming language][rust] and following the
[Rust 2018 Edition Guide][rust-2018] as its code styling convention.

## Installing

We have installation guides for several operating systems:

- [Installing `witnet-rust` on GNU/Linux][install-gnu-linux]
- [Installing `witnet-rust` on macOS][install-macos]
- [Installing `witnet-rust` on Windows][install-windows]
- [Compiling `witnet-rust` on from source code][install-from-source]
- [Cross compiling `witnet-rust`][cross-compilation]


## Running the CLI

### Synopsis
```console
RUST_LOG=witnet=[error | info | debug | main | trace] cargo run \
[node [ --address address] [--config config_filename]]
```

### Components

#### Node
```console
--address <address>
-d <address>
```

Read server address from `<address>` argument.

```console
--config <file_path>
-c <file_path>
```

Read configuration from the given `<file_path>` argument (defaults to `./witnet.toml`).

## Development Scripts

There are some useful scripts to run with the `just` tool:

- `clippy`: run `clippy` style checking.
- `docs-build`: compile docs into static files.
- `docs-deploy`: deploy compiled docs into gh-pages branch.
- `docs-dev`: run local documentation server at `localhost:8000`.
- `fmt`: run code formatter.
- `install-clippy`: install `clippy` library.
- `install-rustfmt`: install `rustfmt` library.
- `install-setup`: install dev tools (`clippy` and `rustfmt`).
- `server`: run witnet server component.
- `travis`: run travis build.

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