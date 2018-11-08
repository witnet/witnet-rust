# Peer discovery

Peer discovery protocol allows nodes to exchange information of known peers to other nodes. This information may be used by peer discovery algorithms.

Nodes may request to their outbound peers for a list of their known "recent" peers. This request is initiated by sending a `get_peers` message to a remote peer. The receiving node will reply a `peers` message with a list of peer addresses that have been recently seen active in the network (e.g. peers that sent at least a message in the last 90 minutes). Usually, the transmitting node will then update its local list of peer addresses accordingly.

```ascii
         NodeA                            NodeB
           +                                +
           |           GET_PEERS            |
           +------------------------------->+
           |             PEERS              |
           +<-------------------------------+
           |                                |
           +                                +
```

## Get peers message

The `get_peers` message only consists of a message header with the `GET_PEERS` command.

## Peers message

The `peers` message consists of a message header with the `PEERS` command and a payload containing a list of known peers as:

| Field   | Type     | Description                                                                          |
| ------- | :------: | ------------------------------------------------------------------------------------ |
| `peers` | `addr[]` | List of IP addresses of active known peers, as described in the [IP address] section |

[IP Address]: /protocol/network/data-structures/ip-address/
