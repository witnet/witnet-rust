# Messages

All Witnet network protocol messages include a message header identifying which message type is being sent and a command-specific payload.

## Message Header

The message header format is composed of the following fields:

| Field     | Type     | Description                                                     |
| --------- | :------: | --------------------------------------------------------------- |
| `magic`   | `u16`    | Magic value indicating message origin network                   |
| `command` | `string` | Message being sent from a predefined list of available commands |
| `payload` | `data`   | Message data defined in the specific message type being sent    |

The `command` string must be one message type from the current available commands defined in the Witnet network protocol.

```math
available_commands = {VERSION, VERACK, GET_PEERS, PEERS, PING, PONG, GET_BLOCKS, INV, GET_DATA, BLOCK, TX}
```

Available commands are detailed in the consecutive sections:

- [Handshake]
- [Peer discovery]
- [Heartbeat]
- [Inventory]

## Message Payload

The payload includes the message data defined in the specification of the command being sent. More details can be found in the following sections.

[Handshake]: /protocol/network/messages/handshake/
[Heartbeat]: /protocol/network/messages/heartbeat/
[Peer discovery]: /protocol/network/messages/peer-discovery/
[Inventory]: /protocol/network/messages/inventory/