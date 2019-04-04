# Running `witnet-rust` on GNU/Linux

## Download the `witnet-rust` package
GNU/Linux packages [are available in our GitHub repository][release]. Currently supported GNU/Linux architectures are:

- `x86_64-linux-gnu`: most modern GNU/Linux distributions for the typical Intel or AMD desktop/laptop processors.
- `aarch-unknown-linux-gnu`: 64 bit GNU/Linux distributions on ARMv8 processors, like the Raspberry Pi 3
- `armv7-unknown-linux-gnueabihf`: 32 bit GNU/Linux distributions on ARMv7/8 processors, like the Raspberry Pi 2 and 3
- `arm-unknown-linux-gnueabihf`: 32 bit GNU/Linux distributions on ARMv6 processors, like the Raspberry Pi 1 and Zero

If you want to run `witnet-rust` on a Raspberry Pi, you should try the `armv7-unknown-linux-gnueabihf` binary unless:
- You positively know you are using a 64 bit distribution. Then use `aarch-unknown-linux-gnu`.
- You are using Pi model 1, model Zero or Zero W. Then use `arm-unknown-linux-gnueabihf`.

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