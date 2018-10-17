# Storage Manager

The __storage manager__ is the actor that wraps up the logic of the __persistent storage__ library. 

There will be one storage manager actor per system and it will be registered at the system
registry. This way, any other actor will be able to get the address of the storage manager and
send messages to it.

## State

The state of the actor is just the connection to the `RocksDB` storage backend ([`rocks.rs`][rocks])
wrapped up in an option.

```rust
/// Storage manager actor
#[derive(Default)]
pub struct StorageManager {
    /// DB storage
    storage: Option<RocksStorage>,
}
```

The `StorageManager` actor requires the implementation of the `Default` trait (as well as
`Supervised` and `SystemService` traits) to become a service that can be registered in the system
registry.

The connection to the database is an `Option` to handle failures in the creation of the connection
to the database.

## Actor creation and registration

The creation of the storage manager actor is performed directly by the `main` process:

```rust
let storage_manager_addr = StorageManager::new(&db_root).start();
```

The `new()` method tries to connect to the database specified in the path given as argument. If the
connection is not possible for any reason, the storage in the state will be `None`. Otherwise, the
state will contain the handle to the database for future use.

Once the storage manager actor is started, the `main` process registers the actor in the system
registry:

```rust
System::current().registry().set(storage_manager_addr);
```

## API
 
### Incoming messages: Others -> Storage manager
 
These are the messages supported by the storage manager handlers:

| Message   | Inputs                                    | Outputs                               | Description                               |
|-----------|-------------------------------------------|---------------------------------------|-------------------------------------------|
| Get       | `key: &'static [u8]`                      | `StorageResult<Option<Vec<u8>>>`      | Wrapper to RocksStorage `get()` method    |
| Put       | `key: &'static [u8], value: Vec<u8>`      | `StorageResult<()>`                   | Wrapper to RocksStorage `put()` method    |
| Delete    | `key: &'static [u8]`                      | `StorageResult<()>`                   | Wrapper to RocksStorage `delete()` method |

The handling of these messages is basically just calling the corresponding method from the [`Storage`][storage]
trait that is implemented by the `RocksStorage`. For example, the handler of the `Get` message
would be implemented as:

```rust
/// Handler for Get message.
impl Handler<Get> for StorageManager {
    type Result = StorageResult<Option<Vec<u8>>>;

    fn handle(&mut self, msg: Get, _: &mut Context<Self>) -> Self::Result {
        self.storage.as_ref().unwrap().get(msg.key)
    }
}
```

Being the `StorageManager` such a simple actor, there are no errors that can arise due to its own
logic and thus, returning the `StorageResult` library generic error may be the right thing to do.

The way other actors will communicate with the storage manager is:

1. Get the address of the storage manager from the registry:
```rust
// Get storage manager address
let storage_manager_addr = System::current().registry().get::<StorageManager>();
```

2. Use the methods that the address offers to send a message to the actor (`do_send()`,
`try_send()`, `send()`):
```rust
// Example 
storage_manager_addr
    .send(Get{key: PEERS_KEY})
    .into_actor(self)
    .then(|res, _act, _ctx| {
        match res {
            Ok(res) => {
                // Process StorageResult
                match res {
                    Ok(opt) => {
                        // Process Option<Vec<u8>>
                        match opt {
                            Some(vec) => println!("PEERS_KEY found in storage, value: {:?}", vec),
                            None => println!("PEERS_KEY not found in storage")
                        };
                    },
                    Err(_) => println!("Something went wrong when accessing the storage")
                };
            },
            _ => println!("Something went really wrong in the actors message passing")
        };
        actix::fut::ok(())
    })
    .wait(ctx);
```

!!! warning
    The keys of the storage need to be defined with the `static` lifetime. Literals can be a good
    choice to achieve this purpose:
    ```rust
    pub static PEERS_KEY: &'static [u8] = b"peers";
    ```

### Outgoing messages: Storage manager -> Others

The storage manager is quite a simple wrapper over the storage library and it does not need to
start a communication with other actors in order to perform its functions.

## Further information
The full source code of the `StorageManager` can be found at [`storage_manager.rs`][storage_manager].

[storage_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/storage_manager.rs
[storage]: https://github.com/witnet/witnet-rust/blob/master/storage/src/storage.rs
[rocks]: https://github.com/witnet/witnet-rust/blob/master/storage/src/backends/rocks.rs
