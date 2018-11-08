# Handshake

The protocol to connect from a local peer (handshake initiator) to a known remote peer, is known as a "handshake." The handshake starts with a TCP connection to a given IP address and port.

The handshake initiator, sends a `version` message to the remote peer. Then, the remote peer will analyze the information in order to evaluate if the submitting peer is compatible regarding their supported versions and capabilities. If so, the remote peer will acknowledge the `version` message and establish a connection by sending a `verack` message.

Subsequently, the handshake initiator will expect a `version` message from the remote peer. The local peer will also acknowledge by replying with a `verack` message.

A peer cannot consider a **Witnet session** to be valid and established until it has received a `verack` message (in response to a previously sent `version` message) and it has sent a `verack` message (as acknowledgement to a previously received `version` message).

After the TCP connection has been started, both peers will define a timeout to wait for establishing a valid Witnet session. If no `verack` is received during these timeouts (e.g. 10 seconds), the TCP connection will be dropped and the remote peer will be discarded from the known peers list. Additionally, Witnet nodes will not reply to any other message types until a valid Witnet session has been successfully established.

```ascii
         NodeA                            NodeB
           +                                +
         ^ |            VERSION             | ^
         | +------------------------------->+ |
         | |                                | |
         | |            VERACK              | |
         | +<-------------------------------+ |
TimeoutA | |            VERSION             | | TimeoutB
         | +<-------------------------------+ |
         | |                                | |
         | |            VERACK              | |
         | +------------------------------->+ |
         | |                                | |
         v |                                | v
           +                                +
```

## Version message

The `version` message contains the following information:

| Field              | Type       | Description                                                                                                    |
| ------------------ | :--------: | -------------------------------------------------------------------------------------------------------------- |
| `version`          | `u32`      | The Witnet p2p protocol version that the client is using                                                       |
| `timestamp`        | `u64`      | The current UTC Unix timestamp (seconds since Unix epoch)                                                      |
| `capabilities`     | `u64`      | List of flags of supported services, by default NODE_NETWORK is used                                           |
| `sender_address`   | `addr`     | The IP address and port of the handshake initiator peer                                                        |
| `receiver_address` | `addr`     | The IP address and port of the remote peer                                                                     |
| `user_agent`       | `string`   | A version showing which software is running the local peer                                                     |
| `last_epoch`       | `u32`      | Last epoch in the local peer blockchain                                                                        |
| `genesis`          | `[u32; 8]` | Hash of the genesis block                                                                                      |
| `nonce`            | `u64`      | Node random nonce, randomly generated every time a version packet is sent (used to detect connections to self) |

## Verack message

The `verack` message is sent as reply to the version and it only consists of a message header with the command `VERACK`.
