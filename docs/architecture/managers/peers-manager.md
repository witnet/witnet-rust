# Peers Manager

The __peers manager__ is the actor that encapsulates the logic of the __peers__ library, defined
under the subcrate `witnet_p2p`. The library allows to manage a list of peers known to the Witnet
node.

## State

The state of the actor is an instance of the [`Peers`][peers] library, which contains a list of peers known to the Witnet node.

```rust
#[derive(Default)]
pub struct PeersManager {
    /// Known peers
    peers: Peers,
}
```

## Actor creation and registration

The creation of the peers manager actor and its registration into the system registry are
performed directly by the `main` process:

```rust
let peers_manager_addr = PeersManager::default().start();
System::current().registry().set(peers_manager_addr);
```

## API

### Incoming: Others -> Peers Manager

These are the messages supported by the peers manager handlers:

| Message        | Input type            | Output type                       | Description            |
| -------------- | --------------------- | --------------------------------- | ---------------------- |
| AddPeers       | `address: SocketAddr` | `PeersResult<Vec<SocketAddr>>`    | Add peers to list      |
| RemovePeers    | `address: SocketAddr` | `PeersResult<Vec<SocketAddr>>`    | Remove peers from list |
| GetRandomPeer  | `()`                  | `PeersResult<Option<SocketAddr>>` | Get random peer        |
| GetPeers       | `()`                  | `PeersResult<Vec<SocketAddr>>`    | Get all peers          |

The handling of these messages is basically just calling the corresponding methods from the
[`Peers`][peers] library that is implemented by [`peers.rs`][peers].
For example, the handler of the `AddPeers` message would be implemented as:

```rust
/// Handler for AddPeers message
impl Handler<AddPeers> for PeersManager {
    type Result = PeersSocketAddrsResult;

    fn handle(&mut self, msg: AddPeers, _: &mut Context<Self>) -> Self::Result {
        // Insert address
        debug!("Add peer handle for addresses: {:?}", msg.addresses);
        self.peers.add(msg.addresses)
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
        .send(AddPeers { addresses })
        .into_actor(self)
        .then(|res, _act, _ctx| {
            match res {
                Ok(res) => {
                    // Process PeersResult
                    println!("Add peer {:?} returned {:?}", addresses, res)
                },
                _ => println!("Something went really wrong in the actors message passing")
            };
            actix::fut::ok(())
        })
        .wait(ctx);
    ```

### Outgoing messages: Peers Manager -> Others

These are the messages sent by the peers manager:

| Message     | Destination       | Input type                                | Output type                 | Description                               |
| ----------- | ----------------- | ----------------------------------------- | --------------------------- | ----------------------------------------- |
| `GetConfig` | `ConfigManager`   | `()`                                      | `Result<Config, io::Error>` | Request the configuration                 |
| `Get`       | `StorageManager`  | `&'static [u8]`                           | `StorageResult<Option<T>>`  | Wrapper to Storage `get()` method         |
| `Put`       | `StorageManager`  | `&'static [u8]`, `Vec<u8>`                | `StorageResult<()>`         | Wrapper to Storage `put()` method         |

#### GetConfig

This message is sent to the [`ConfigManager`][config_manager] actor when the peers manager actor is started.

The return value is used to initialize the list of known peers. For further information, see  [`ConfigManager`][config_manager].

#### Get

This message is sent to the [`StorageManager`][storage_manager] actor when the peers manager actor is started.

The return value is a list of peers from the storage which are added to the list of known peers.

#### Put

This message is sent to the [`StorageManager`][storage_manager] actor periodically using a period
obtained from [`ConfigManager`][config_manager]

The return value is used to check if the storage process has been successful.

## Further information

The full source code of the `PeersManager` can be found at [`peers_manager.rs`][peers_manager].

[peers]: https://github.com/witnet/witnet-rust/blob/master/p2p/src/peers
[peers_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/peers_manager
[config_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/config_manager
[storage_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/storage_manager