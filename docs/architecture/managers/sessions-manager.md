# Sessions Manager

The __sessions manager__ is the actor that handles incoming (inbound) and outgoing (outbound) sessions. These are some of
its responsibilities:

- Register / unregister new sessions
- Keep track of the status of the sessions
- Periodically check the number of outgoing connections. If less than the configured number of
outgoing peers, the sessions manager will:
    - Request a new peer address from the [`PeersManager`][peers_manager].
    - Send a message to the [`ConnectionsManager`][connections_manager] to request a new TCP
    connection to that peer.

The __sessions manager__ is the actor that encapsulates the logic of the __sessions__ library,
defined under the subcrate `witnet_p2p`. The library allows to manage the sessions collection
present at the Witnet node.

## State

The state of the `Sessions Manager` is an instance of the [`Sessions`][sessions] library,
which contains the collection of inbound and outbound sessions present at the Witnet node.

```rust
#[derive(Default)]
pub struct SessionsManager {
    // Registered sessions
    sessions: Sessions<Addr<Session>>,
}
```

The `Sessions` struct is generic over a type T, which is the type of the reference to the `Session`.
As the __actors__ paradigm is being used, this generic type T is the `Addr` of the `Session` actor. 

## Actor creation and registration

The creation of the sessions manager actor and its registration into the system registry are
performed directly by the `main` process:

```rust
let sessions_manager_addr = SessionsManager::default().start();
System::current().registry().set(sessions_manager_addr);
```

## API
 
### Incoming messages: Others -> Sessions Manager

These are the messages supported by the sessions manager handlers:

| Message       | Input type                                | Output type           | Description                               |
|---------------|-------------------------------------------|-----------------------|-------------------------------------------|
| Register      | `SocketAddr, Addr<Session>, SessionType`  | `SessionsResult<()>`  | Request to register a new session         |
| Unregister    | `SocketAddr, SessionType`                 | `SessionsResult<()>`  | Request to unregister a session           |
| Update        | `SocketAddr, SessionType, SessionStatus`  | `SessionsResult<()>`  | Request to update the status of a session |

The handling of these messages is basically just calling the corresponding methods from the
[`Sessions`][sessions] library. For example, the handler of the `Register` message would be
implemented as:

```rust
pub type SessionsUnitResult = SessionsResult<()>;

/// Handler for Register message.
impl Handler<Register> for SessionsManager {
    type Result = SessionsUnitResult;

    fn handle(&mut self, msg: Register, _: &mut Context<Self>) -> Self::Result {
        // Call method register session from sessions library
        self.sessions
            .register_session(msg.session_type, msg.address, msg.actor)
    }
}
```

Being the `SessionsManager` such a simple actor, there are no errors that can arise due to its own
logic and thus, returning just a `SessionsResult<()>` library generic error may be the right thing
to do.

The way other actors will communicate with the sessions manager is:

1. Get the address of the sessions manager from the registry:
```rust
// Get sessions manager address
let sessions_manager_addr = System::current().registry().get::<SessionsManager>();
```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to
send a message to the actor:
```rust
session_manager_addr
    .send(Register {
        address: self.address,
        actor: ctx.address(),
        session_type: self.session_type,
    })
    .into_actor(self)
    .then(|res, _act, ctx| {
        match res {
            Ok(Ok(_)) => debug!("Session successfully registered into the Session Manager"),
            _ => debug!("Session register into Session Manager failed")
        }
        actix::fut::ok(())
    })
    .wait(ctx);
```

### Outgoing messages: Connections Manager -> Others

These are the messages sent by the connections manager:

| Message               | Destination           | Input type    | Output type                       | Description                           |
|-----------------------|-----------------------|---------------|-----------------------------------|---------------------------------------|
| GetServerAddress      | ConfigManager         | `()`          | `Option<SocketAddr>`              | Request the config server address     |
| GetConnLimits         | ConfigManager         | `()`          | `Option<(u16, u16)>`              | Request the config connections limits |
| GetPeer               | PeersManager          | `()`          | `PeersResult<Option<SocketAddr>>` | Request the address of a peer         | 
| OutboundTcpConnect    | ConnectionsManager    | `SocketAddr`  | `()`                              | Request a TCP conn to an address      | 

#### GetServerAddress

This message is sent to the [`ConfigManager`][config_manager] actor when the sessions manager actor
is started.

The return value is stored at the [`Sessions`][sessions] state and used at the Witnet node to avoid
connections to itself.

For further information, see [`ConfigManager`][config_manager].

#### GetConnLimits
 
This message is sent to the [`ConfigManager`][config_manager] actor when the sessions manager actor
is started.

The return value is stored at the [`Sessions`][sessions] state and used to reject incoming
connections and to not request new outgoing connections once the configured limits have been
reached.

For further information, see [`ConfigManager`][config_manager].

#### GetPeer

This message is sent to the [`PeersManager`][peers_manager] actor when the sessions manager actor
detects that the number of outbound sessions registered is less than the configured limit. This
detection is done in a bootstrap periodic task.

The return value is then processed. If an error happened, nothing occurs. If the `PeersManager`
returned an address, then the `SessionsManager` checks if it is valid and if so, it sends an 
`OutboundTcpConnect` message to the `ConnectionsManager` to start a new TCP connection to that
address.

In this context, a __valid__ address means that:

- The address is not the own Witnet node's server address
- The address is not one of the already existing outbound connections  

For further information, see [`PeersManager`][peers_manager].
 
#### OutboundTcpConnect 

This message is sent to the [`ConnectionsManager`][connections_manager] actor when the sessions
manager receives a valid peer address from the `PeersManager`.

It is a best effort message, its return value is not processed and the sessions manager actor does
not even wait for its response after sending it.

If the operation was successful, the sessions manager will know it by other means (a session will be
created and registered into the `SessionsManager`). If the operation was not successful, then the
sessions manager will detect in its next periodic bootstrap task that there are no new outbound
connections and try to create a new one.

For further information, see [`ConnectionsManager`][connections_manager].

## Further information
The full source code of the `SessionsManager` can be found at [`sessions_manager.rs`][sessions_manager].

[connections_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/connections_manager.rs
[peers_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/peers_manager.rs
[sessions_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/sessions_manager.rs
[config_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/config_manager.rs
[sessions]: https://github.com/witnet/witnet-rust/blob/master/p2p/src/sessions/mod.rs
