# Inventory Exchange

The inventory exchange protocol describes the interaction between nodes in order to synchronize themselves with the network and reestablish the full blockchain (in terms of blocks and transactions).

Inventory exchanges envision 2 main scenarios:

 1. **Block Download**, for nodes in need of synchronizing their blockchains by requesting and downloading blocks.

 2. **Inventory Broadcasting**, for nodes willing to broadcast transactional information, such as blocks and transactions.

## Block Download

**Block Download** is performed by nodes that need to catch up with other nodes by downloading missing blocks in order to reconstruct their blockchains. For example, this is the case of nodes connecting to the blockchain for the first time or after a significant amount of downtime.

Starting from the block #0 (the hardcoded genesis block), the nodes need to validate all blocks up to the current tip of the blockchain. Therefore, in case of missing blocks, a local node will initiate the following process of synchronization with its outbound peers:

 1. The local node will have already exchanged `version` messages with its remote outbound peers. Those `version` messages contain the last epoch known to others peers, i.e. the local peer can already compare how many blocks they each have and identify how many are missing.

 2. The local node will send a `get_blocks` message to all its outbound nodes (after successful handshake protocol). These messages contain the hash of the top block of the local blockchain.

 3. Remote peers will reply by sending another `get_blocks` message containing the hash of their top block of their respective blockchains.

 4. The peer with the longest blockchain will identify which blocks are required by the other peer in order to allow it to synchronize to its blockchain. The peer will select up to the first consecutive 500 blocks and it will transmit their hashes using an `inv` (inventory) message.

 5. After identifying which blocks are missing (it may already have some of them), the node may request them by using a `get_data` message, containing the hashes of the needed blocks. This message will help the node to catch up with the current full blockchain.

 6. After receiving the `get_data` message, the peer sends the requested blocks individually by using `block` messages.

The following diagram depicts the previously described process under the assumption that the peer with the longest blockchain is `NodeB` (step 4).

```ascii
         NodeA                            NodeB
           +                                +
           |           GET_BLOCKS           |
           +------------------------------->+
           |           GET_BLOCKS           |
           +<-------------------------------+
           |              INV               |
           +<-------------------------------+
           |                                |
           |                                |
           |            GET_DATA            |
           +------------------------------->+
           |             BLOCK              |
           +<-------------------------------+
           |             BLOCK              |
           +<-------------------------------+
           |             BLOCK              |
           +<-------------------------------+
           |                                |
           +                                +
```

## Inventory Broadcasting

Similarly to the previously described process of synchronization, any node may contribute to the synchronization of their outbound peers by advertising inventory objects such as blocks and transactions. Inventory broadcasting is also used in case a node creates transactions or mine blocks.

The inventory broadcasting can be described as the following sequence of steps:

 1. A remote node broadcasts its inventory by sending an `inv` message, containing all the hashes of the advertised inventory objects.

 2. After receiving an `inv` message and filtering the inventory objects that may be missing in the local blockchain, the local node sends a `get_data` message with the hashes of the needed objects.

 3. The remote note receives the `get_data` message and sends a `block` or `tx` message per requested inventory object (identified by a hash).

The following diagram depicts the previous step under the assumption that the local node (`NodeA`) sends a `get_data` message requesting 3 blocks and 2 transactions.

```ascii
         NodeA                            NodeB
           +                                +
           |              INV               |
           +<-------------------------------+
           |                                |
           |           GET_DATA             |
           +------------------------------->+
           |                                |
           |             BLOCK              |
           +<-------------------------------+
           |             BLOCK              |
           +<-------------------------------+
           |             BLOCK              |
           +<-------------------------------+
           |              TX                |
           +<-------------------------------+
           |              TX                |
           +<-------------------------------+
           |                                |
           +                                +
```

## Get blocks message

The `get_blocks` messages are used in order to notify the hash of the highest known block by the peer. After exchanging `get_blocks` messages between peers, the one with the longest blockchain in terms of blocks will send an `inv` message to the other peer. This message will include the list of block hashes starting right after the last known block hash provided by the other peer.

The `get_blocks` message consists of a message header with the `GET_BLOCKS` command and a payload containing the tip of the chain (the hash of the latest block) as known to the local peer:

| Field        | Type       | Description                                    |
| ------------ | :--------: | ---------------------------------------------- |
| `last_block` | `[u8; 32]` | Hash of the last known block to the local node |

## Inv message

The `inv` message is used to advertise the knowledge of one or more objects (e.g. blocks, transactions, ...). The inventory message can be received unsolicited or in reply to a `get_blocks` message.

The `inv` message consists of a message header with the `INV` command and a payload containing one or more inventory entries:

| Field       | Type         | Description                 |
| ----------- | :----------: | --------------------------- |
| `count`     | `u16`        | Number of inventory entries |
| `inventory` | `inv_vect[]` | Inventory vectors           |

The `inv_vect` (inventory vector) data structure has the following schema:

| Field  | Type       | Description                                  |
| ------ | :--------: | -------------------------------------------- |
| `type` | `u8`       | Type of object linked to the inventory entry |
| `hash` | `[u8; 32]` | Hash of the object                           |

The possible values for the `type` field are:

| Value | Name           | Description                                     |
| ----- | -------------- | ----------------------------------------------- |
| `0`   | `ERROR`        | Data with this number may be ignored            |
| `1`   | `TX`           | Hash is related to a transaction                |
| `2`   | `BLOCK`        | Hash is related to a block                      |
| `3`   | `DATA_REQUEST` | Hash is related to a data request               |
| `4`   | `DATA_RESULT`  | Hash is related to the result of a data request |

In the future, the type values may be extended in order to consider additional features.

## Get data messages

The `get_data` messages are used in order to request specific objects from other nodes. Usually, `get_data` messages are sent after receiving a `inv` (inventory) message and filtering the already known objects. The response to a `get_data` message is often a message carrying the requested object (e.g.: `block`, `tx`, etc.).

The `get_data` message consists of a message header with the `GET_DATA` command and a payload following this format:

| Field       | Type         | Description                 |
| ----------- | :----------: | --------------------------- |
| `count`     | `u16`        | Number of inventory entries |
| `inventory` | `inv_vect[]` | Inventory vectors           |

## Block message

The `block` message is used to transmit a single serialized block as a response to a `get_data` message.

The `block` message consists of a message header with the `BLOCK` command and a payload containing information for a transaction following the format defined in the [Block] section.

## Tx message

Analogously, the `tx` message is used to transmit a single serialized transaction as a response to a `get_data` message.

The `tx` message consists of a message header with the `TX` command and a payload containing information for a transaction following the format defined in the [Transaction] section.

[Block]: /protocol/network/data-structures/block/
[Transaction]: /protocol/network/data-structures/transaction/