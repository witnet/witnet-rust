# Session

__Session__ is the actor that encapsulates the entire business logic of the Witnet network protocol.



## Actor creation and registration

The creation of the session actor is performed by the [`SessionsManager`][sessions_manager] actor
upon reception of a `Create` message:

```rust
// Create a session actor
Session::create(move |ctx| {
    // Get local peer address
    let local_addr = msg.stream.local_addr().unwrap();

    // Get remote peer address
    let remote_addr = msg.stream.peer_addr().unwrap();

    // Split TCP stream into read and write parts
    let (r, w) = msg.stream.split();

    // Add stream in session actor from the read part of the tcp stream
    Session::add_stream(FramedRead::new(r, P2PCodec), ctx);

    // Create the session actor and store in its state the write part of the tcp stream
    Session::new(
        local_addr,
        remote_addr,
        msg.session_type,
        FramedWrite::new(w, P2PCodec, ctx),
        handshake_timeout,
    )
});
```


## API

### Incoming: Others -> Session 

These are the messages supported by the session handlers:

| Message          | Input type            | Output type                       | Description            |
| ---------------- | --------------------- | --------------------------------- | ---------------------- |
| `GetPeers`       | `()`                  | `()`                              | Empty                  |

#### GetPeers
The handler of `GetPeers` message is currently empty.

// TODO Update documentation when `GetPeers` gets any actual functionality.


### Outgoing messages: Session -> Others

These are the messages sent by the session:

| Message                 | Destination               | Input type                                        | Output type                         | Description                           |
|-------------------------|---------------------------|---------------------------------------------------|-------------------------------------|---------------------------------------|
| `Register`              | `SessionsManager`         | `SocketAddr, Addr<Session>, SessionType`          | `SessionsResult<()>`                | Request to register a new session     |
| `Unregister`            | `SessionsManager`         | `SocketAddr, SessionType, SessionStatus`          | `SessionsResult<()>`                | Request to unregister a session       |


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