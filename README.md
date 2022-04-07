<div align="center">
    <h1><img src="https://raw.githubusercontent.com/witnet/witnet-rust/master/.github/header.png" alt="Witnet-rust"/></a></h1>
    <a href="https://gitter.im/witnet/witnet-rust?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge"><img src="https://badges.gitter.im/witnet/witnet-rust.svg" alt="Join the chat at https://gitter.im/witnet/witnet-rust" /></a>
    <a href="https://travis-ci.com/witnet/witnet-rust"><img src="https://travis-ci.com/witnet/witnet-rust.svg?branch=master" alt="Build Status" /></a>
    <a href="https://github.com/witnet/witnet-rust/blob/master/LICENSE"><img src="https://img.shields.io/github/license/witnet/witnet-rust.svg" alt="GPLv3 Licensed" /></a>
    <a href="https://github.com/witnet/witnet-rust/graphs/contributors"><img src="https://img.shields.io/github/contributors/witnet/witnet-rust.svg" alt="GitHub contributors" /></a>
    <a href="https://github.com/witnet/witnet-rust/commits/master"><img src="https://img.shields.io/github/last-commit/witnet/witnet-rust.svg" alt="Github last commit" /></a>
    <br/><br/>
    <p><strong>witnet-rust</strong> is an open source implementation of the Witnet Decentralized Oracle Network protocol written in Rust.</p>
</div>

## Components
__witnet-rust__ implements many different components intended to work in the Witnet ecosystem:
- __[node]__: a fully validating and archival Witnet blockchain node.
- __[wallet]__: a separate server for managing Witnet keys and abstracting the complexity of creating transactions.
- __[crypto]__: library implementing all the crypto-related operations used by Witnet, including signatures, hash functions and verifiable random functions.
- __[rad]__: an interpreter for [RADON] scripts included in Witnet data requests.  
- __[storage]__: the convenient local storage solution used by `node` and `wallet`.
- __[p2p]__: modules for managing peer sessions and connections.
- __[data_structures]__: data structures common to all other components.
- __[validations]__: functions that validate Witnet protocol data structures.
- __[schemas]__: Protocol Buffer schemas for the Witnet protocol.

Members of the Witnet project also develop and maintain these other related Rust crates:
- __[protobuf-convert]__: macros for convenient serialization of Rust data structures into/from Protocol Buffers.
- __[async-jsonrpc-client]__: event-driven JSON-RPC client with support for multiple transports

## Current status

_DISCLAIMER: This is experimental software running on experimental network protocols. Be careful!_

- All the main components are in place—but they need yet to be battle tested before mainnet. 
- `Testnet-1` is live. [Here's the roadmap][roadmap] and this is [how to run a node].
- The Witnet community is doing its best to make `witnet-rust` rock solid as soon as possible.
- [Contributions](CONTRIBUTING.md) are more than welcome.

## Running / installing

Detailed installation instructions can be found in the [installation guide][install].

## Contributing

- To get involved, read our [contributing guide][contributing].
- You can find us on [Discord].

## Project documentation

Witnet project's official documentation is available at [docs.witnet.io][docs].

## License

Witnet-rust is published under the [GNU General Public License v3.0][license].

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
[how to run a node]: https://medium.com/witnet/how-to-run-a-full-node-on-the-witnet-testnet-911986b8add3
[docs]: https://docs.witnet.io
[install]: https://docs.witnet.io/try/run-a-node/
[Contributing]: https://github.com/witnet/witnet-rust/blob/master/CONTRIBUTING.md
[RADON]: https://docs.witnet.io/protocol/data-requests/overview/#the-rad-engine
[Discord]: https://discord.gg/FDPPv7H
[license]: https://github.com/witnet/witnet-rust/blob/master/LICENSE
