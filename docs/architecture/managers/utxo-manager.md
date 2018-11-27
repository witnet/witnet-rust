# UTXO Manager

The __UTXO manager__ is the actor that encapsulates the logic of the _unspent transaction outputs_, that is, it will be in charge of:

* Keeping every unspent transaction output (UTXO) in the block chain in memory. This is called the _UTXO set_.
* Updating the UTXO set with valid transactions that have already been anchored into a valid block. This includes:
    - Removing the UTXOs that the transaction spends as inputs.
    - Adding a new UTXO for every output in the transaction.

## Actor creation and registration

The creation of the UTXO manager actor is performed directly by the `main` process:

```rust
let utxo_manager_addr = UtxoManager::start_default();
System::current().registry().set(utxo_manager_addr);
```

## API
 
### Incoming messages: Others -> UTXO manager
 
These are the messages supported by the UTXO manager handlers:

| Message   | Input type                                | Output type                           | Description                               |
|-----------|-------------------------------------------|---------------------------------------|-------------------------------------------|

### Outgoing messages: UTXO manager -> Others

These are the messages sent by the UTXO manager:

| Message           | Destination   | Input type    | Output type                        | Description                          |
|-------------------|---------------|---------------|------------------------------------|--------------------------------------|
