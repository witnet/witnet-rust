# Storage Manager

The __storage manager__ is the actor that encapsulates the logic of the __persistent storage__
library. 

## State

The state of the actor is an instance of the [`RocksStorage`][rocks] backend encapsulated in an
option.

```rust
/// Storage manager actor
#[derive(Default)]
pub struct StorageManager {
    /// DB storage
    storage: Option<RocksStorage>,
}
```

The connection to the database is an `Option` to handle failures in the creation of the connection
to the database.

## Actor creation and registration

The creation of the storage manager actor is performed directly by the `main` process:

```rust
let storage_manager_addr = StorageManager::new(&db_root).start();
System::current().registry().set(storage_manager_addr);
```

The `new()` method tries to connect to the database specified in the path given as argument. If the
connection is not possible for any reason, the storage in the state will be `None`. Otherwise, the
state will contain the handle to the database for future use.

Once the storage manager actor is started, the `main` process registers the actor into the system
registry.

## API
 
### Incoming messages: Others -> Storage manager
 
These are the messages supported by the storage manager handlers:

| Message   | Input type                                | Output type                           | Description                               |
|-----------|-------------------------------------------|---------------------------------------|-------------------------------------------|
| Get       | `&'static [u8]`                           | `StorageResult<Option<Vec<u8>>>`      | Wrapper to RocksStorage `get()` method    |
| Put       | `&'static [u8]`, `Vec<u8>`                | `StorageResult<()>`                   | Wrapper to RocksStorage `put()` method    |
| Delete    | `&'static [u8]`                           | `StorageResult<()>`                   | Wrapper to RocksStorage `delete()` method |

The handling of these messages is basically just calling the corresponding method from the [`Storage`][storage]
trait that is implemented by [`RocksStorage`][rocks]. For example, the handler of the `Get` message
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

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to
send a message to the actor:
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
    The values used as keys for the storage need to be defined with the `static` lifetime.
    Literals can be a good choice for this purpose:
    ```rust
    pub static PEERS_KEY: &'static [u8] = b"peers";
    ```

### Outgoing messages: Storage manager -> Others

These are the messages sent by the storage manager:

| Message           | Destination   | Input type    | Output type                        | Description                          |
|-------------------|---------------|---------------|------------------------------------|--------------------------------------|
| GetConfig       | ConfigManager      | `()`  | `Result<Config, io::Error>` | Request config info     |

#### GetConfig

This message is sent to the [`ConfigManager`][config_manager] actor when the storage manager actor
is started.

The return value is used to launch the rocks db storage. For further information, see
[`ConfigManager`][config_manager].

## Further information
The full source code of the `StorageManager` can be found at [`storage_manager.rs`][storage_manager].

[storage_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/storage_manager.rs
[storage]: https://github.com/witnet/witnet-rust/blob/master/storage/src/storage.rs
[rocks]: https://github.com/witnet/witnet-rust/blob/master/storage/src/backends/rocks.rs
[config_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/config_manager.rs
