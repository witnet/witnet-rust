# Cross compiling `witnet-rust`

`witnet-rust` supports cross compilation to different architectures and targets.

For the sake of easing up the process of having a working cross compilation environment, we provide multiple [Dockerfiles] for quickly setting up [Docker] containers with Ubuntu 18.04 and all the dependencies already in place.

## Supported hosts

Cross compilation only supports by `x86_64` hosts, i.e. GNU/Linux or macOS running on the typical Intel or AMD 64 bit processors found in most desktop and laptop computers.

## Supported targets

- `aarch-unknown-linux-gnu`: 64 bit GNU/Linux distributions on ARMv8 processors, like the Raspberry Pi 3
- `armv7-unknown-linux-gnueabihf`: 32 bit GNU/Linux distributions on ARMv7/8 processors, like the Raspberry Pi 2 and 3
- `arm-unknown-linux-gnueabihf`: 32 bit GNU/Linux distributions on ARMv6 processors, like the Raspberry Pi 1 and Zero

## Requirements

The only requirements for cross compilation are:

- A `x86_64` host running either a 64 bit GNU/Linux distribution or a recent version of macOS.
- Docker. Note that on GNU/Linux non-sudo users need to be in the `docker` group. Read the [official post-installation steps][docker-postinstall-linux].
- The `just` command runner tool.

!!! tip "Installing the `just` tool"

    `just` is a command runner tool widely used in the Rust ecosystem. You can install it with a single line:

    ```console
    cargo install just
    ```

## Building the Docker images

We provide a one-liner command that will build a Docker image ready to cross compile the specified target:

```console
just docker-image-build <target>
```

!!! tip

    For example, to generate a Docker image for cross compiling `armv7-unknown-linux-gnueabihf` binaries, just run:

    ```console
    just docker-image-build armv7-unknown-linux-gnueabihf
    ```

We also provide another command for conveniently generating Docker images for all the supported cross compilation targets:

```console
just docker-image-build-all
``` 

## Running the cross compilation process

Once you have built a Docker image for one of the targets, running the cross compilation process inside it is extremely easy:

```console
just cross-compile <target> <profile=release>
```

The second argument of `just cross-compile` allows to customize the [release profile] for the compilation. If not specified, it will use the `release` profile by default. 

!!! tip

    For example, to cross-compile `witnet-rust` for `armv7-unknown-linux-gnueabihf`, just run:

    ```console
    just cross-compile armv7-unknown-linux-gnueabihf
    ```

The resulting binary should be located at `./target/<target>/<profile>/witnet`.

We also provide another command for conveniently cross compiling all the supported targets at once:

```console
just cross-compile-all
```

## Supporting more targets

Adding support for additional targets is extremely easy as long as the target platform is in turn [supported by Rust][Rust-platforms].

1. Write a Dockerfile capable of producing binaries for the target of your choice.
2. Copy the Dockerfile to `./docker/<target>/Dockerfile`.
3. Try it with `just docker-image-build <target>` and `just cross-compile <target>`.
4. Make a Pull Request to our [GitHub repository] so that others can also build binaries for the target platform.


[Dockerfiles]: https://github.com/witnet/witnet-rust/tree/master/docker
[Docker]: https://www.docker.com/why-docker
[docker-postinstall-linux]: https://docs.docker.com/install/linux/linux-postinstall/
[release profile]: https://doc.rust-lang.org/1.30.0/book/second-edition/ch14-01-release-profiles.html
[Rust-platforms]: https://forge.rust-lang.org/platform-support.html
[GitHub repository]: https://github.com/witnet/witnet-rust