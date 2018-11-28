# Blocks Manager

The __blocks manager__ is the actor in charge of managing the blocks of the Witnet blockchain. Among its responsabilities are the following:

* Initializing the chain info upon running the node for the first time and persisting it into storage (see **Storage Manager**).
* Recovering the chain info from storage and keeping it in its state.
* Validating block candidates as they come from a session (see **Sessions Manager**).
* Consolidating multiple block candidates for the same checkpoint into a single valid block.
* Putting valid blocks into storage by sending them to the storage manager actor.
* Having a method for letting other components to get blocks by *hash* or *checkpoint*.
* Having a method for letting other components get the epoch of the current tip of the blockchain (e.g. last epoch field required for the handshake in the Witnet network protocol).

## State

The state of the actor is an instance of the [`ChainInfo`][chain] data structures.

```rust
/// BlocksManager actor
#[derive(Default)]
pub struct BlocksManager {
    /// Blockchain information data structure
    chain_info: Option<ChainInfo>,
}
```

## Actor creation and registration

The creation of the blocks manager actor and its registration into the system registry are
performed directly by the main process [`node.rs`][noders]:

```rust
let blocks_manager_addr = BlocksManager::default().start();
System::current().registry().set(blocks_manager_addr);
```

## API

### Incoming: Others -> BlocksManager

These are the messages supported by the `BlocksManager` handlers:

| Message                                   | Input type                    | Output type              | Description                                    |
|-------------------------------------------|-------------------------------|--------------------------| -----------------------------------------------|
| `EpochNotification<EpochPayload>`         | `Epoch`, `EpochPayload`       | `()`                     | The requested epoch has been reached           |
| `EpochNotification<EveryEpochPayload>`    | `Epoch`, `EveryEpochPayload`  | `()`                     | A new epoch has been reached                   |
| `GetHighestBlockCheckpoint`               | `()`                          | `ChainInfoResult`        | Request a copy of the highest block checkpoint |
| `AddNewBlock`                             | `Block`                       | `Result<Hash, BlocksManagerError>` | Add a new block and announce it to other sessions |

Where `ChainInfoResult` is just:

``` rust
/// Result type for the ChainInfo in BlocksManager module.
pub type ChainInfoResult<T> = WitnetResult<T, ChainInfoError>;
```

The way other actors will communicate with the BlocksManager is:

1. Get the address of the BlocksManager actor from the registry:

    ```rust
    // Get BlocksManager address
    let blocks_manager_addr = System::current().registry().get::<BlocksManager>();
    ```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to send a message to the actor:

    ```rust
    config_manager_addr
        .send(GetHighestBlockCheckpoint)
        .into_actor(self)
        .then(|res, _act, _ctx| {
            // Process the response from BlocksManager
            process_get_config_response(res)
        })
        .and_then(|checkpoint, _act, ctx| {
            // Do something with the checkpoint
            actix::fut::ok(())
        })
        .wait(ctx);
    ```

For the time being, the handlers for Epoch messages just print a debug message with the notified
checkpoint. 

```rust
fn handle(&mut self, msg: EpochNotification<EpochPayload>, _ctx: &mut Context<Self>) {
    debug!("Epoch notification received {:?}", msg.checkpoint);
}
```
### Outgoing messages: BlocksManager -> Others

These are the messages sent by the blocks manager:

| Message           | Destination       | Input type                                    | Output type                 | Description                       |
|-------------------|-------------------|-----------------------------------------------|-----------------------------|-----------------------------------|
| `SubscribeEpoch`  | `EpochManager`    | `Epoch`, `Addr<BlocksManager>, EpochPayload`  | `()`                        | Subscribe to a particular epoch   |
| `SubscribeAll`    | `EpochManager`    | `Addr<BlocksManager>, EveryEpochPayload`      | `()`                        | Subscribe to all epochs           |
| `GetConfig`       | `ConfigManager`   | `()`                                          | `Result<Config, io::Error>` | Request the configuration         |
| `Get`             | `StorageManager`  | `&'static [u8]`                               | `StorageResult<Option<T>>`  | Wrapper to Storage `get()` method |
| `Put`             | `StorageManager`  | `&'static [u8]`, `Vec<u8>`                    | `StorageResult<()>`         | Wrapper to Storage `put()` method |
| `Broadcast<AnnounceItems>` | `SessionsManager` | `Vec<InvItems>`                      | `()`                        | Announce a new block to the sessions |

#### SubscribeEpoch

This message is sent to the [`EpochManager`][epoch_manager] actor when the `BlocksManager` actor is
started, in order to subscribe to the next epoch (test functionality).

Subscribing to the next epoch means that the [`EpochManager`][epoch_manager] will send an
`EpochNotification<EpochPayload>` back to the `BlocksManager` when the epoch is reached.

For further information, see [`EpochManager`][epoch_manager].

#### SubscribeAll

This message is sent to the [`EpochManager`][epoch_manager] actor when the blocks manager actor is
started, in order to subscribe to the all epochs (test functionality).

Subscribing to all epochs means that the [`EpochManager`][epoch_manager] will send an
`EpochNotification<EveryEpochPayload>` back to the `BlocksManager` when every epoch is reached.

For further information, see [`EpochManager`][epoch_manager].

#### GetConfig

This message is sent to the [`ConfigManager`][config_manager] actor when the peers manager actor is started.

The return value is used to initialize the list of known peers. For further information, see  [`ConfigManager`][config_manager].

#### Get

This message is sent to the [`StorageManager`][storage_manager] actor when the blocks manager actor is started.

The return value is a `ChainInfo` structure from the storage which are added to the state of the actor.

#### Put

This message is sent to the [`StorageManager`][storage_manager] actor to persist the `ChainInfo` structure

The return value is used to check if the storage process has been successful.

#### Broadcast<AnnounceItems>

This message is sent to the [`SessionsManager`][sessions_manager] actor which will
broadcast a `AnnounceItems` message to the open outbound sessions.

## Further information

The full source code of the `BlocksManager` can be found at [`blocks_manager.rs`][blocks_manager].

[blocks_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/blocks_manager/mod.rs
[storage_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/storage_manager/mod.rs
[sessions_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/sessions_manager/mod.rs
[epoch_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/epoch_manager/mod.rs
[noders]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/node.rs
[chain]: https://github.com/witnet/witnet-rust/tree/master/data_structures/src/chain.rs
