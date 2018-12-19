# Transaction

In the Witnet network protocol, a `transaction` is formatted as follows:

| Field     |    Type    | Description                                |
| --------- | :--------: | ------------------------------------------ |
| `version` |   `u32`    | The transaction data format version number |
| `inputs`  | `[input]`  | A list of transaction inputs               |
| `outputs` | `[output]` | A list of 1 or more transaction outputs    |
| `claims`  | `[claim]`  | A list of claims                           |

## Inputs

Transaction inputs are references to outputs from past transactions, and additionally, some may require specific spending conditions. They "pull" all the value from those outputs and make it available for being spent by the outputs in the same transaction they are in. This data structure—which pairs a transaction's identifier with the index of one of its outputs—is also known as `outpoint` as it unambiguously points to a unique output from a specific transaction.

Transactions may contain different types of inputs:

- Value transfer: roughly equivalent to Bitcoin's P2PKH/P2WPKH, where the input refers to a value transfer past transaction.
- Commit: used by witnesses to (1) reference a past data request output, and (2) prove their eligibility as witness for such data request.
- Reveal: used by witnesses to (1) reference a past commitment output, and (2) prove its validity by providing the result and a nonce.
- Tally: used by block miner to (1) reference a past reveal output.

All input structures consist at least of the following fields:

| Field            |  Type  | Description                                       |
| ---------------- | :----: | ------------------------------------------------- |
| `transaction_id` | `[u8]` | The transaction identifier                        |
| `output_index`   | `u32`  | The index of a specific output in the transaction |

The commit and reveal input types require additional fields in their data structures.

### Commit input

A **data request** output has to be consumed/used by a number of **witnesses**, i.e. there will be as many claims as **witnesses** have been defined. For a committer to be able to pledge a share of the reward from the data request, they must provide an input with a _Proof of Eligibility_ (PoE) that proves their eligibility as witnesses for such data request in the current epoch. In addition, for everyone in the network to be able to verify such proof, this PoE should have been produced using a private key that matches the the public key included in the claim.

Thus, the commit input structure consists of the following fields:

| Field            |  Type  | Description                                            |
| ---------------- | :----: | ------------------------------------------------------ |
| `transaction_id` | `[u8]` | The transaction identifier                             |
| `output_index`   | `u32`  | The index of a specific output in the transaction      |
| `poe`            | `[u8]` | Proof of Eligibility (same key pair as in claim field) |

### Reveal input

A reveal input is used to prove that the witness has a valid `reveal`, i.e. the data request result and nonce match the previous commitment.

The commit input structure consists of the following fields:

| Field            |  Type  | Description                                            |
| ---------------- | :----: | ------------------------------------------------------ |
| `transaction_id` | `[u8]` | The transaction identifier                             |
| `output_index`   | `u32`  | The index of a specific output in the transaction      |
| `reveal`         | `[u8]` | Data request result                                    |
| `nonce`          | `u64`  | The nonce used to generate the data request commitment |

## Outputs

Transactions may contain different types of outputs:

- Value transfer: roughly equivalent to Bitcoin's P2PKH/P2WPKH, where the output specifies the hash of a public key.
- Client Data Request (DR): output that publishes a request for data. It shall include scripts for retrieval, aggregation, consensus and, optionally, deliver clauses.
- Commit: used by witnesses to (1) commit the results of their retrieval tasks without revealing the actual value of the retrieved data, and (2) pledge their share of the value attached to the data request as a reward.
- Reveal: used by witnesses to (1) reveal the actual value of the retrieved data that they committed in their previous *commit*, and once again to (2) pledge their share of the value attached to the data request as a reward.
- Consensus: used by the block miner to (1) publish the result of a data request after consensus, and (2) reimburse the data request creator with the unspent commit outputs.

### Value transfer outputs

P2PKH outputs are used in Value Transfer Transactions (VTT). The `pkh` field is defined as the first 20 bytes of the public key hash.

| Field   |  Type  | Description                         |
| ------- | :----: | ----------------------------------- |
| `pkh`   | `[u8]` | Slice of public key hash (20 bytes) |
| `value` | `u64`  | Transaction value                   |

### Data Request outputs

Data request outputs publish requests for retrieving, aggregating and delivering data from external sources. At the same time, they specify and lock fees that will reward the different players involved throughout the life cycle of a data request, i.e. the nodes retrieving the data (a.k.a. _witnesses_) and the miner nodes responsible for timely including `commit`, `reveal` and `tally` transactions into new blocks.

| Field              |  Type  | Description                                                                                                                     |
| ------------------ | :----: | ------------------------------------------------------------------------------------------------------------------------------- |
| `data_request`     | `[u8]` | Data request script as a byte array                                                                                             |
| `value`            | `u64`  | Transaction value that will be used as reward to be distributed after consensus has been reached and fees have been substracted |
| `witnesses`        |  `u8`  | Minimum amount of data request witnesses                                                                                        |
| `backup_witnesses` |  `u8`  | Number of backup data request witnesses                                                                                         |
| `commit_fee`       | `u64`  | Miner fee for each valid commit output included in the block during the **commit stage**                                        |
| `reveal_fee`       | `u64`  | Miner fee for each valid reveal output included in the block during the **reveal stage**                                        |
| `tally_fee`        | `u64`  | Miner fee for each valid P2PKH output included in the block during the  **tally stage**                                         |
| `time_lock`        | `u64`  | The UTC Unix timestamp after which data request may be executed                                                                 |

The minimum data request reward to be distributed among nodes that agreeed with the consensus is defined as follows:

```math
dr_reward_min = value - (witnesses * commit_fee) - (witnesses * reveal_fee) - (witnesses * tally_fee)
```

During the reveal stage, some eligible witnesses who published commitments may not follow up with their reveals. This could happen if they are not able to see their commitment transactions timely included in a block (e.g. because of network errors).

Miners are actually not obliged to include all the reveal transactions and eventually end up assigning rewards to the committers. This is because there is no way for the network to enforce punishment on them for neglecting or trying to conceal those transactions because there is no guarantee that they will be known to them in discrete time or even known whatsoever.

However, for every of those transactions that they include in a block, they are eligible for collecting special fees as explicitly specified and set aside for them in the original data request output. It is therefore to be expected that miners will include as many of those transactions as known to them as for maximizing their profit.

### Commit outputs

Commit outputs are used by witnesses to submit a commitment of the results of their retrieval tasks without revealing the actual value of the data.
This commitment is signed with the same private key as used for the Proof of Eligibility (PoE), so that the witness can be identified in the follow up with the **reveal**.

Additionally, commitments have be nounced in order to avoid lazy commitments and/or replaying by other witnesses.

| Field        |  Type  | Description                                                                                                |
| ------------ | :----: | ---------------------------------------------------------------------------------------------------------- |
| `commitment` | `[u8]` | Digest of the data request result sent by a witness, salted by a nonce and the previous checkpoint beacon  |
| `value`      | `u64`  | Remaining transaction value that will be used as reward to be distributed after consensus has been reached |

The `value` of the commit output depends on the number of committers:

```math
commit_value = (data_request_value / witnesses) - commit_fee
```

### Reveal outputs

The reveal output is included by witnesses and it contains the data request result. It also provides the public key hash to which the witness wants to be reimbursed if the consensus is reached.

| Field    |  Type  | Description                                                                                                |
| -------- | :----: | ---------------------------------------------------------------------------------------------------------- |
| `reveal` | `[u8]` | Data request result                                                                                        |
| `pkh`    | `[u8]` | Slice of public key hash (20 bytes)                                                                        |
| `value`  | `u64`  | Remaining transaction value that will be used as reward to be distributed after consensus has been reached |

The `value` of the reveal output depends on the number of committers that revealed their data request results:

```math
reveal_value = commit_value - reveal_fee
```

### Consensus outputs

The consensus output is included by the **block miner** and it defines the data request result after the consensus clause. Only one **consensus output** is required per data request.

| Field    |  Type  | Description                                                                                               |
| -------- | :----: | --------------------------------------------------------------------------------------------------------- |
| `result` | `[u8]` | Data request consensus result after using all data request reveal values of the previous **reveal stage** |
| `pkh`    | `[u8]` | Slice of public key hash (20 bytes) of the data request creator                                           |
| `value`  | `u64`  | Remaining transaction value that has not been used as reward or fee of the data request                   |

The `value` of the consensus output is the remaining value after distributing all rewards and fees among witnesses and miners respectively:

```math
reveal_value = data_request_value - committers * commit_fee - revealers * (reveal_fee + tally_fee + reward)
```

## Claims

As aforementioned, transactions should include as many claims as inputs. In every transaction, claims complement the material required for satisfying the spending conditions that encumbered the past transaction outputs that the inputs in the transaction are trying to spend (e.g. signatures). Claims and inputs are matched positionally, that is, the first claim is checked against the first input and so forth.

Claims prove ownership of a certain private key by providing a signature of the identifier of the transaction produced with such key and the serialization of the matching public key.

| Field        |  Type  | Description                                                |
| ------------ | :----: | ---------------------------------------------------------- |
| `signature`  | `[u8]` | Signature of the transaction digest, i.e. `transaction_id` |
| `public_key` | `[u8]` | Public Key of the P2PKH outpoint to be consumed            |

In Witnet, only the **tally inputs** do not require corresponding claims, as they are built by the miner, which already provides its own proof of leadership to mine the block.
