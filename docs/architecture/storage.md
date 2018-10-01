# Persistent Storage

From the perspective of software architecture, __persistent storage__ is one of the key elements to maintaining a
distributed block chain. Its role is allowing nodes in the network to preserve important data structures that need to be
kept over time for trustless validation of new chain objects.

Namely, those structures are:

- The UTXO set
- Data requests
- Transactions
- Blocks

## Generic `Storage` Trait

__Witnet-rust__ features a generic `Storage` Rust trait ([`storage.rs`][storage]) that exposes a key/value API with the
elemental CRUD methods _(create, read, update, delete)_ while abstracting away from specific storage backend
implementations.

```rust
pub trait Storage<ConnData, Key, Value> { /** **/ }
```

The meaning of the generic types is the following:

| Generic type | Description                                                                                           |
|--------------|-------------------------------------------------------------------------------------------------------|
| ConnData     | Type of the data needed by the constructor for creating a connection to the storage backend.          |
| Key          | Type of the keys used to identify the records in the storage.                                         |
| Value        | Type of the values in the storage.                                                                    |


As of [PR #21][#21], Witnet-rust incorporates implementations for the following storage backends:

- [`rocks.rs`][rocks] : persists data into the local file system using the performant RocksDB engine.
- [`in_memory.rs`][in_memory]: keeps data in a `HashMap` that lives in the memory heap.
  
!!! warning
    In-memory storage is implemented only for the sake of testing the `Storage` trait. It is obviously not a viable
    persistence solution as data is totally wiped as soon as references to the storage go out of scope or the app dies.

### Instantiation

All implementors of the `Storage` trait can be instantiated with the `witnet_storage::storage::new()` constructor,
which must be used as a static method.

__Signature__
```rust
fn new(connection_data: ConnData) -> Result<Box<Self>>;
``` 

!!! tip
    Please note that the `witnet_storage::storage::new()` method wraps the return type into a `Box`.
    This is to ensure the value is allocated into the heap and to allow a reference to it (the `Box` itself) to outlive
    the constructor. 

__Example__
```rust
use witnet_storage::backends::in_memory::InMemoryStorage;

let storage: &InMemoryStorage = InMemoryStorage::new().unwrap();
```

### Creating and updating records with the `put()` Method

The `witnet_storage::storage::put()` method allows creating or replacing a value in the storage under a certain key.

__Signature__
```rust
fn put(&mut self, key: Key, value: Value) -> Result<()>;
```

__Example__
```rust
// Put value "bar" into key "foo"
storage.put(b"foo", b"bar".to_vec())?;
// Update value of "foo" to be "beer"
storage.put(b"foo", b"beer".to_vec())?;
```

### Getting records with the `get()` method

The `witnet_storage::storage::get()` method allows reading the value in the storage under a certain key.

__Signature__
```rust
fn get(&self, key: Key) -> Result<Option<Value>>;
```

__Example__
```rust
match storage.get(b"foo") {
    Ok(Some(value)) => , // Found a value
    Ok(None) => , // The key didn't exist
    Err(error) =>  // Error while reading
}
```

### Deleting records with the `delete()` method

The `witnet_storage::storage::delete()` method allows deleting a record in the storage given its key.

__Signature__
```rust
fn delete(&mut self, key: Key) -> Result<()>;
```

__Example__
```rust
storage.delete(b"foo")?;
```

## RocksDB Storage Backend

The `RocksDB` storage backend ([`rocks.rs`][rocks]) is one of the bundled storage backends in Witnet-rust.
It implements all the methods of the `Storage` trait for the `RocksStorage` struct:

```rust
/// Data structure for the RocksDB storage whose only member is a
/// rocksdb::DB object.
pub struct RocksStorage {
    db: DB
}
```

The actual implementor looks like this (function bodies and some lifetime annotations have been omitted for
brevity):

```rust
// Implement the Storage generic trait for the RocksStorage storage
// data structure.
impl Storage<&str, &[u8], Vec<u8>> for RocksStorage {

    fn new(path: &str) -> Result<Box<Self>>;
    
    fn put(&mut self, key: &[u8], value: Vec<u8>) -> Result<()>;
    
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    
    fn delete(&mut self, key: &[u8]) -> Result<()>;
    
}

```

These are the specific types for this implementor:

| Generic type | Specific type                                                                                         |
|--------------|-------------------------------------------------------------------------------------------------------|
| `ConnData`   | `&str`                                                                                                |
| `Key`        | `&[u8]`                                                                                               |
| `Value`      | `Vec<u8>`                                                                                             |

The full source code of the `Storage` implementor for `RocksStorage` can be found at [`rocks.rs`][rocks].

[#21]: https://github.com/witnet/witnet-rust/pull/21
[storage]: https://github.com/witnet/witnet-rust/blob/master/storage/src/storage.rs
[rocks]: https://github.com/witnet/witnet-rust/blob/master/storage/src/backends/source.rs
[in_memory]: https://github.com/witnet/witnet-rust/blob/master/storage/src/backends/in_memory.rs