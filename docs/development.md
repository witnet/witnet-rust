# Development
Witnet-rust is build using [The Rust Programming language](https://rust-lang-nursery.github.io/edition-guide/introduction.html) and following [rust2018 edition guide](https://rust-lang-nursery.github.io/edition-guide/introduction.html).

## Installing

1. Install dependencies
    - Rust 1.31 (currently on the `beta` release channel)
    ```
    $ curl https://sh.rustup.rs -sSf | sh
    $ rustup default beta
    $ rustc --version
    ```
    - `clang` Clang compiler
    ```
    $ apt-get install clang
    ```
    - `flatc` FlatBuffers compiler (optional, only if recompiling schemas)
    ```
    $ pip install FoLiA-Linguistic-Annotation-Tool
    ```
    - `mkdocs`, `pymdown-extensions`, `mkdocs-material` python packages (optional, only if generating documentation)
    ```
    $ pip install mkdocs
    $ pip install pymdown-extensions
    $ pip install mkdocs-material
    ```

2. Clone and build the source code from github:
  ```
  $ git clone https://github.com/witnet/witnet-rust.git
  $ cd witnet-rust
  $ cargo build
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
  $ RUST_LOG=witnet=trace cargo run node
    or
  $ just node
  ```

## CLI
### Synopsis
    RUST_LOG=witnet=[error | info | debug | main | trace] cargo run
    [node [ --address address] [--config config_filename]]

### Components

#### Node
  --address (-d) *&lt;address&gt;*

  Read server address from *&lt;address&gt;* argument.

  --config (-c) *&lt;config_filename&gt;*

  Read config filename from *&lt;config_filename&gt;* argument.

## Development Scripts

  There are some useful scripts to run with ```just```:

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