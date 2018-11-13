# Session

__Session__ is the actor that encapsulates the entire business logic of the Witnet network protocol.



## Actor creation and registration

The creation of the session actor and its addition to the system registry are
performed by the `connection_manager` process:

```rust
Session::create(move |ctx| {
    // Get peer address
    let address = stream.peer_addr().unwrap();

    // Split TCP stream into read and write parts
    let (r, w) = stream.split();

    // Add stream in session actor from the read part of the tcp stream
    Session::add_stream(FramedRead::new(r, P2PCodec), ctx);

    // Create the session actor and store in its state the write part of the TCP stream
    Session::new(address, session_type, FramedWrite::new(w, P2PCodec, ctx))
});
```


## API

### Incoming: Others -> Peers Manager

These are the messages supported by the peers manager handlers:

| Message          | Input type            | Output type                       | Description            |
| ---------------- | --------------------- | --------------------------------- | ---------------------- |
| `GetPeers`       | `()`                  | `()`                              | Empty                  |

#### GetPeers
The handler of `GetPeers` message is currently empty.

// TODO Update documentation when `GetPeers` gets any actual functionality.

### Outgoing messages: Sessions Manager -> Others

These are the messages sent by the connections manager:

| Message                 | Destination               | Input type                                        | Output type                       | Description                           |
|-------------------------|---------------------------|---------------------------------------------------|-----------------------------------|---------------------------------------|
| `Register`              | `SessionsManager`         | `SocketAddr, Addr<Session>, SessionType`          | `SessionsResult<()>`              | Request to register a new session     |
| `Unregister`            | `SessionsManager`         | `SocketAddr, SessionType`                         | `SessionsResult<()>`              | Request to unregister a session       |


#### Register

This message is sent to the [`SessionsManager`][sessions_manager] actor when the session 
actor is started to register this session.

The returned value is a `Result` for easy verification of the success of the operation.

For further information, see [`SessionsManager`][sessions_manager].

#### Unregister
 
This message is sent to the [`SessionsManager`][sessions_manager] actor when the session
actor is stopped to unregister this session.

The returned value is a `Result` for easy verification of the success of the operation.

For further information, see [`SessionsManager`][sessions_manager].

## Further information
The full source code of the `Session` actor can be found at [`session.rs`][session].

[sessions_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/sessions_manager.rs
[session]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/session.rs