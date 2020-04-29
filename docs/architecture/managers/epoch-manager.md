# Epoch Manager

The __epoch manager__ is the actor that handles the logic related to epochs:
it knows the current epoch based on the current time and the timestamp of
checkpoint zero (the start of epoch zero) and allows other actors to
subscribe to specific checkpoints.

The current epoch can be calculated as:

```
(current_timestamp - checkpoint_zero_timestamp) / checkpoint_period
```

## State

The state of the actor contains the values needed to determine the current
epoch, as well as a list of subscriptions.

```rust
pub struct EpochManager {
    /// Checkpoint corresponding to the start of epoch #0
    checkpoint_zero_timestamp: Option<i64>,

    /// Period between checkpoints
    checkpoints_period: Option<u16>,

    /// Subscriptions to a particular epoch
    subscriptions_epoch: BTreeMap<Epoch, Vec<Box<dyn SendableNotification>>>,

    /// Subscriptions to all epochs
    subscriptions_all: Vec<Box<dyn SendableNotification>>,

    /// Last epoch that was checked by the epoch monitor process
    last_checked_epoch: Option<Epoch>,
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

### Incoming: Others -> EpochManager

These are the messages supported by the EpochManager handlers:

| Message          | Input type                             | Output type          | Description                                               |
|------------------|----------------------------------------|----------------------|-----------------------------------------------------------|
| `GetEpoch`       | `()`                                   | `EpochResult<Epoch>` | Returns the current epoch id (last checkpoint)            |
| `SubscribeEpoch` | `Epoch, Box<dyn SendableNotification>` | `()`                 | Subscribe to a specific checkpoint (the start that epoch) |
| `SubscribeAll`   | `Box<dyn SendableNotification>`        | `()`                 | Subscribe to all future checkpoints                       |

`SubscribeEpoch` and `SubscribeAll` are created using a helper function
as detailed in the section [subscribe](#subscribe-to-a-specific-checkpoint).
The `GetEpoch` message wraps the `current_epoch()` method:

```rust
fn handle(&mut self, _msg: GetEpoch, _ctx: &mut Self::Context) -> EpochResult<Epoch> {
    let r = self.current_epoch();
    log::debug!("Current epoch: {:?}", r);
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

#### Subscribe to a specific checkpoint

In order to subscribe to a specific epoch, the actors need the `epoch_manager_addr` and
the current epoch. For example to subscribe to the next checkpoint:

```rust
// The payload we send with `EpochNotification`
struct EpochPayload;

// Get the current epoch from `EpochManager`
// let epoch = ...

// Get the address of the current actor
let self_addr = ctx.address();

// Subscribe to the next epoch with an Update
epoch_manager_addr
    .do_send(Subscribe::to_epoch(
        Epoch(epoch.0 + 1),
        self_addr,
        EpochPayload,
    ));
```

The logic is implemented as an `EpochNotification<T>` handler, where
`T` is one specific payload.

```rust
/// Handler for EpochNotification<EpochPayload>
impl Handler<EpochNotification<EpochPayload>> for BlockManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EpochPayload>, _ctx: &mut Context<Self>) {
        log::debug!("Epoch notification received {:?}", msg.checkpoint);
    }
}
```

It is assumed that subscribing cannot fail. However, if the `EpochManager`
skips some checkpoints, all the missed notifications will be sent at the next
checkpoint but with the old requested checkpoint in the message.

The notifications are sent according to their checkpoint id: the oldest
checkpoints first.

#### Subscribe to all new checkpoints

In order to receive a notification on each checkpoint, the actors need
to subscribe with a cloneable payload. If an actor doesn't need a payload,
a type like `()` or an empty struct can be used.

```rust
#[derive(Clone)]
struct EveryEpochPayload;
// Subscribe to all epochs with a cloneable payload
epoch_manager_addr
    .do_send(Subscribe::to_all(
        self_addr,
        EveryEpochPayload,
    ));
```

The logic is implemented as an `EpochNotification<T>` handler, where
`T` is one specific payload.

```rust
/// Handler for EpochNotification<EveryEpochPayload>
impl Handler<EpochNotification<EveryEpochPayload>> for BlockManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EveryEpochPayload>, _ctx: &mut Context<Self>) {
        log::debug!("Periodic epoch notification received {:?}", msg.checkpoint);
    }
}
```

In case of skipped epochs, the notifications are lost.

### Outgoing messages: EpochManager -> Others

These are the messages sent by the EpochManager:

| Message                | Destination     | Input type | Output type                 | Description                                             |
|------------------------|-----------------|------------|-----------------------------|---------------------------------------------------------|
| `GetConfig`            | `ConfigManager` | `()`       | `Result<Config, io::Error>` | Request the configuration                               |
| `EpochNotification<T>` | *               | `Epoch, T` | `()`                        | A notification sent at the start of the requested epoch |

#### GetConfig

This message is sent to the [`ConfigManager`][config_manager] actor when the epoch manager actor is started.

The return value is used to initialize the protocol constants (checkpoint period and
epoch zero timestamp).
For further information, see [`ConfigManager`][config_manager].

#### EpochNotification<T>

This message is sent to all the actors which are subscribed to the epoch that just started.
There are two types of subscriptions:

* `SubscriptionEpoch` only sends the `EpochNotification` once.
* `SubscriptionAll` sends an `EpochNotification` at every checkpoint.

The `EpochNotification` is defined as:

```rust
#[derive(Message)]
pub struct EpochNotification<T: Send> {
    /// Epoch that has just started
    pub checkpoint: Epoch,

    /// Payload for the epoch notification
    pub payload: T,
}
```

Therefore it can be accessed in the message handler as:

```rust
let epoch = msg.checkpoint;
let payload = msg.payload;
```

## Further information

The full source code of the `EpochManager` can be found at [`epoch_manager.rs`][epoch_manager].

[epoch_manager]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/epoch_manager
[config_manager]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/config_manager
[noders]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/node.rs
