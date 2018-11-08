# Witnet Network Protocol

Witnet network protocol is inspired by other blockchain network protocols such as Bitcoin, Ethereum, Exonum, Mimblewimble Grin and Rchain. Some of the aforementioned blockchains architecture have been taken into consideration as their reference implementations are also coded in Rust.

The Witnet network protocol can be deconstructed into different message protocols:

- Handshake: negotiation between peers to establish valid Witnet protocol sessions
- Peer discovery: exchange lists of known peers
- Heartbeat: exchange beacons indicating that the session is active
- Inventory exchange: synchronization of objects (blocks, transactions, etc.) between peers

Additionally, for the aforementioned protocols, some [constants] and specific data structures have been specified, such as:

- [Block]
- [IP Address]
- [Transaction]

## References

Bitcoin:

- [Developer Reference - Bitcoin](https://bitcoin.org/en/developer-reference)
- [Wiki](https://en.bitcoin.it/wiki/)
- [GitHub - bitcoin/bitcoin: Bitcoin Core integration/staging tree](https://github.com/bitcoin/bitcoin/)
- [GitHub - paritytech/parity-bitcoin: The Parity Bitcoin client](https://github.com/paritytech/parity-bitcoin/)

Ethereum:

- [Wiki · GitHub](https://github.com/ethereum/wiki/wiki)
- [GitHub - ethereum/go-ethereum: Official Go implementation of the Ethereum protocol](https://github.com/ethereum/go-ethereum)
- [GitHub - paritytech/parity-ethereum: The fast, light, and robust EVM and WASM client.](https://github.com/paritytech/parity-ethereum/)

Exonum:

- [Exonum Documentation](https://exonum.com/doc/)
- [GitHub - exonum/exonum: An extensible open-source framework for creating private/permissioned blockchain applications](https://github.com/exonum/exonum)

Mimblewimble Grin:

- [Grin, the Tech | Simple, privacy-focused, scalable MimbleWimble chain implementation.](https://grin-tech.org/)
- [Wiki · GitHub](https://github.com/mimblewimble/docs/wiki)
- [GitHub - mimblewimble/grin: Minimal implementation of the MimbleWimble protocol.](https://github.com/mimblewimble/grin)

RChain:

- [Documentation](https://developer.rchain.coop/documentation)
- [GitHub - rchain/rchain](https://github.com/rchain/rchain)


[constants]: /protocol/network/constants/
[Block]: /protocol/network/data-structures/block/
[IP Address]: /protocol/network/data-structures/ip-address/
[Transaction]: /protocol/network/data-structures/transaction/