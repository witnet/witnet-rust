# Chain Manager

The __Chain Manager__ is the actor in charge of managing the blocks of managing
the blocks and transactions of the Witnet blockchain received through the protocol,
and also encapsulates the logic of the _unspent transaction outputs_.

Among its responsabilities are the following:

* Initializing the chain info upon running the node for the first time and persisting it into storage (see **Storage Manager**).
* Recovering the chain info from storage and keeping it in its state.
* Validating block candidates as they come from a session (see **Sessions Manager**).
* Consolidating multiple block candidates for the same checkpoint into a single valid block.
* Putting valid blocks into storage by sending them to the storage manager actor.
* Having a method for letting other components to get blocks by *hash* or *checkpoint*.
* Having a method for letting other components get the epoch of the current tip of the blockchain (e.g. last epoch field required for the handshake in the Witnet network protocol).
* Validating transactions as they come from any [Session](actors::session::Session). This includes:
  * Iterating over its inputs, adding the value of the inputs to calculate the value of the transaction.
  * Running the output scripts, expecting them all to return `TRUE` and leave an empty stack.
  * Verifying that the sum of all inputs is greater than or equal to the sum of all the outputs.
* Keeping valid transactions into memory. This in-memory transaction pool is what we call the _mempool_. Valid transactions are immediately appended to the mempool.
* Keeping every unspent transaction output (UTXO) in the block chain in memory. This is called the _UTXO set_.
* Updating the UTXO set with valid transactions that have already been anchored into a valid block. This includes:
  * Removing the UTXOs that the transaction spends as inputs.
  * Adding a new UTXO for every output in the transaction.

## State

The state of the actor is an instance of the [`ChainInfo`][chain] data structures.

```rust
/// ChainManager actor
#[derive(Default)]
pub struct ChainManager {
    /// Blockchain information data structure
    chain_info: Option<ChainInfo>,
    /// Map that relates an epoch with the hashes of the blocks for that epoch
    // One epoch can have more than one block
    epoch_to_block_hash: HashMap<Epoch, HashSet<Hash>>,
    /// Map that stores blocks by their hash
    blocks: HashMap<Hash, Block>,
    /// Current Epoch
    current_epoch: Option<Epoch>,
    /// Block candidate to update chain_info in the next epoch
    block_candidate: Option<Block>,
}
```

## Actor creation and registration

The creation of the Chain Manager actor and its registration into the system registry are
performed directly by the main process [`node.rs`][noders]:

```rust
let chain_manager_addr = ChainManager::default().start();
System::current().registry().set(chain_manager_addr);
```

## API

### Incoming: Others -> ChainManager

These are the messages supported by the `ChainManager` handlers:

| Message                                | Input type                   | Output type                       | Description                                       |
| -------------------------------------- | ---------------------------- | --------------------------------- | ------------------------------------------------- |
| `EpochNotification<EpochPayload>`      | `Epoch`, `EpochPayload`      | `()`                              | The requested epoch has been reached              |
| `EpochNotification<EveryEpochPayload>` | `Epoch`, `EveryEpochPayload` | `()`                              | A new epoch has been reached                      |
| `GetHighestBlockCheckpoint`            | `()`                         | `ChainInfoResult`                 | Request a copy of the highest block checkpoint    |
| `AddNewBlock`                          | `Block`                      | `Result<(), ChainManagerError>` | Add a new block and announce it to other sessions |
| `BuildBlock`                           | `CheckpointBeacon`,`LeadershipProof` | `()`                    | Build a new block and add it |

Where `ChainInfoResult` is just:

``` rust
/// Result type for the ChainInfo in ChainManager module.
pub type ChainInfoResult<T> = WitnetResult<T, ChainInfoError>;
```

The way other actors will communicate with the ChainManager is:

1. Get the address of the ChainManager actor from the registry:

    ```rust
    // Get ChainManager address
    let chain_manager_addr = System::current().registry().get::<ChainManager>();
    ```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to send a message to the actor:

    ```rust
    chain_manager_addr
        .send(GetHighestBlockCheckpoint)
        .into_actor(self)
        .then(|res, _act, _ctx| {
            // Process the response from ChainManager
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

### Outgoing messages: ChainManager -> Others

These are the messages sent by the Chain Manager:

| Message                    | Destination         | Input type                                  | Output type                         | Description                                    |
| -------------------------- | ------------------- | ------------------------------------------- | ----------------------------------- | ---------------------------------------------- |
| `SubscribeEpoch`           | `EpochManager`      | `Epoch`, `Addr<ChainManager>, EpochPayload` | `()`                                | Subscribe to a particular epoch                |
| `SubscribeAll`             | `EpochManager`      | `Addr<ChainManager>, EveryEpochPayload`     | `()`                                | Subscribe to all epochs                        |
| `GetConfig`                | `ConfigManager`     | `()`                                        | `Result<Config, io::Error>`         | Request the configuration                      |
| `Get`                      | `StorageManager`    | `&'static [u8]`                             | `StorageResult<Option<T>>`          | Wrapper to Storage `get()` method              |
| `Put`                      | `StorageManager`    | `&'static [u8]`, `Vec<u8>`                  | `StorageResult<()>`                 | Wrapper to Storage `put()` method              |
| `Broadcast<AnnounceItems>` | `SessionsManager`   | `Vec<InventoryEntry>`                       | `()`                                | Announce new inventory entries to the sessions |
| `ValidatePoE`              | `ReputationManager` | `CheckpointBeacon`,`LeadershipProof`        | `bool`                              | Request Proof of Eligibility validation        |
| `AddItem`                  | `InventoryManager`  | `InventoryItem`                             | `Result<(), InventoryManagerError>` | Persist the `block_candidate`                  |

#### SubscribeEpoch

This message is sent to the [`EpochManager`][epoch_manager] actor when the `ChainManager` actor is
started, in order to subscribe to the next epoch (test functionality).

Subscribing to the next epoch means that the [`EpochManager`][epoch_manager] will send an
`EpochNotification<EpochPayload>` back to the `ChainManager` when the epoch is reached.

For further information, see [`EpochManager`][epoch_manager].

#### SubscribeAll

This message is sent to the [`EpochManager`][epoch_manager] actor when the Chain Manager actor is
started, in order to subscribe to the all epochs (test functionality).

Subscribing to all epochs means that the [`EpochManager`][epoch_manager] will send an
`EpochNotification<EveryEpochPayload>` back to the `ChainManager` when every epoch is reached.

For further information, see [`EpochManager`][epoch_manager].

#### GetConfig

This message is sent to the [`ConfigManager`][config_manager] actor when the peers manager actor is started.

The return value is used to initialize the list of known peers. For further information, see  [`ConfigManager`][config_manager].

#### Get

This message is sent to the [`StorageManager`][storage_manager] actor when the Chain Manager actor is started.

The return value is a `ChainInfo` structure from the storage which are added to the state of the actor.

#### Put

This message is sent to the [`StorageManager`][storage_manager] actor to persist the `ChainInfo` structure

The return value is used to check if the storage process has been successful.

#### Broadcast<AnnounceItems>

This message is sent to the [`SessionsManager`][sessions_manager] actor which will
broadcast a `AnnounceItems` message to the open outbound sessions.

#### ValidatePoE

This message is sent to the [`ReputationManager`][reputation_manager] actor to request a
Proof of Eligibility validation for the `BlockHeaderWithProof` sent.

#### AddItem

This message is sent to the [`InventoryManager`][inventory_manager] actor as a `InventoryItem`
to persist the `block_candidate` state.

## Further information

The full source code of the `ChainManager` can be found at [`chain_manager.rs`][chain_manager].

[chain_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/chain_manager
[storage_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/storage_manager
[sessions_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/sessions_manager
[epoch_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/epoch_manager
[reputation_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/reputation_manager
[inventory_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/inventory_manager

[noders]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/node.rs
[chain]: https://github.com/witnet/witnet-rust/tree/master/data_structures/src/chain.rs
