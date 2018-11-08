# Heartbeat

The hearbeat protocol's main purpose is to notify to other peers that the node is still active and running. This information may be relevant for managing a list of active peers.

The heartbeat protocol is defined by using `ping` and `pong` messages and it allows to implement different strategies to react to peer inactivity.

For example:

- If during a period of time (e.g. 30 minutes) a peer has not transmitted any messages, it will send a heartbeat as `ping` message.
- If during a period of time (e.g. 90 minutes) no message has been received by a remote peer, the local node will assume that the connection has been closed.

```ascii
         NodeA                          NodeB
           +                              +
           |             PING             |
           +----------------------------->+
           |             PONG             |
           +<-----------------------------+
           |                              |
           +                              +
```

## Ping and Pong messages

The `ping` message confirms that the connection is still valid. The `pong` message is sent in response to a `ping` message. Both contain only 1 field:

| Field   | Type  | Description     |
| ------- | :---: | --------------- |
| `nonce` | `u64` | A random number |
