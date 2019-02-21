# Peer discovery

Peer discovery protocol allows nodes to exchange information of known peers to other nodes. This information may be used by peer discovery algorithms.

Nodes may request to their outbound peers for a list of their known "recent" peers. This request is initiated by sending a `GetPeers` message to a remote peer. The receiving node will reply a `Peers` message with a list of peer addresses that have been recently seen active in the network (e.g. peers that sent at least a message in the last 90 minutes). Usually, the transmitting node will then update its local list of peer addresses accordingly.

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

The `GetPeers` message has no payload.

## Peers message

The `Peers` message has a payload containing a list of known peers as:

| Field   |        Type        | Description                                                                          |
|:--------|:------------------:|:-------------------------------------------------------------------------------------|
| `peers` | `repeated Address` | List of IP addresses of active known peers, as described in the [IP address] section |

[IP Address]: /protocol/network/data-structures/ip-address/
