# Development
Witnet-rust is build using [The Rust Programming language](https://rust-lang-nursery.github.io/edition-guide/introduction.html) and following [rust2018 edition guide](https://rust-lang-nursery.github.io/edition-guide/introduction.html).

## Installing

1. Install dependencies
    - Rust
    - FlatBuffers (Optional)

2. Clone the source code from github:
  ```
  $ git clone https://github.com/witnet/witnet-rust.git
  $ cd witnet-rust
  ```
3. Use cargo to install ```just``` .
  ```
  $ cargo install just
  ```
4. Run just script to install dev tools
  ```
  $ just install-setup
  ```
5. Run a witnet component. Find a list of components and how to run them at [CLI](#cli).
  ```
  $ RUST_LOG=witnet=trace cargo run server
    or
  $ just server
  ```

## CLI
### Synopsis
    RUST_LOG=witnet=[error | info | debug | main | trace] cargo run
    [server [ --address address] [--peer peer-address] [--background]]

### Components

#### Server
  --address *&lt;address&gt;*

  Read server address from *&lt;address&gt;* argument.

  --peer *&lt;peer-address&gt;*

  Read address to peer from *&lt;peer-address&gt;* argument.

  --background

  Not implemented.

## Development Scripts

  There are some usefull scripts to run with ```just```:

  - ```clippy```: Run ```clippy``` style checking.
  - ```docs-build```: Compile docs into static files.
  - ```docs-deploy```: Deploy compiled docs into gh-pages branch.
  - ```docs-dev```: Run local documentation server at localhost:8000
  - ```fmt```: Run code formatter.
  - ```install-clippy```. Install ```clippy``` library.
  - ```install-rustfmt```: Install ```rustfmt``` library.
  - ```install-setup```: Install dev tools.
  - ```server```: Run witnet server component.
  - ```travis```: Run travis build.