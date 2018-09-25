# Witnet-rust

__Witnet-rust__ is an open-source implementation of the Witnet protocol written in Rust.

The Witnet protocol, as outlined by the [Witnet Whitepaper][whitepaper], allows a network of computers to act as a
"decentralized oracle" that retrieves, attests and delivers information to smart contracts without having to place trust
 in a single entity.

This _Decentralized Oracle Network (DON)_ maintains and distributes a _block chain_ data structure that serves as a
common ledger for the operation of the protocol as well as for the _wit_ token, which is central to incentivizing the
network players to abide by the protocol and make them liable for any misbehavior.

Active network participants will earn wit tokens for fulfilling the data retrieval, attestation and delivery tasks
coming from different smart contract platforms such as Ethereum and RChain.

Witnet-rust is the first open-source implementation of the Witnet protocol and leverages the Rust programming language
to achieve utmost speed, memory safety and fearless concurrency without compromising on performance.

!!! tip
    See "[Why Rust?][why-rust]" for a more technical overview on why we chose Rust.

## Get started

### Installation

Witnet-rust is an open-source native app providing "full node" functionality of the Witnet Decentralized Oracle Network
protocol. It is available under the [GNU General Public License v3.0][license]. You may refer to the
[installation guide][install] in order to install the app together with its dependencies.

## Roadmap

Witnet-rust is an ambitious effort in its early days. We are currently working towards launching our first testnet.

As you can guess from our [datailed roadmap][roadmap] and [GitHub issues][issues], there are still a lot of missing
features (and a whole lot more that would be nice to have yet not critical for our testnet launch).

## Contributing

See the [contributing guide][contributing] to get more information on how to contribute to Rust-witnet development, and
the [roadmap] to find out what features are coming soon.


[why-rust]: get-started/why-rust.md
[whitepaper]: https://witnet.io/static/witnet-whitepaper.pdf
[license]: https://github.com/witnet/witnet-rust/blob/master/LICENSE
[install]: get-started/install.md
[issues]: https://github.com/witnet/witnet-rust/issues
[contributing]: contributing.md
[roadmap]: roadmap.md