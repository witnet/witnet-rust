# Transaction

In the Witnet network protocol, a `transaction` is formatted as follows:

| Field     |    Type    | Description                                 |
| --------- | :--------: | ------------------------------------------- |
| `version` |   `u32`    | The transaction data format version number  |
| `inputs`  | `[input]`  | A list of transaction inputs                |
| `outputs` | `[output]` | A list of 1 or more transaction outputs     |
| `claims`  | `[claim]`  | A list of claims (i.e. spending conditions) |

## Inputs

Transaction inputs are references to outputs from past transactions. They "pull" all the value from those outputs and make it available for being spent by the outputs in the same transaction they are in. This data structure—which pairs a transaction's identifier with the index of one of its outputs—is also known as `outpoint` as it unambiguously points to a unique output from a specific transaction.

The input structure consists of the following fields:

| Field            |  Type  | Description                                       |
| ---------------- | :----: | ------------------------------------------------- |
| `transaction_id` | `[u8]` | The transaction identifier                        |
| `output_index`   | `u32`  | The index of a specific output in the transaction |

## Outputs

Transactions may contain different types of outputs:

- Value transfer: roughly equivalent to Bitcoin's P2PKH/P2WPKH, where the output specifies the hash of a public key.
- Client Data Request (DR): output that publishes a request for data. It shall include scripts for retrieval, aggregation, consensus and, optionally, deliver clauses.
- Commit: used by witnesses to (1) commit the results of their retrieval tasks without revealing the actual value of the retrieved data, and (2) pledge their share of the value attached to the data request as a reward.
- Reveal: used by witnesses to (1) reveal the actual value of the retrieved data that they committed in their previous *commit*, and once again to (2) pledge their share of the value attached to the data request as a reward.

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

| Field   | Type  | Description                                                                                                |
| ------- | :---: | ---------------------------------------------------------------------------------------------------------- |
| `value` | `u64` | Remaining transaction value that will be used as reward to be distributed after consensus has been reached |

The `value` of the reveal output depends on the number of committers that revealed their data request results:

```math
reveal_value = commit_value - reveal_fee
```

## Claims

As aforementioned, transactions should include as many claims as inputs. In every transaction, claims provide the material required for satisfying the spending conditions that encumbered the past transaction outputs that the inputs in the transaction are trying to spend (e.g. signatures). Claims and inputs are matched positionally, that is, the first claim is checked against the first input and so forth.

In Witnet, different output types implicitly have different spending conditions and therefore the spending transaction must provide specific items in their claims:

- Value transfer claim: prove ownership of a certain private key.
- Data request claim: prove eligibility for resolving the data request and publishing a commitment of the resulting value.
- Commit claim: provide a signed reveal message matching a previous `commitment` (hash) and that matches with the previous advertised public key.
- Reveal claim: provide a data request consensus result which is valid according to the consensus script and previous reveals outputs.

### Value transfer claim

Value transfer claims prove ownership of a certain private key by providing a signature of the identifier of the transaction produced with such key and the serialization of the matching public key.

| Field        |  Type  | Description                                                |
| ------------ | :----: | ---------------------------------------------------------- |
| `signature`  | `[u8]` | Signature of the transaction digest, i.e. `transaction_id` |
| `public_key` | `[u8]` | Public Key of the P2PKH outpoint to be consumed            |

### Data request claim (**commit stage**)

A **data request** output has to be consumed/used by a number of **witnesses**, i.e. there will be as many claims as **witnesses** have been defined. For a committer to be able to pledge a share of the reward from the data request, they must provide a _Proof of Eligibility_ (PoE) that proves their eligibility as witnesses for such data request in the current epoch. In addition, for everyone in the network to be able to verify such proof, they must include the public key that matches the private key that produced the PoE.

| Field        |  Type  | Description                                                |
| ------------ | :----: | ---------------------------------------------------------- |
| `signature`  | `[u8]` | Signature of the transaction digest, i.e. `transaction_id` |
| `poe`        | `[u8]` | Proof of eligibility                                       |
| `public_key` | `[u8]` | Public Key used for computing the PoE                      |

### Commit claim (**reveal stage**)

A commit claim is used to prove that the witness has a valid `reveal`, i.e. the signed data request result and nonce match the previous commitment.

| Field       |  Type  | Description                                                                                                |
| ----------- | :----: | ---------------------------------------------------------------------------------------------------------- |
| `signature` | `[u8]` | Signature of the transaction digest, i.e. `transaction_id`                                                 |
| `reveal`    | `[u8]` | Signed data request result using the previously advertised public key during the previous **commit stage** |
| `nonce`     | `u64`  | The nonce used to generate the data request commitment                                                     |

### Reveal claim (**tally stage**)

The reveal claim is included by the **block miner** and it defines the data request result after the consensus clause. Only one **reveal claim** is required for all **reveal inputs** of the same data request as it contains the data consensus result. The amount of inputs that the **reveal claim** is matching, is defined by the `reveals` field.

| Field       |  Type  | Description                                                                                               |
| ----------- | :----: | --------------------------------------------------------------------------------------------------------- |
| `consensus` | `[u8]` | Data request consensus after using all data request results provided during the previous **reveal stage** |
| `reveals`   |  `u8`  | Number of witnesses that revealed a data request result                                                   |
