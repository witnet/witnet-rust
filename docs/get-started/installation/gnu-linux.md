# Running `witnet-rust` on GNU/Linux

## Download the `witnet-rust` package
GNU/Linux packages are available in our GitHub repository:

- [Witnet-rust for desktop GNU/Linux (x86_64)][release]
- [Witnet-rust for Raspberry Pi GNU/Linux (armv6l)][release]

## Unpacking and granting execution permission

```console
tar -zxf witnet-*-linux-gnu.tar.gz
chmod +x ./witnet
```

## Running the binary

Running the `witnet-rust` binary cannot be easier. By default, this line will run a Witnet node and connect to the
Testnet using the default configuration:

```console
./witnet node
```

For more `witnet-rust` components (`cli`, `wallet`, etc.) you can read the [Witnet-rust CLI documentation][CLI].

[release]: https://github.com/witnet/witnet-rust/releases/latest
[CLI]: /development/#cli