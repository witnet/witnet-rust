# Mining Manager

The __Mining manager__ is the actor which handles the logic related to
discovering our eligibility for mining new blocks and resolving data requests.

This manager is optional and can be disabled using a configuration flag in `witnet.toml`:

```
[mining]
enabled = false
```

## Actor creation and registration

The creation of the mining manager actor is performed directly by the main process [`node.rs`][noders]:

```rust
let mining_manager_addr = MiningManager::start_default();
System::current().registry().set(mining_manager_addr);
```

## API

### Incoming messages: Others -> mining manager

These are the messages supported by the inventory manager handlers:

| Message                                | Input type                                | Output type                           | Description                               |
|----------------------------------------|-------------------------------------------|---------------------------------------|-------------------------------------------|
| `EpochNotification<EveryEpochPayload>` | `Epoch`, `EveryEpochPayload`              | `()`                                  | A new epoch has been reached              |

`EpochNotification` starts the eligibility discovery process.

### Outgoing messages: inventory manager -> Others

These are the messages sent by the inventory manager:

| Message                      | Destination         | Input type                                  | Output type                        | Description                                   |
|------------------------------|---------------------|---------------------------------------------|------------------------------------|-----------------------------------------------|
| `GetConfig`                  | `ConfigManager`     | `()`                                        | `Result<Config, io::Error>`        | Request the configuration                     |
| `SubscribeAll`               | `EpochManager`      | `Addr<ChainManager>, EveryEpochPayload`     | `()`                               | Subscribe to all epochs                       |
| `GetHighestCheckpointBeacon` | `ChainManager`      | `()`                                        | `()`                               | Get the beacon of the top of the chain        |
| `ValidatePoE`                | `ReputationManager` | `CheckpointBeacon`,`LeadershipProof`        | `bool`                             | Request Proof of Eligibility validation       |
| `BuildBlock`                 | `ChainManager`      | `CheckpointBeacon`,`LeadershipProof`        | `()`                               | Build a new block                             |

#### GetConfig

This message is sent to the [`ConfigManager`][config_manager] actor when the mining manager actor is started.

The return value is used to decide whether or not start the mining manager.

#### SubscribeAll

This message is sent to the [`EpochManager`][epoch_manager] actor when the mining manager actor is
started, in order to subscribe to the all epochs.

Subscribing to all epochs means that the [`EpochManager`][epoch_manager] will send an
`EpochNotification<EveryEpochPayload>` back to the `MiningManager` when every epoch is reached.

For further information, see [`EpochManager`][epoch_manager].

#### GetHighestCheckpointBeacon

This message is sent to the [`ChainManager`][chain_manager] actor to request the
checkpoint beacon: the hash of the last valid block, and the current epoch.

#### ValidatePoE

This message is sent to the [`ReputationManager`][reputation_manager] actor to request a
Proof of Eligibility validation for the given `CheckpointBeacon` and `LeadershipProof`.

#### BuildBlock

This message is sent to the [`ChainManager`][chain_manager] actor which will build
a new block with the given `CheckpointBeacon` and `LeadershipProof`, and transactions
from the `ChainManager`. The new block will be validated and broadcasted.

[chain_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/chain_manager
[config_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/config_manager
[epoch_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/epoch_manager
[reputation_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/reputation_manager

[noders]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/node.rs
