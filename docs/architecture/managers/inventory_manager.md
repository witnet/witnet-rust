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

| Message   | Input type                                | Output type                           | Description                               |
|-----------|-------------------------------------------|---------------------------------------|-------------------------------------------|

### Outgoing messages: inventory manager -> Others

These are the messages sent by the inventory manager:

| Message           | Destination   | Input type    | Output type                        | Description                          |
|-------------------|---------------|---------------|------------------------------------|--------------------------------------|
