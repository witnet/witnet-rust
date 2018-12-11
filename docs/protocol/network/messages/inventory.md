# Inventory Exchange

The inventory exchange protocol describes the interaction between nodes in order to synchronize themselves with the network and reestablish the full blockchain (in terms of blocks and transactions).

Inventory exchanges envision 2 main scenarios:

 1. **Block Download**, for nodes in need of synchronizing their blockchains by requesting and downloading blocks.

 2. **Inventory Broadcasting**, for nodes willing to broadcast transactional information, such as blocks and transactions.

## Block Download

**Block Download** is performed by nodes that need to catch up with other nodes by downloading missing blocks in order to reconstruct their blockchains. For example, this is the case of nodes connecting to the blockchain for the first time or after a significant amount of downtime.

Starting from the block #0 (the hardcoded genesis block), the nodes need to validate all blocks up to the current tip of the blockchain. Therefore, in case of missing blocks, a local node will initiate the following process of synchronization with its outbound peers:

 1. The local node will have already exchanged `version` messages with its remote outbound peers. Those `version` messages contain the last epoch known to others peers, i.e. the local peer can already compare how many blocks they each have and identify how many are missing.

 2. The local node will send a `last_beacon` message to all its outbound nodes (after successful handshake protocol). These messages contain the hash of the top block of the local blockchain and the epoch of that block.

 3. Remote peers will reply by sending another `last_beacon` message containing the hash of the top block of their respective blockchains and the epochs for those top blocks.

 4. The peer with the longest blockchain will identify which blocks are required by the other peer in order to allow it to synchronize to its blockchain. The peer will select up to the first consecutive 500 blocks and it will transmit their hashes using an `inventory_announcement` message.

 5. After identifying which blocks are missing (it may already have some of them), the node may request them by using a `inventory_request` message, containing the hashes of the needed blocks. This message will help the node to catch up with the current full blockchain.

 6. After receiving the `inventory_request` message, the peer sends the requested blocks individually by using `block` messages.

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

 1. A remote node broadcasts its inventory by sending an `inventory_announcement` message, containing all the hashes of the advertised inventory objects.

 2. After receiving an `inventory_announcement` message and filtering the inventory objects that may be missing in the local blockchain, the local node sends a `inventory_request` message with the hashes of the needed objects.

 3. The remote note receives the `inventory_request` message and sends a `block` or `tx` message per requested inventory object (identified by a hash).

The following diagram depicts the previous step under the assumption that the local node (`NodeA`) sends a `inventory_request` message requesting 3 blocks and 2 transactions.

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

The `last_beacon` messages are used in order to notify the hash of the highest known block by the peer together with its epoch.
After exchanging `last_beacon` messages between peers, the one with the longest blockchain in terms of blocks will send an `inventory_announcement` message to the other peer. This message will include the list of block hashes starting right after the last known block hash provided by the other peer.

The `last_beacon` message consists of a message header with the `LAST_BEACON` command and a payload
containing the beacon for the tip of the chain (the hash of the latest block and its checkpoint) as
known to the local peer:

| Field                      | Type                | Description                     |
| -------------------------- | ------------------- | ------------------------------- |
| `highest_block_checkpoint` | `checkpoint_beacon` | Last beacon (checkpoint + hash) |

The `checkpoint_beacon` data structure has the following schema:

| Field             | Type     | Description                            |
| ----------------- | -------- | -------------------------------------- |
| `checkpoint`      | `uint32` | The checkpoint of the last known block |
| `hash_prev_block` | `Hash`   | The hash of the last known block       |

## InventoryAnnouncement message

The `inventory_announcement` message is used to advertise the knowledge of one or more objects (e.g. blocks, transactions, ...). The inventory message can be received unsolicited or in reply to a `last_beacon` message.

The `inventory_announcement` message consists of a message header with the `INVENTORY_ANNOUNCEMENT` command and a payload containing one or more inventory entries:

| Field       | Type                | Description                 |
| ----------- | ------------------- | --------------------------- |
| `inventory` | `inventory_entry[]` | Vector of inventory entries |

## InventoryRequest message

The `inventory_request` messages are used in order to request specific objects from other nodes. Usually, `inventory_request` messages are sent after receiving a `inventory_announcement` message and filtering the already known objects. The response to a `inventory_request` message is often one or more messages carrying the requested objects (e.g.: `block`, `tx`, etc.).

The `inventory_request` message consists of a message header with the `INVENTORY_REQUEST` command and a payload following this format:

| Field       | Type                | Description                                |
| ----------- | ------------------- | ------------------------------------------ |
| `inventory` | `inventory_entry[]` | Inventory entries that are being requested |

## Block message

The `block` message is used to transmit a single serialized block as a response to a `inventory_request` message.

The `block` message consists of a message header with the `BLOCK` command and a payload containing information for a block following the format defined in the [Block] section.

## Transaction message

Analogously, the `transaction` message is used to transmit a single serialized transaction as a response to a `inventory_request` message.

The `transaction` message consists of a message header with the `TRANSACTION` command and a payload containing information for a transaction following the format defined in the [Transaction] section.

## Helper data structures

### Hash

The `hash` (`Hash`) data structure has the following schema:

| Field   | Type        | Description |
| ------- | ----------- | ----------- |
| `type`  | `hash_type` | Hash type   |
| `bytes` | `[ubyte]`   | Hash bytes  |

The possible values for the `hash_type` enum (`ubyte`) field are:

| Value | Name     | Description      |
| ----- | -------- | ---------------- |
| `0`   | `SHA256` | SHA256 hash type |

In the future, the type values may be extended in order to consider additional hash types.

### InventoryEntry

The `inventory_entry` data structure has the following schema:

| Field  | Type                  | Description                                  |
| ------ | --------------------- | -------------------------------------------- |
| `type` | `inventory_item_type` | Type of object linked to the inventory entry |
| `hash` | `Hash`                | Hash of the object                           |

The possible values for the `inventory_item_type` enum (`ubyte`) field are:

| Value | Name           | Description                                     |
| ----- | -------------- | ----------------------------------------------- |
| `0`   | `ERROR`        | Data with this number may be ignored            |
| `1`   | `TX`           | Hash is related to a transaction                |
| `2`   | `BLOCK`        | Hash is related to a block                      |
| `3`   | `DATA_REQUEST` | Hash is related to a data request               |
| `4`   | `DATA_RESULT`  | Hash is related to the result of a data request |

In the future, the type values may be extended in order to consider additional features.

[Block]: /protocol/network/data-structures/block/
[Transaction]: /protocol/network/data-structures/transaction/