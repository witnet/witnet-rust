# Witnet Network Protocol

Witnet network protocol is inspired in other blockchain network protocols such as Bitcoin, Ethereum and Rchain. Additionally, other blockchains such as Grin and Exonum have been taken into consideration as their reference implementations are coded in Rust.

## Initial considerations

In order to start the network discovery process, at least one existing node on the network is required. However, in order to avoid unnecessary risks or potential attack vectors, it is always recommended to know several existing nodes extracted from different sources.

### Peer bootstrapping methods

The strategy for selecting the first node to connect should be random, thus it should not depend on geographical position or any other network parameter.

The client discovers the IP addresses and ports of other nodes in a decentralized way, i.e. there should not be a central point of trust or failure to bootstrap the network.

For example, some of the considered approaches for bootstrapping a new peer into the network could one or more of the following methods:

+ Hard coded addresses
+ Addresses provided as command line arguments
+ Addresses provided from user provided text file on startup
+ Store addresses in database and read them on startup
+ Exchange addresses with other nodes
+ DNS requests to receive IP addresses
+ IRC channel where peers are advertised

## Constants and Defaults

Witnet constants and default values used in the network protocol.

### Default Values

| Network | Default Port |
|---------|:------------:|
| mainnet |    11337     |
| testnet |    21337     |

### Witnet versions

Below the existing versions of the Witnet P2P network protocol are listed:

| Version |  Initial Release  | Major Changes |
|---------|-------------------|---------------|
|   010   | to be determined  |               |

### Magic numbers

These constant values indicate the originating network in the message headers:

| Network | Magic number |
|---------|:------------:|
| mainnet |     0x00     |
| testnet |     0xF0     |

### Node capabilities

Currently, only one node capabilities is being offered by Witnet:

| Flag    | Description  |
|---------|--------------|
| 0x0000000000000001 | NODE_NETWORK, Witnet full node which is the default operation mode |

### User agents

Currently, only one user agent is being developed:

| User Agent | Description  |
|---------|--------------|
| /Witnet-rust:0.1.0 | Witnet node implemented in Rust and version 0.1.0 |

## Common Structures

Common structures used along different Witnet network messages.

### Address Format

Witnet peer addresses are defined as follows:

| Field  | Type | Description |
|--------|:----:|-------------|
| ipv4   | u32 | IPv4 address of the peer |
| port | u16 | port number in which the peer is listening |

Alternatively, IP addresses may be IPv6:

| Field  | Type | Description |
|--------|:----:|-------------|
| ipv6   | [u32; 4] | IPv6 address of the peer |
| port | u16 | port number in which the peer is listening |

## Messages

The current network protocol includes the description and definition of the following messages:

+ VERSION
+ VERACK
+ GET_PEERS
+ PEERS
+ PING
+ PONG

All of these messages include a message header identifying which message type is being sent.

### Message Header

Inspired in other blockchains, the message header format is composed of the following fields:

| Field  | Type | Description |
|--------|:----:|-------------|
| magic  | u16  | magic value indicating message origin network |
| command  | string  | message being sent from a predefined list of available commands |
| payload  | ? | message data |

### Handshake (VERSION, VERACK)

The protocol to connect from a local peer (initiator) to a known remote peer, also known as "handshake", starts with a TCP connection to a given IP address and port.

The handshake initiator, the local peer, sends a `version` message to the remote peer.
The remote peer will analyze the information in order to evaluate if the submitting peer is compatible regarding their supported versions and capabilities. If so, the remote peer will acknowledge the `version` message and establish a connection by sending a `verack` message.

The `version` message contains the following information:

| Field  | Type | Description |
|--------|:----:|-------------|
| version| u32  | the Witnet p2p protocol version that the client is using |
| timestamp | i64 | current UTC Unix timestamp (seconds since Unix epoch) |
| capabilities | u64 | list of flags of supported services, by default NODE_NETWORK is used |
| sender_address | addr | the IP address and port of the handshake initiator peer |
| receiver_address | addr | the IP address and port of the remote peer |
| user_agent | string | a version showing which software is running the local peer |
| last_epoch | u32 | last epoch in the local peer blockchain |
| genesis | [u32; 8] | hash of the genesis block |
| nonce | u64 | Node random nonce, randomly generated every time a version packet is sent (used to detect connections to self) |

The `verack` message is sent as reply to the version and it only consists of a message header with the command `VERACK`.

Subsequently, the handshake initiator will expect a `version` message from the remote peer. The local peer will also acknowledge by replying with a `verack` message.

Connection cannot be considered as establish until both `verack` messages have been received by both peers.

After sending a `version` message the peers will define a timeout to wait for a response. If no response is received during this timeout (usually set to 10 seconds), the peer will be discarded from the known peers list.

### Peer discovery (GET_PEERS, PEERS)

In Witnet, a node must always establish different paths into the network by connecting to different peers, ideally randomly selected. As connections may disappear, network peers should assist others when they bootstrap.

After establishing a connection with remote peers (throughout a handshake mechanism), a node may request to their remote peer to receive a list of their known "recent" peers. This request is initiated by sending a `get_peers` message to a remote peer. The receiving node will reply with a `peers` message with a list of peer addresses that have been seen recently active in the network. The typical assumption is that a node is likely to be active if it has sent any message during the last 90 minutes. The transmitting node will then update those IP addresses and ports into its database of available nodes.

The `get_peers` message only consists of the message header with the command `GET_PEERS`.

The `peers` message consists of the message header with the command `PEERS` and as payload it contains a list of known peers as:

| Field  | Type | Description |
|--------|:----:|-------------|
| peers | addr[] | list of addresses of active known peers |

### Heartbeat (PING, PONG)

As in Bitcoin, the heartbeat protocol is defined with `ping` and `pong` messages and it is defined as follows:

+ If during the last 30 minutes a peer has no transmitted any message, it will send a heartbeat as `ping` message.
+ If in the last 90 minutes no message has been received by a remote peer, the local node will assume that the connection has been closed.

The `ping` message confirms that the connection is still valid. The `pong` message is sent in response to a `ping` message. Both contain only one field:

| Field  | Type | Description |
|--------|:----:|-------------|
| nonce | u64 | a random number |

### Error messages

The `error` messages are structured as follow:

| Field  | Type | Description |
|--------|:----:|-------------|
| code | u32 | predefined error code |
| message | string | user friendly message |

## References

Bitcoin:

+ [Developer Reference - Bitcoin](https://bitcoin.org/en/developer-reference)
+ [Wiki](https://en.bitcoin.it/wiki/)
+ [GitHub - bitcoin/bitcoin: Bitcoin Core integration/staging tree](https://github.com/bitcoin/bitcoin/)
+ [GitHub - paritytech/parity-bitcoin: The Parity Bitcoin client](https://github.com/paritytech/parity-bitcoin/)

Ethereum:

+ [Wiki · GitHub](https://github.com/ethereum/wiki/wiki)
+ [GitHub - ethereum/go-ethereum: Official Go implementation of the Ethereum protocol](https://github.com/ethereum/go-ethereum)
+ [GitHub - paritytech/parity-ethereum: The fast, light, and robust EVM and WASM client.](https://github.com/paritytech/parity-ethereum/)

RChain:

+ [Documentation](https://developer.rchain.coop/documentation)
+ [GitHub - rchain/rchain](https://github.com/rchain/rchain)

Mimblewimble Grin:

+ [Grin, the Tech | Simple, privacy-focused, scalable MimbleWimble chain implementation.](https://grin-tech.org/)
+ [Wiki · GitHub](https://github.com/mimblewimble/docs/wiki)
+ [GitHub - mimblewimble/grin: Minimal implementation of the MimbleWimble protocol.](https://github.com/mimblewimble/grin)

Exonum:

+ [Exonum Documentation](https://exonum.com/doc/)
+ [GitHub - exonum/exonum: An extensible open-source framework for creating private/permissioned blockchain applications](https://github.com/exonum/exonum)