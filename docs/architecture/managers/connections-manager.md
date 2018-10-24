# Connections Manager

The __connections manager__ is the actor in charge of providing:

- A **TCP server** bound to the address indicated by the configuration file 
- As many **TCP clients** as requested, connected to the addresses returned by the
[`Peers Manager`][peers_manager]

There can only be one connections manager actor per system, and it must be registered into the system
registry. This way, any other actor can get the address of the connections manager and send messages
to it.

## State

The `Connections Manager` actor has no proper state.

```rust
/// Connections manager actor
#[derive(Default)]
pub struct ConnectionsManager;
```

The `ConnectionsManager` actor requires the implementation of the `Default` trait (as well as
`Supervised` and `SystemService` traits) to become a service that can be registered in the system
registry.


## Actor creation and registration

The creation of the storage manager actor is performed directly by the `main` process:

```rust
let connections_manager_addr = ConnectionsManager::default().start();
```

Once the connections manager actor is started, the `main` process registers the actor into the system
registry:

```rust
System::current().registry().set(connections_manager_addr);
```

## API
 
### Incoming messages: Others -> Connections Manager

These are the messages supported by the connections manager handlers:

| Message               | Input type    | Output type   | Description                                                   |
|-----------------------|---------------|---------------|---------------------------------------------------------------|
| InboundTcpConnect     | `TcpStream`   | `()`          | Request to create a session from an incoming TCP connection   |
| OutboundTcpConnect    | `()`          | `()`          | Request to create a start a TCP connection to a peer          |

The way other actors will communicate with the connections manager is:

1. Get the address of the connections manager from the registry:
```rust
// Get connections manager address
let connections_manager_addr = System::current().registry().get::<ConnectionsManager>();
```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to
send a message to the actor:
```rust
// Example 
connections_manager_addr
    .send(OutboundTcpConnect)
    .into_actor(self)
    .then(|res, _act, _ctx| {
        actix::fut::ok(())
    })
    .wait(ctx);
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

When an `InboundTcpConnect` message arrives at the connections manager actor, a new session is 
created of `Server` type:

```rust
/// Method to handle the InboundTcpConnect message
fn handle(&mut self, msg: InboundTcpConnect, _ctx: &mut Self::Context) {
    // Create a session actor from connection
    ConnectionsManager::create_session(msg.stream, SessionType::Server);
}
```

#### OutboundTcpConnect message

The `OutboundTcpConnect` message is sent to the `ConnectionsManager` by other actors. 

When an `OutboundTcpConnect` message arrives at the connections manager actor, several actions are
performed:
- Send a message to the [`PeersManager`][peers_manager] actor to get the peer address
- Send a `ConnectAddr` message to the `Resolver` actor to connect to that peer address
- Handle the result:
    - If error, do nothing but log it
    - If success, create a session of `Client` type
    
```rust
/// Method to handle the OutboundTcpConnect message
fn handle(&mut self, _msg: OutboundTcpConnect, ctx: &mut Self::Context) {
    // Get peer address from peers manager
    // TODO query peer address from peers manager [23-10-2018]
    let address = "127.0.0.1:50004".parse().unwrap();

    info!("Trying to connect to peer {}...", address);

    // Get resolver from registry and send a ConnectAddr message to it
    Resolver::from_registry()
        .send(ConnectAddr(address))
        .into_actor(self)
        .map(move |res, _act, _ctx| match res {
            // Successful connection
            Ok(stream) => {
                info!("Connected to peer {}", address);

                // Create a session actor from connection
                ConnectionsManager::create_session(stream, SessionType::Client);
            }

            // Not successful connection
            Err(err) => {
                info!("Cannot connect to peer `{}`: {}", address, err);
            }
        })
        // Not successful connection
        .map_err(move |err, _act, _ctx| {
            info!("Cannot connect to peer `{}`: {}", address, err);
        })
        .wait(ctx);
}
```

### Outgoing messages: Connections Manager -> Others

These are the messages sent by the connections manager:

| Message           | Destination   | Input type    | Output type                       | Description                           |
|-------------------|---------------|---------------|-----------------------------------|---------------------------------------|
| GetServerAddress  | ConfigManager | `()`          | `Option<SocketAddr>`              | Request the config server address     |
| GetPeer           | PeersManager  | `()`          | `PeersResult<Option<SocketAddr>>  | Request the address of a peer         | 

#### GetServerAddress

This message is sent to the [`ConfigManager`][config_manager] actor when the connections manager actor is started.

The return value is used to launch the TCP server of the Witnet node. For further information, see 
[`ConfigManager`][config_manager].

#### GetPeer

This message is sent to the [`PeersManager`][peers_manager] actor when an `OutboundTcpConnect` message is received
at the connections manager actor.

The return value is used to start the TCP server of the Witnet node. For further information, see [`ConfigManager`][config_manager].

## Further information
The full source code of the `ConnectionsManager` can be found at [`connections_manager.rs`][connections_manager].

[peers_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/peers_manager.rs
[connections_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/connections_manager.rs
[config_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/config_manager.rs
