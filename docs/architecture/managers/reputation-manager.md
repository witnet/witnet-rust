# Reputation Manager

The __Reputation manager__ is the actor that encapsulates the logic related to the reputation of the node inside the Witnet network, that is, it will be in charge of:

* Checking that proofs of eligibility are valid for the known reputation of the issuers of such proofs
* Keeping score of the reputation balances for everyone in the network

## Actor creation and registration

The creation of the Reputation manager actor is performed directly by the `main` process:

```rust
let reputation_manager_addr = ReputationManager::start_default();
System::current().registry().set(reputation_manager_addr);
```

## API
 
### Incoming messages: Others -> Reputation manager
 
These are the messages supported by the Reputation manager handlers:

| Message       | Input type                            | Output type   | Description                                         |
|---------------|---------------------------------------|---------------|-----------------------------------------------------|
| `ValidatePoE` | `CheckpointBeacon`,`LeadershipProof`  | `bool`        | Checks that the given proof of eligibility is valid |

The way other actors will communicate with the ReputationManager is:

1. Get the address of the ReputationManager actor from the registry:

    ```rust
    // Get ReputationManager address
    let reputation_manager_addr = System::current().registry().get::<ChainManager>();
    ```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to send a message to the actor:

    ```rust

    reputation_manager_addr
        .send(ValidatePoE {
            beacon: msg.block.block_header.beacon.clone(),
            proof: msg.block.proof.clone(),
        })
        .into_actor(self)
        .then(|res, _act, _ctx| {
            // Process the response from ReputationManager
            process_get_config_response(res)
        })
        .wait(ctx);
    ```

### Outgoing messages: Reputation manager -> Others

These are the messages sent by the Reputation manager:

| Message | Destination | Input type | Output type | Description |
|---------|-------------|------------|-------------|-------------|
