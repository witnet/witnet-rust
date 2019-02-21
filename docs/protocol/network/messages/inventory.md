# Inventory Exchange

The inventory exchange protocol describes the interaction between nodes in order to synchronize themselves with the network and reestablish the full blockchain (in terms of blocks and transactions).

Inventory exchanges envision 2 main scenarios:

 1. **Block Download**, for nodes in need of synchronizing their blockchains by requesting and downloading blocks.

 2. **Inventory Broadcasting**, for nodes willing to broadcast transactional information, such as blocks and transactions.

## Block Download

**Block Download** is performed by nodes that need to catch up with other nodes by downloading missing blocks in order to reconstruct their blockchains. For example, this is the case of nodes connecting to the blockchain for the first time or after a significant amount of downtime.

Starting from the block #0 (the hardcoded genesis block), the nodes need to validate all blocks up to the current tip of the blockchain. Therefore, in case of missing blocks, a local node will initiate the following process of synchronization with its outbound peers:

 1. The local node will have already exchanged `Version` messages with its remote outbound peers. Those `Version` messages contain the last epoch known to others peers, i.e. the local peer can already compare how many blocks they each have and identify how many are missing.

 2. The local node will send a `LastBeacon` message to all its outbound nodes (after successful handshake protocol). These messages contain the hash of the top block of the local blockchain and the epoch of that block.

 3. Remote peers will reply by sending another `LastBeacon` message containing the hash of the top block of their respective blockchains and the epochs for those top blocks.

 4. The peer with the longest blockchain will identify which blocks are required by the other peer in order to allow it to synchronize to its blockchain. The peer will select up to the first consecutive 500 blocks and it will transmit their hashes using an `InventoryAnnouncement` message.

 5. After identifying which blocks are missing (it may already have some of them), the node may request them by using a `InventoryRequest` message, containing the hashes of the needed blocks. This message will help the node to catch up with the current full blockchain.

 6. After receiving the `InventoryRequest` message, the peer sends the requested blocks individually by using `Block` messages.

The following diagram depicts the previously described process under the assumption that the peer with the longest blockchain is `NodeB` (step 4).

```ascii
      NodeA                        NodeB
        +                            +
        |        LAST_BEACON         |
        +--------------------------->+
        |        LAST_BEACON         |
        +<---------------------------+
        |    INVENTORY_ANNOUNCEMENT  |
        +<---------------------------+
        |                            |
        |                            |
        |    INVENTORY_REQUEST       |
        +--------------------------->+
        |            BLOCK           |
        +<-------------------------- +
        |            BLOCK           |
        +<-------------------------- +
        |            BLOCK           |
        +<---------------------------+
        |                            |
        +                            +
```

## Inventory Broadcasting

Similarly to the previously described process of synchronization, any node may contribute to the synchronization of their outbound peers by advertising inventory objects such as blocks and transactions. Inventory broadcasting is also used in case a node creates transactions or mine blocks.

The inventory broadcasting can be described as the following sequence of steps:

 1. A remote node broadcasts its inventory by sending an `InventoryAnnouncement` message, containing all the hashes of the advertised inventory objects.

 2. After receiving an `InventoryAnnouncement` message and filtering the inventory objects that may be missing in the local blockchain, the local node sends a `InventoryRequest` message with the hashes of the needed objects.

 3. The remote note receives the `InventoryRequest` message and sends a `Block` or `Transaction` message per requested inventory object (identified by a hash).

The following diagram depicts the previous step under the assumption that the local node (`NodeA`) sends a `InventoryRequest` message requesting 3 blocks and 2 transactions.

```ascii
         NodeA                            NodeB
           +                                +
           |    INVENTORY_ANNOUNCEMENT      |
           +<-------------------------------+
           |                                |
           |        INVENTORY_REQUEST       |
           +------------------------------->+
           |                                |
           |             BLOCK              |
           +<-------------------------------+
           |             BLOCK              |
           +<-------------------------------+
           |             BLOCK              |
           +<-------------------------------+
           |          TRANSACTION           |
           +<-------------------------------+
           |          TRANSACTION           |
           +<-------------------------------+
           |                                |
           +                                +
```

## LastBeacon message

The `LastBeacon` messages are used in order to notify the hash of the highest known block by the peer together with its epoch.
After exchanging `LastBeacon` messages between peers, the one with the longest blockchain in terms of blocks will send an `InventoryAnnouncement` message to the other peer. This message will include the list of block hashes starting right after the last known block hash provided by the other peer.

The `LastBeacon` message consists of a message header with the `LastBeacon` command and a payload
containing the beacon for the tip of the chain (the hash of the latest block and its checkpoint) as
known to the local peer:

| Field                      | Type               | Description                     |
|:---------------------------|:-------------------|:--------------------------------|
| `highest_block_checkpoint` | `CheckpointBeacon` | Last beacon (checkpoint + hash) |

The checkpoint beacon (`CheckpointBeacon`) is composed of the following fields:

| Field             | Type      | Description                            |
|:------------------|:----------|:---------------------------------------|
| `checkpoint`      | `fixed32` | The serial number for this epoch       |
| `hash_prev_block` | `Hash`    | The 256-bit hash of the previous block |

## InventoryAnnouncement message

The `InventoryAnnouncement` message is used to advertise the knowledge of one or more objects (e.g. blocks, transactions, ...). The inventory message can be received unsolicited or in reply to a `LastBeacon` message.

The `InventoryAnnouncement` message consists of a message header with the `InventoryAnnouncement` command and a payload containing one or more inventory entries:

| Field       | Type                | Description                 |
| ----------- | ------------------- | --------------------------- |
| `inventory` | `inventory_entry[]` | Vector of inventory entries |

## InventoryRequest message

The `InventoryRequest` messages are used in order to request specific objects from other nodes. Usually, `InventoryRequest` messages are sent after receiving a `InventoryAnnouncement` message and filtering the already known objects. The response to a `InventoryRequest` message is often one or more messages carrying the requested objects (e.g.: `Block`, `Transaction`, etc.).

The `InventoryRequest` message consists of a message header with the `InventoryRequest` command and a payload following this format:

| Field       | Type                      | Description                                |
|:------------|:--------------------------|:-------------------------------------------|
| `inventory` | `repeated InventoryEntry` | Inventory entries that are being requested |

## Block message

The `Block` message is used to transmit a single serialized block as a response to a `InventoryRequest` message.

The `Block` message consists of a message header with the `Block` command and a payload containing information for a block following the format defined in the [Block] section.

## Transaction message

Analogously, the `Transaction` message is used to transmit a single serialized transaction as a response to a `InventoryRequest` message.

The `Transaction` message consists of a message header with the `Transaction` command and a payload containing information for a transaction following the format defined in the [Transaction] section.

## Helper data structures

### Hash

The `hash` (`Hash`) data structure is a tagged union of the supported hashing algorithms:

| Kind     | Description                 |
|:---------|:----------------------------|
| `SHA256` | SHA256 hash type (32 bytes) |

In the future, the type values may be extended in order to consider additional hash types.

### InventoryEntry

The `InventoryEntry` data structure is a tagged union of the different inventory item types:
                                    
| Kind          | Description                                     |
|:--------------|:------------------------------------------------|
| `Error`       | Data with this number may be ignored            |
| `Tx`          | Hash is related to a transaction                |
| `Block`       | Hash is related to a block                      |
| `DataRequest` | Hash is related to a data request               |
| `DataResult`  | Hash is related to the result of a data request |

Each type has one field:

| Field  | Type   | Description           |
|:-------|:-------|:----------------------|
| `hash` | `Hash` | The hash of this item |


In the future, the type values may be extended in order to consider additional features.

[Block]: /protocol/network/data-structures/block/
[Transaction]: /protocol/network/data-structures/transaction/