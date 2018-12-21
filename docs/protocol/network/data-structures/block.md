# Block

In the Witnet network protocol, a `block` is formatted as follows:

| Field       | Type              | Description                                                                                          |
|-------------|-------------------|------------------------------------------------------------------------------------------------------|
| `header`    | `block_header`    | The header of the block                                                                              |
| `txns`      | `[tx]`            | Block transactions following the format of a `TX` command, as described in the [Transaction] section |
| `signature` | `keyed_signature` | A [keyed signature][Signature] of the block header                                                   |

A non-empty list of transactions is always provided because the _mint_ transaction should always be included.

## Block header structure

The block header (`block_header`) is composed of the following fields:

| Field              | Type        | Description                                                             |
|--------------------|-------------|-------------------------------------------------------------------------|
| `version`          | `u32`       | The block version number indicating the block validation rules          |
| `beacon`           | `beacon`    | A checkpoint beacon for the epoch that this block is closing            |
| `hash_merkle_root` | `[u8; 32]`  | A 256-bit hash based on all of the transactions committed to this block |
| `proof`            | `signature` | A miner-provided _Proof of Eligibility_                                 |

## Checkpoint beacon structure

The checkpoint beacon (`beacon`) is composed of the following fields:

| Field             | Type       | Description                                   |
|-------------------|------------|-----------------------------------------------|
| `checkpoint`      | `u32`      | The serial number for an epoch                |
| `hash_prev_block` | `[u8; 32]` | The 256-bit hash of the previous block header |

## Proof of Eligibility

The _Proof of Eligibility_ (`proof`) signature is computed by simply signing the `beacon` field of the block header using the same private key as for the `signature`.

Signature structures are defined in the [Signature] section.

[Signature]: /protocol/network/data-structures/signature/
[Transaction]: /protocol/network/data-structures/transaction/