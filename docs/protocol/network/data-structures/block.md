# Block

In the Witnet network protocol, a `Block` is formatted as follows:

| Field          | Type                   | Description                                        |
|:---------------|:-----------------------|:---------------------------------------------------|
| `block_header` | `BlockHeader`          | The header of the block                            |
| `proof`        | `LeadershipProof`      | A miner-provided _Proof of Eligibility_            |
| `txns`         | `repeated Transaction` | A [keyed signature][Signature] of the block header |

A non-empty list of transactions is always provided because the _mint_ transaction should always be included.

## Block header structure

The block header (`BlockHeader`) is composed of the following fields:

| Field              | Type               | Description                                                             |
|:-------------------|:-------------------|:------------------------------------------------------------------------|
| `version`          | `uint32`           | The block version number indicating the block validation rules          |
| `beacon`           | `CheckpointBeacon` | A checkpoint beacon for the epoch that this block is closing            |
| `hash_merkle_root` | `Hash`             | A 256-bit hash based on all of the transactions committed to this block |

## Checkpoint beacon structure

The checkpoint beacon (`CheckpointBeacon`) is composed of the following fields:

| Field             | Type      | Description                            |
|:------------------|:----------|:---------------------------------------|
| `checkpoint`      | `fixed32` | The serial number for this epoch       |
| `hash_prev_block` | `Hash`    | The 256-bit hash of the previous block |

## Proof of Eligibility

The _Proof of Eligibility_ (`LeadershipProof`) signature is computed by
simply signing the `beacon` field of the block header using the same
private key as for the `signature`.

Signature structures are defined in the [Signature] section.

[Signature]: /protocol/network/data-structures/signature/
[Transaction]: /protocol/network/data-structures/transaction/