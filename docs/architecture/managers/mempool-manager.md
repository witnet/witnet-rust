# Mempool Manager

The __mempool manager__ is the actor in charge of:

* Validating transactions as they come from any `Session`. This includes:
    - Iterating over its inputs, asking the `UtxoManager` for the to-be-spent UTXOs and adding the value of the inputs to calculate the value of the transaction.
    - Running the output scripts, expecting them all to return `TRUE` and leave an empty stack.
    - Verifying that the sum of all inputs is greater than or equal to the sum of all the outputs.
* Keeping valid transactions into memory. This in-memory transaction pool is what we call the _mempool_. Valid transactions are immediately appended to the mempool.
* Receiving confirmation notifications from `BlocksManager`. This notifications tell that a certain transaction ID has been anchored into a new block and thus it can be removed from the mempool and persisted into local storage (for archival purposes, non-archival nodes can just drop them).
* Notifying `UtxoManager` for it to apply a valid transaction on the UTXO set.

The mempool actor is not backed by any persistance medium. If a node goes down, it will need to ask its peers for the entire mempool.

## Actor creation and registration

The creation of the mempool actor and its registration into the system registry are
performed directly by the main process [`node.rs`][noders]:

```rust
let mempool_manager_addr = MempoolManager::start_default();
System::current().registry().set(mempool_manager_addr);
```

## API

### Incoming: Others -> MempoolManager

These are the messages supported by the `MempoolManager` handlers:

| Message                                   | Input type                    | Output type              | Description                                    |
|-------------------------------------------|-------------------------------|--------------------------| -----------------------------------------------|

### Outgoing messages: MempoolManager -> Others

These are the messages sent by the blocks manager:

| Message           | Destination       | Input type                                    | Output type                 | Description                       |
|-------------------|-------------------|-----------------------------------------------|-----------------------------|-----------------------------------|
