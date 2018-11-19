# Blocks Manager

The __blocks manager__ is the actor in charge of managing the blocks of the Witnet blockchain. Among its responsabilities lie the following:

* Initializing the chain info upon running the node for the first time and persisting it into storage (see **Storage Manager**).
* Recovering the chain info from storage and keeping it in its state.
* Validating block candidates as they come from a session (see **Sessions Manager**).
* Consolidating multiple block candidates for the same checkpoint into a single valid block.
* Putting valid blocks into storage by sending them to the storage manager actor.
* Having a method for letting other components to get blocks by *hash* or *checkpoint*.
* Having a method for letting other components to get the epoch of the current tip of the blockchain (e.g. last epoch field required for the handshake in the Witnet network protocol)

## State

The blocks manager has no state for the time being.

```rust
/// Blocks manager actor
pub struct BlocksManager {}
```

## Actor creation and registration

The creation of the blocks manager actor and its registration into the system registry are
performed directly by the main process [`node.rs`][noders]:

```rust
let blocks_manager_addr = BlocksManager::default().start();
System::current().registry().set(blocks_manager_addr);
```

## API

### Outgoing messages: Blocks Manager -> Others

These are the messages sent by the blocks manager:

| Message           | Destination       | Input type                                    | Output type   | Description                       |
|-------------------|-------------------|-----------------------------------------------|---------------|-----------------------------------| 
| `SubscribeEpoch`  | `EpochManager`    | `Epoch`, `Addr<BlocksManager>, EpochMessage`  | `()`          | Subscribe to a particular epoch   |  
| `SubscribeAll`    | `EpochManager`    | `Addr<BlocksManager>, PeriodicMessage`        | `()`          | Subscribe to all epochs           |

#### SubscribeEpoch

This message is sent to the [`EpochManager`][epoch_manager] actor when the blocks manager actor is
started, in order to subscribe to the next epoch (test functionality).

Subscribing to the next epoch means that the [`EpochManager`][epoch_manager] will send an
`EpochNotification<EpochMessage>` back to the `BlocksManager` when the epoch is reached.

For further information, see [`EpochManager`][epoch_manager].

#### SubscribeAll

This message is sent to the [`EpochManager`][epoch_manager] actor when the blocks manager actor is
started, in order to subscribe to the all epochs (test functionality).

Subscribing to all epochs means that the [`EpochManager`][epoch_manager] will send an
`EpochNotification<PeriodicMessage>` back to the `BlocksManager` when every epoch is reached.

For further information, see [`EpochManager`][epoch_manager].

### Incoming: Others -> Blocks Manager

These are the messages supported by the blocks manager handlers:

| Message                               | Input type                    | Output type   | Description                           |
|---------------------------------------|-------------------------------|---------------| --------------------------------------|
| `EpochNotification<EpochMessage>`     | `Epoch`, `EpochMessage`       | `()`          | The requested epoch has been reached  | 
| `EpochNotification<PeriodicMessage>`  | `Epoch`, `PeriodicMessage`    | `()`          | A new epoch has been reached          |

For the time being, the handlers for those message just print a debug message with the notified
checkpoint. 

```rust
fn handle(&mut self, msg: EpochNotification<EpochMessage>, _ctx: &mut Context<Self>) {
    debug!("Epoch notification received {:?}", msg.checkpoint);
}
```

## Further information

The full source code of the `BlocksManager` can be found at [`blocks_manager.rs`][blocks_manager].

[blocks_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/blocks_manager.rs
[epoch_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/epoch_manager.rs
[noders]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/node.rs
