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

| Message     | Input type | Output type | Description                                         |
|-------------|------------|-------------|-----------------------------------------------------|
| ValidatePoE | -          | bool        | Checks that the given proof of eligibility is valid |


### Outgoing messages: Reputation manager -> Others

These are the messages sent by the Reputation manager:

| Message | Destination | Input type | Output type | Description |
|---------|-------------|------------|-------------|-------------|
