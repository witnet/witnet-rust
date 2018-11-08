# Constants

Constant values are immutable within Witnet protocol versions. A change in the constants necessarily requires a new protocol version.

## Witnet versions

Witnet network protocol versions, defined as `u32`, are listed below:

| Version | Initial Release  | Major Changes |
| ------- | ---------------- | ------------- |
| `010`   | To be determined |               |

## Magic numbers

These constant values indicate the originating network in the message headers:

 | Magic number | Network     |
 | ------------ | :---------: |
 | `0x00`       | `mainnet`   |
 | `0xF1`       | `testnet-1` |

## Node capabilities

Node capabilities are defined as 64 bits sequences of masked flags, so that nodes may advertise which subset of services they are supporting. Currently, only one node capability is specified in the Witnet network protocol.

| Flag                 | Name           | Description                                          |
| -------------------- | -------------- | ---------------------------------------------------- |
| `0x0000000000000001` | `NODE_NETWORK` | Witnet full node which is the default operation mode |

## User agents

List of known user agents. Currently, only 1 user agent is being implemented:

| User Agent           | Description                                         |
| -------------------- | --------------------------------------------------- |
| `/Witnet-rust:0.1.0` | Witnet node implemented in Rust and version `0.1.0` |
