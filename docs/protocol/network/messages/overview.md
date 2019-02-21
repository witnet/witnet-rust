# Messages

All Witnet network protocol messages include a message header identifying which message type is being sent and a command-specific payload.

## Message Header

The message header format is composed of the following fields:

| Field     |   Type    | Description                                                     |
|:----------|:---------:|:----------------------------------------------------------------|
| `magic`   | `uint32`  | Magic value indicating message origin network                   |
| `command` | `Command` | Message being sent from a predefined list of available commands |

The `command` must be one message type from the current available commands defined in the Witnet network protocol:

* `Version`
* `Verack`
* `GetPeers`
* `Peers`
* `Ping`
* `Pong`
* `Block`
* `InventoryAnnouncement`
* `InventoryRequest`
* `LastBeacon`
* `Transaction`

Available commands are detailed in the consecutive sections:

- [Handshake]
- [Peer discovery]
- [Heartbeat]
- [Inventory]

[Handshake]: /protocol/network/messages/handshake/
[Heartbeat]: /protocol/network/messages/heartbeat/
[Peer discovery]: /protocol/network/messages/peer-discovery/
[Inventory]: /protocol/network/messages/inventory/