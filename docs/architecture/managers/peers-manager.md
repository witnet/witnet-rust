# Peers Manager

The __peers manager__ is the actor that encapsulates the logic of the __peers__ library, defined
under the subcrate `witnet_p2p`. The library allows to manage a list of peers known to the Witnet
node.

There can only be one peers manager actor per system, and it must be registered into the system
registry. This way, any other actor can get the address of the peers manager and send messages to it.

## State

The state of the actor is an instance of the [`Peers`][peers] library, which contains a list of 
peers known to the Witnet node.

```rust
#[derive(Default)]
pub struct PeersManager {
    /// Known peers
    peers: Peers,
}
```

The `PeersManager` actor requires the implementation of the `Default` trait (as well as `Supervised`
and `SystemService` traits) to become a service that can be registered in the system registry.

## Actor creation and registration

The creation of the peers manager actor is performed directly by the `main` process:

```rust
let peers_manager_addr = PeersManager::default().start();
```

The `default()` method initializes the peers manager and its underlying peers data structure.

Once the peers manager actor is started, the `main` process registers the actor into the system
registry:

```rust
System::current().registry().set(storage_manager_addr);
```

## API

### Incoming: Others -> Peers Manager

These are the messages supported by the peers manager handlers:

| Message    | Input type            | Output type                       | Description           |
| ---------- | --------------------- | --------------------------------- | --------------------- |
| GetPeer    | `()`                  | `PeersResult<Option<SocketAddr>>` | Get random peer       |
| AddPeer    | `address: SocketAddr` | `PeersResult<Option<SocketAddr>>` | Add peer to list      |
| RemovePeer | `address: SocketAddr` | `PeersResult<Option<SocketAddr>>` | Remove peer from list |

The handling of these messages is basically just calling the corresponding methods from the [`Peers`][peers] library that is implemented by [`peers.rs`][peers]. For example, the handler of the `AddPeer` message would be implemented as:

```rust
/// Handler for Add peer message.
impl Handler<AddPeer> for PeersManager {
    type Result = PeersResult<Option<SocketAddr>>;

    fn handle(&mut self, msg: AddPeer, _: &mut Context<Self>) -> Self::Result {
        // Insert address
        debug!("Add peer handle for address {}", msg.address);
        self.peers.add(msg.address)
    }
}
```

Being the `PeersManager` such a simple actor, there are no errors that can arise due to its own
logic and thus, returning a `PeersResult` library generic error may be the right thing to do.

The way other actors will communicate with the storage manager is:

1. Get the address of the manager from the registry:

    ```rust
    // Get peers manager address
    let peers_manager_addr = System::current().registry().get::<PeersManager>();
    ```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to send a message to the actor:

    ```rust
    // Example
    peers_manager_addr
        .send(AddPeer { address })
        .into_actor(self)
        .then(|res, _act, _ctx| {
            match res {
                Ok(res) => {
                    // Process PeersResult
                    println!("Add peer {:?} returned {:?}", address, res)
                },
                _ => println!("Something went really wrong in the actors message passing")
            };
            actix::fut::ok(())
        })
        .wait(ctx);
    ```

### [WIP] Outgoing: Storage manager -> Others

Currently, the peers manager is quite a simple wrapper over the `peers` library and it does not need to
start a communication with other actors in order to perform its functions.

However, it is foreseen that in the future, the peers manager will call the `Storage Manager` in order to store a list of known peers.
This list might be used for initialization purposes of a future execution of the Witnet node.

## Further information

The full source code of the `PeersManager` can be found at [`peers_manager.rs`][peers_manager].

[peers]: https://github.com/witnet/witnet-rust/blob/master/p2p/src/peers.rs
[peers_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/peers_manager.rs