# Inventory Manager

The __Inventory manager__ is the actor in charge of managing the entire life cycle of all 
inventory items (i.e. transactions and blocks). It acts as a single entry point for 
getting and putting inventory items from and into StorageManager. This creates one more 
degree of abstraction between how storage works and the core business logic of the app.

Evicting inventory items should not be necessary for the time being. However, we may need 
to support such feature in the future if we decide to deal with deeper chain reorganizations.

## Actor creation and registration

The creation of the inventory manager actor is performed directly by the `main` process:

```rust
let inventory_manager_addr = InventoryManager::start_default();
System::current().registry().set(inventory_manager_addr);
```

## API

### Incoming messages: Others -> inventory manager

These are the messages supported by the inventory manager handlers:

| Message   | Input type                                | Output type                                    | Description                                                    |
|-----------|-------------------------------------------|------------------------------------------------|----------------------------------------------------------------|
| `AddItem` | `InventoryItem`                           | `Result<(), InventoryManagerError>`            | Add a valid Inventory Item to storage through InventoryManager |
| `GetItem` | `Hash`                                    | `Result<InventoryItem, InventoryManagerError>` | Get a Inventory Item from storage through InventoryManager     |

The way other actors will communicate with the InventoryManager is:

1. Get the address of the InventoryManager actor from the registry:

    ```rust
    // Get InventoryManager address
    let inventory_manager_addr = System::current().registry().get::<InventoryManager>();
    ```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to send a message to the actor:

    ```rust
    inventory_manager_addr
        .send(GetItem)
        .into_actor(self)
        .then(|res, _act, _ctx| {
            // Process the response from InventoryManager
            process_get_config_response(res)
        })
        .and_then(|item, _act, ctx| {
            // Do something with the item
            actix::fut::ok(())
        })
        .wait(ctx);
    ```


### Outgoing messages: inventory manager -> Others

These are the messages sent by the inventory manager:

| Message           | Destination   | Input type    | Output type                        | Description                          |
|-------------------|---------------|---------------|------------------------------------|--------------------------------------|
