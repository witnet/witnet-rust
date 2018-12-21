# Transaction

In the Witnet network protocol, a `transaction` is formatted as follows:

| Field        | Type                | Description                                    |
|--------------|---------------------|------------------------------------------------|
| `version`    | `u32`               | The transaction data format version number     |
| `inputs`     | `[input]`           | A list of transaction inputs                   |
| `outputs`    | `[output]`          | A list of 1 or more transaction outputs        |
| `signatures` | `[keyed_signature]` | A list of keyed signatures (as many as inputs) |

Long story short, _inputs_ contain data that proves ability to "pull" value from past transactions into a new transaction, while _outputs_ redistribute such value and lock them under new spending conditions. Signatures ensure integrity of the transaction and complement input's function when it comes to prove ability to unlock funds from past transactions.

Generally, the sum of the values of the outputs in a transaction must not exceed the sum of the values of the inputs, so as to guarantee that value is not created out of thin air. The only exception to this rule is the _mint_ transaction, which every block's miner node must include at the beginning of the transactions list contained in it. _Mint_ transactions, which are roughly equivalent to Bitcoin's _coinbase_, have no inputs and only one output, thus effectively _minting_ a fixed amount of new value.

As it is the case for many other _unspent output based_ cryptocurrencies, for every transaction, any value surplus after detracting the total output value from the total input value is considered to be the _miner fee_, which can be redeemed by the miner of the block in which the transaction gets anchored.

## Outputs

Outputs gather the value brought into transactions by inputs and lock fractions of that value under new spending conditions. 

Transactions may contain different types of outputs:

- Value transfer: roughly equivalent to Bitcoin's P2PKH/P2WPKH, where the output specifies the hash of a public key.
- Client Data Request (DR): publishes a request for data. It must include scripts for retrieval, aggregation, consensus and, optionally, deliver clauses.
- Commit: used by witnesses to (1) commit the results of their retrieval tasks without revealing the actual value of the retrieved data, and (2) pledge their share of the value attached to the data request as a reward.
- Reveal: used by witnesses to (1) reveal the actual value of the retrieved data that they committed in their previous _commit_, and once again to (2) pledge their share of the value attached to the data request as a reward.
- Tally: used by the block miner to (1) publish the result of a data request after consensus, and (2) refund the unspent _commit_ outputs to the data request creator.

Different output types also cause the transactions they are in to be validated using specific validation rules.

### Value transfer outputs

_Value transfer_ outputs (VTO) very much resemble Bitcoin's _pay-to-public-key_ (P2PKH) outputs. For anyone to spend a value transfer output, they must sign the spending transaction with a private key whose matching public key's `SHA256` hash digest starts with the exact 20 bytes explicitly stated in the output itself.

As those 20 bytes represent an entropy of `2^160` taken from the output of a hash function that is generally accepted to be secure under the [random oracle model], it can be safely assumed that a signature that satisfies such requirements was likely produced with a particular private key and therefore whoever provided the signature is also in possession of such private key.

The `pkh` field is defined as the first 20 bytes of the digest of a public key.

VTOs can be time locked so as to prevent further transactions from spending their value before a certain date and time.

#### Data structure

| Field       | Type   | Description                                                     |
|-------------|--------|-----------------------------------------------------------------|
| `pkh`       | `[u8]` | Slice of the digest of a public key (20 bytes)                  |
| `value`     | `u64`  | Transaction value                                               |
| `time_lock` | `u64`  | The UTC Unix timestamp before which the output can not be spent |


#### Specific validation rules

- VTOs take their value from the aggregate of all the inputs in the transactions.
- The number of VTOs in a single transaction is virtually unlimited as long as the VTOs are all contiguous and located at the end of the outputs list.
- A single VTO spending from no inputs is considered to be a _mint_ transaction, which is only acceptable if located first in the list of transactions of a block.
- The value brought into a transaction by an input pointing to a VTO can be freely assigned to any output of any type, unless otherwise restricted by the specific validation rules for such output type.

### Data Request outputs

Data request outputs publish requests for retrieving, aggregating and delivering data from external sources. At the same time, they specify and lock fees that will reward the different players involved throughout the life cycle of a data request, i.e. the nodes retrieving the data (a.k.a. _witnesses_) and the miner nodes responsible for timely including `commit`, `reveal` and `tally` transactions into new blocks.

During the _reveal_ stage, some eligible witnesses who published commitments may not follow up with their reveals. This could happen if they are not able to see their commitment transactions timely included in a block (e.g. because of network errors).

Miners are actually not obliged to include all the reveal transactions and eventually end up assigning rewards to the committers. This is because there is no way for the network to enforce punishment on them for neglecting or trying to conceal those transactions because there is no guarantee that they will be known to them in discrete time or even known whatsoever.

However, for every of those transactions that they include in a block, they are eligible for collecting special fees as explicitly specified and set aside for them in the original data request output, i.e. the _reveal_fee_ and _tally_fee_. It is therefore to be expected that miners will include as many of those transactions as known to them as for maximizing their profit.

This type of output also provides the digest of the public key to which the requester wants any unassigned rewards to be refunded. This digest does not necessarily need to match the public key used to sign the transaction where this output is included, which allows requesters to "donate" those funds to a third party or to simply move them to another public key of their own.

#### Data structure

| Field              | Type   | Description                                                                                                                    |
|--------------------|--------|--------------------------------------------------------------------------------------------------------------------------------|
| `data_request`     | `[u8]` | Data request scripts as a byte array                                                                                           |
| `pkh`              | `[u8]` | Slice of the digest of a public key (20 bytes)                                                                                 |
| `value`            | `u64`  | Transaction value that will be used as reward to be distributed after consensus has been reached and fees have been subtracted |
| `witnesses`        | `u8`   | Minimum amount of witness nodes that will be employed for resolving this data request                                          |
| `backup_witnesses` | `u8`   | Number of backup witnesses that will be employed for resolving this data request                                               |
| `commit_fee`       | `u64`  | Miner fee for each valid _commit_ output included in the block during the _commit stage_                                       |
| `reveal_fee`       | `u64`  | Miner fee for each valid _reveal_ output included in the block during the _reveal stage_                                       |
| `tally_fee`        | `u64`  | Miner fee for each valid _value_ transfer output included in the block during the _tally stage_                                |
| `time_lock`        | `u64`  | The UTC Unix timestamp after which data request shall be executed                                                              |

#### Values, rewards and fees

The minimum data request reward to be eventually distributed in the _tally_ among nodes that agreed with the consensus is defined as follows:

```math
dr_reward_min = value - (witnesses * commit_fee) - (witnesses * reveal_fee) - (witnesses * tally_fee)
```

#### Specific validation rules

- Multiple _data request_ outputs can be included into a single transaction as long as the _inputs are greater than outputs_ rule still hold true. The difference with VTOs is that the total output value for _data request_ outputs also include the _commit fee_, _reveal fee_ and _tally fee_.
- The value brought into a transaction by an input pointing to a _data request_ output can only be spent by _commit_ outputs.

### Commit outputs

_Commit_ outputs are used by witnesses for submitting a commitment of the results of their retrieval and aggregation tasks without revealing the actual value of the data. This prevents other eligible witness nodes from not executing the data request, just trying to replay other witness nodes' reported results.

When creating commitments, a randomly generated secret value called _nonce_ is paired with the actual value that resulted from executing the data request, again to prevent other witness nodes from acting lazy, trying to guess and replay others' commitments.

An unforeseeable and time-bound source of pseudo-randomness is also included into the mix when creating the commitment, so that this computation cannot be performed ahead of time. Namely, this source of randomness is the latest checkpoint beacon, which contains the identifier of the latest block in the chain, which is extremely hard to predict.

Therefore, the algorithm for computing a commitment is:

```math
SHA256(result || nonce || beacon)
```
#### Data structure

| Field        | Type   | Description                                                                                                |
|--------------|--------|------------------------------------------------------------------------------------------------------------|
| `commitment` | `[u8]` | Digest of the data request's aggregation stage, salted by a nonce and the previous checkpoint beacon       |
| `value`      | `u64`  | Remaining transaction value that will be used as reward to be distributed after consensus has been reached |

#### Values, rewards and fees

The `value` of the commit output depends on the target number of witness nodes employed, as stated in the data request itself:

```math
commit_value = (data_request_value / witnesses) - commit_fee
```

#### Specific validation rules

- _Commit_ outputs can only take value from _data request_ inputs whose index in the inputs list is the same as their own index in the outputs list.
- Multiple _commit_ outputs can exist in a single transaction, but each of them needs to be coupled with a _data request_ input occupying the same index in the inputs list as their own in the outputs list. Predictably, as a result of the previous rule, each of the multiple _commit_ outputs only takes value from the _data request_ input with the same index.
- The value brought into a transaction by an input pointing to a _commit_ output can only be spent by _reveal_ or _tally_ outputs.

### Reveal outputs

_Reveal_ outputs are created and published by every witness node who previously published a commitment only after they have verified that a sufficient number of other witness nodes have published their own commitments for the same data request. This is to prevent others from forging commitments without actually executing the retrieval and aggregation as requested.

This type of output contains the result of executing the retrieval and aggregation stage scripts of a data request. It also provides the digest of the public key to which the witness node wants the reward to be assigned if the revealed value passes the consensus stage function as explicitly defined by the original data request. This digest does not necessarily need to match the public key used by the witness node for eligibility (i.e. mining and request resolving). This allows witness nodes to "donate" the rewards to a third party or to simply move them to another public key of their own.

#### Data structure

| Field    | Type   | Description                                                                                                |
|----------|--------|------------------------------------------------------------------------------------------------------------|
| `reveal` | `[u8]` | The result of executing the retrieval and aggregation stage scripts of a data inputs can onrequest         |
| `pkh`    | `[u8]` | Slice of the digest of a public key (20 bytes)                                                             |
| `value`  | `u64`  | Remaining transaction value that will be used as reward to be distributed after consensus has been reached |

#### Values, rewards and fees

The `value` of the reveal output depends on the number of witness nodes employed, as stated in the data request itself:

```math
reveal_value = commit_value - reveal_fee
```

#### Specific validation rules

- _Reveal_ outputs can only take value from _commit_ inputs whose index in the inputs list is the same as their own index in the outputs list.
- Multiple _reveal_ outputs can exist in a single transaction, but each of them needs to be coupled with a _commit_ input occupying the same index in the inputs list as their own in the outputs list. Predictably, as a result of the previous rule, each of the multiple _reveal_ outputs only takes value from the _commit_ input with the same index.
- The value brought into a transaction by an input pointing to a _reveal_ output can only be spent by _value transfer_ outputs.
- Any transaction including an input pointing to a _reveal_ output must also include exactly only one _tally_ output.

### Tally outputs

_Tally_ outputs are used by block miners for publishing the result of running each data request's consensus stage script on the data revealed by the witness nodes that were lucky enough to be eligible for doing so. _Tally_ outputs are only present in transactions created by miners for joining all the reveal outputs for the same data request and eventually creating new outputs for rewarding the "revealers". Thus, those transactions will contain at most as many _value transfer_ outputs as witnesses were originally employed plus the _tally_ output itself.

Singularly, the `pkh` found in tally outputs is not the digest of the public key of the miner or any witness node, but that of the request creator, as explicitly stated in the original data request. This allows refunding any value left after distributing all rewards and fees.

#### Data structure

| Field    | Type   | Description                                                                                                                                                              |
|----------|--------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `result` | `[u8]` | Data request result as computed by applying the consensus stage function as specified by the data request on every _reveal_ input in the same transaction as this output |
| `pkh`    | `[u8]` | Slice of the digest of the public key of the data request creator (20 bytes)                                                                                             |
| `value`  | `u64`  | Remaining transaction value that has not been used as reward or fee of the data request                                                                                  |

#### Values, rewards and fees

The `value` of the _tally_ output is the remaining value after distributing all rewards and fees among witnesses and miners respectively:

```math
reveal_value = data_request_value - committers * commit_fee - revealers * (reveal_fee + tally_fee + reward)
```

#### Specific validation rules

- Any transaction can contain at most one _tally_ output.
- Transactions containing _tally_ outputs must not be broadcast through the inventory announcement protocol.
- As a result of the previous rule, transactions containing _tally_ outputs can only be included into a block by the miner of the block.
- The value brought into a transaction by an input pointing to a _tally_ output can be freely assigned to any output of any type, unless otherwise restricted by the specific validation rules for such output type.

## Inputs

Transaction inputs are references to outputs from past transactions. They "pull" all the value from those outputs and make it available for being spent by the outputs in the same transaction they are in. This data structure—which pairs a transaction's identifier with the index of one of its outputs—unambiguously points to a unique output from a specific transaction.

Every input included in a transaction needs to be coupled with a signature in the _signatures_ section.

Some inputs also provide additional pieces of data as required to fulfill the specific spending conditions of the outputs they are pointing to. These pieces of data are called _claims_, and allow the party creating a transaction to prove their right to spend the referred output and convince every other node in the network to consider the transaction to be valid and to broadcast it.

Different output types require their spending inputs to provide specific claims in order to fulfill their spending conditions.

All input structures consist at least of the following fields:

| Field            | Type   | Description                                       |
|------------------|--------|---------------------------------------------------|
| `transaction_id` | `[u8]` | A transaction identifier                          |
| `output_index`   | `u32`  | The index of a specific output in the transaction |

Inputs trying to spend outputs of type _data request_ and _commit_ have additional fields for their specific claims, as described below.

### Data request input

Every _data request_ output can be spent by as many _data request_ inputs as defined in the output itself, which has a field explicitly stating such number. For a witness node to be able to put aside a share of the reward from the data for itself, it must provide an input with a _Proof of Eligibility_ (PoE) claim: a cryptographically verifiable proof of their right to act as a witness for such data request in the current epoch. In addition, for every other node in the network to be able to verify such proof, this PoE must be produced using a private key that matches the the public key included in the _signatures_ section of the transaction.

Thus, the _data request_ input structure consists of the following fields:

| Field            | Type   | Description                                                                  |
|------------------|--------|------------------------------------------------------------------------------|
| `transaction_id` | `[u8]` | A transaction identifier                                                     |
| `output_index`   | `u32`  | The index of a specific output in the transaction                            |
| `poe`            | `[u8]` | Proof of Eligibility produced with same keypair as the transaction signature |

### Commit input

_Commit_ inputs are used by witness nodes for proving that they actually executed the data request in a timely manner and revealing the actual result value that they secretly committed in their _commit_ transactions. Therefore, the claims in _commit_ inputs provide every element that was used for producing the previously published commitment but was unknown to the rest of the nodes in the network by that moment. Namely, those claims are the _reveal_ and _nonce_ values.

Thus, the _commit_ input structure consists of the following fields:

| Field            | Type   | Description                                                                      |
|------------------|--------|----------------------------------------------------------------------------------|
| `transaction_id` | `[u8]` | A transaction identifier                                                         |
| `output_index`   | `u32`  | The index of a specific output in the transaction                                |
| `reveal`         | `[u8]` | The result of executing the retrieval and aggregation stages of the data request |
| `nonce`          | `u64`  | The nonce used for generating the previously published commitment                |

### Reveal input

_Reveal_ inputs abide by the general input format without adding any specific claim.

The only distinctive feature of _reveal_ inputs is that they do not require matching signatures, as the transactions where these inputs can be included are always built by the nodes who produce the blocks where they are anchored, and in doing so, they already provide the signature of the entire list of transactions in the block's header.

## Signatures

As aforementioned, transactions should include as many signatures as inputs. In every transaction, signatures complement the material required for satisfying the spending conditions that encumbered the past transaction outputs that the inputs in the transaction are trying to spend. Signatures and inputs are matched positionally, i.e. the first claim is checked against the first input and so forth.

Signatures prove ownership of a certain private key by providing a signature of the identifier of the transaction produced with such key and the serialization of the matching public key.

Transaction signatures are structured as [keyed signatures][Signature]. 

Only the _reveal inputs_ do not require matching signatures, as the transactions where these inputs can be included are always built by the nodes who produce the blocks where they are anchored, and in doing so, they already provide the signature of the entire list of transactions in the block's header.

[random oracle model]: https://en.wikipedia.org/wiki/Random_oracle
[Signature]: /protocol/network/data-structures/signature/