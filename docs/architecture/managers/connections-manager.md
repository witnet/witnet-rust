# Connections Manager

The __connections manager__ is the actor in charge of providing:

- A **TCP server** bound to the address indicated by the configuration file 
- As many **TCP clients** as requested, connected to the addresses requested by the
[`Sessions Manager`][sessions_manager]

## State

The `Connections Manager` actor has no proper state.

```rust
/// Connections manager actor
#[derive(Default)]
pub struct ConnectionsManager;
```

## Actor creation and registration

The creation of the connections manager actor and its registration into the system registry are
performed directly by the `main` process:

```rust
let connections_manager_addr = ConnectionsManager::default().start();
System::current().registry().set(connections_manager_addr);
```

## API
 
### Incoming messages: Others -> Connections Manager

These are the messages supported by the connections manager handlers:

| Message               | Input type    | Output type   | Description                                                       |
|-----------------------|---------------|---------------|-------------------------------------------------------------------|
| InboundTcpConnect     | `TcpStream`   | `()`          | Request to create a session from an incoming TCP connection       |
| OutboundTcpConnect    | `SocketAddr`  | `()`          | Request to create a start a TCP connection to a peer              |

The way other actors will communicate with the connections manager is:

1. Get the address of the connections manager from the registry:
```rust
// Get connections manager address
let connections_manager_addr = System::current().registry().get::<ConnectionsManager>();
```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to
send a message to the actor:
```rust
// Send a message to the connections manager
connections_manager_addr.do_send(OutboundTcpConnect { address });
```

#### InboundTcpConnect message

The `InboundTcpConnect` message is sent to the `ConnectionsManager` by the `ConnectionsManager` itself.

In the `started` method of the connections manager actor, the server address is requested from
the [`ConfigManager`][config_manager] actor and a TCP listener is created and bound to that address:

```rust
// Get address to launch the server
let server_address = "127.0.0.1:50005".parse().unwrap();

// Bind TCP listener to this address
let listener = TcpListener::bind(&server_address).unwrap();
```

For each incoming TCP connection that comes into the TCP listener, an `InboundTcpConnect` message is created from 
the TCP stream and sent to the actor:

```rust
// Add message stream which returns an InboundTcpConnect for each incoming TCP connection
ctx.add_message_stream(
    listener
        .incoming()
        .map_err(|_| ())
        .map(InboundTcpConnect::new),
);
```

When an `InboundTcpConnect` message arrives at the connections manager actor, the creation of a new
`Inbound` session is requested to the `SessionsManager`:

```rust
/// Method to handle the InboundTcpConnect message
fn handle(&mut self, msg: InboundTcpConnect, _ctx: &mut Self::Context) {
    // Request the creation of a new session actor from connection
    ConnectionsManager::request_session_creation(msg.stream, SessionType::Inbound);
}
```

#### OutboundTcpConnect message

The `OutboundTcpConnect` message is sent to the `ConnectionsManager` by the [`SessionsManager`][sessions_manager]. 

When an `OutboundTcpConnect` message arrives at the connections manager actor, several actions are
performed:

- Send a `ConnectAddr` message to the [`Resolver`][resolver] actor to connect to the requested peer
address
- Handle the result:
    - If error, do nothing but log it
    - If success, request the creation of an `Outbound` session to the `SessionsManager`
    
```rust
/// Method to handle the OutboundTcpConnect message
fn handle(&mut self, msg: OutboundTcpConnect, ctx: &mut Self::Context) {
    // Get resolver from registry and send a ConnectAddr message to it
    Resolver::from_registry()
        .send(ConnectAddr(msg.address))
        .into_actor(self)
        .then(|res, _act, _ctx| ConnectionsManager::process_connect_addr_response(res))
        .wait(ctx);
}
```

### Outgoing messages: Connections Manager -> Others

These are the messages sent by the connections manager:

| Message           | Destination       | Input type                | Output type                           | Description                           |
|-------------------|-------------------|---------------------------|---------------------------------------|---------------------------------------|
| GetConfig         | ConfigManager     | `()`                      | `Result<Config, io::Error>`           | Request the configuration             |
| ConnectAddr       | Resolver          | `SocketAddr`              | `Result<TcpStream, ResolverError>`    | Request a TCP conn to an address      | 
| Create            | SessionsManager   | `TcpStream, SessionType`  | `()`                                  | Request the creation of a session     | 

#### GetConfig 

This message is sent to the [`ConfigManager`][config_manager] actor when the connections manager actor
is started.

The return value is used to get the TCP server address of the Witnet node and launch it.

For further information, see [`ConfigManager`][config_manager].

#### ConnectAddr 

This message is sent to the [`Resolver`][resolver] actor when an `OutboundTcpConnect` message is received.

Upon reception of this message, the `Resolver` tries to open a TCP connection to the address specified
in the message.

For further information, see [`Resolver`][resolver].

#### Create

This message is sent to the [`SessionsManager`][sessions_manager] actor when a TCP connection is
established to request the creation of a session.

For further information, see [`SessionsManager`][sessions_manager].


## Further information
The full source code of the `ConnectionsManager` can be found at [`connections_manager.rs`][connections_manager].

[connections_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/connections_manager.rs
[sessions_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/sessions_manager.rs
[config_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/config_manager.rs
[resolver]: https://actix.rs/actix/actix/actors/resolver/index.html