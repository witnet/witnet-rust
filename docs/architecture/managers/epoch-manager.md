# Epoch Manager

The __epoch manager__ is the actor that handles the logic related to epochs:
it knows the current epoch based on the current time and the timestamp of
checkpoint zero (the start of epoch zero).

The current epoch can be calculated as:

```
(current_timestamp - checkpoint_zero_timestamp) / checkpoint_period
```

## State

The state of the actor contains the values needed to determine the current
epoch.

```rust
/// Epoch manager actor
pub struct EpochManager {
    checkpoint_zero_timestamp: Option<i64>,
    checkpoints_period: Option<u16>,
}
```

## Actor creation and registration

The creation of the epoch manager actor and its registration into the system registry are
performed directly by the main process [`node.rs`][noders]:

```rust
let epoch_manager_addr = EpochManager::default().start();
System::current().registry().set(epoch_manager_addr);
```

## API

### Incoming: Others -> Epoch Manager

These are the messages supported by the epoch manager handlers:

| Message        | Input type            | Output type                       | Description                    |
| -------------- | --------------------- | --------------------------------- | ------------------------------ |
| `GetEpoch`     | `()`                  | `EpochResult<Epoch>`              | Returns the current epoch id (last checkpoint) |

These messages are simple wrappers to methods in `EpochManager`, for example the `GetEpoch`
message wraps the `current_epoch()` method:

```rust
fn handle(&mut self, _msg: GetEpoch, _ctx: &mut Self::Context) -> EpochResult<Epoch> {
    let r = self.current_epoch();
    debug!("Current epoch: {:?}", r);
    r
}
```

The `EpochResult` type is just a wrapper around a result with an
`EpochManagerError`.

```rust
pub type EpochResult<T> = Result<T, EpochManagerError>;
```

The `EpochManagerError` is defined as:

```rust
pub enum EpochManagerError {
    /// Epoch zero time is unknown
    UnknownEpochZero,
    /// Checkpoint period is unknown
    UnknownCheckpointPeriod,
    /// Checkpoint zero is in the future
    CheckpointZeroInTheFuture,
    /// Overflow when calculating the epoch timestamp
    Overflow,
}
```

The way other actors will communicate with the epoch manager is:

1. Get the address of the manager from the registry:

```rust
// Get epoch manager address
let epoch_manager_addr = System::current().registry().get::<EpochManager>();
```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to send a message to the actor:

```rust
// Example
epoch_manager_addr
    .send(GetEpoch)
    .into_actor(self)
    .then(|res, _act, _ctx| {
        match res {
            Ok(res) => {
                // Process EpochResult
                println!("GetEpoch returned {:?}", res)
            },
            _ => println!("Something went really wrong in the actors message passing")
        };
        actix::fut::ok(())
    })
    .wait(ctx);
```

### Outgoing messages: Epoch Manager -> Others

These are the messages sent by the epoch manager:

| Message     | Destination       | Input type                                | Output type                 | Description                               |
| ----------- | ----------------- | ----------------------------------------- | --------------------------- | ----------------------------------------- |
| `GetConfig` | `ConfigManager`   | `()`                                      | `Result<Config, io::Error>` | Request the configuration                 |

#### GetConfig

This message is sent to the [`ConfigManager`][config_manager] actor when the epoch manager actor is started.

The return value is used to initialize the protocol constants (checkpoint period and
epoch zero timestamp).
For further information, see [`ConfigManager`][config_manager].

## Further information

The full source code of the `EpochManager` can be found at [`epoch_manager.rs`][epoch_manager].

[epoch_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/epoch_manager.rs
[config_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/config_manager.rs
[noders]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/node.rs
