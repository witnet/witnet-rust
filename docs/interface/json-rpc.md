# JSON-RPC

JSON-RPC is a stateless, light-weight remote procedure call (RPC) protocol.
Primarily this specification defines several data structures and the rules
around their processing. It is transport agnostic in that the concepts can
be used within the same process, over sockets, over http, or in many various
message passing environments. It uses
[JSON](http://www.json.org/)
([RFC 4627](http://www.ietf.org/rfc/rfc4627.txt))
as data format.

For more details, see the [JSON-RPC 2.0 Specification][json_rpc_specs].

## Server

By default, a JSON-RPC server is started at `127.0.0.1:21338`.
It can be disabled in the [configuration file][configuration].

## Protocol

A message must be a valid utf8 string finished with a newline (`\n`).

The parser will start processing the request when it finds the first newline.

Therefore, the JSON string cannot contain any newlines expect for the final one.

`NewLineCodec`


### Methods

See [`json_rpc_methods.rs`][json_rpc_methods] for the implementation
details.

#### inventory

Make the node process, validate and potentially broadcast a new inventory item.

@params: `InventoryItem`
```rust
/// Inventory element: block, transaction, etc
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum InventoryItem {
    /// Error
    #[serde(rename = "error")]
    Error,
    /// Transaction
    #[serde(rename = "transaction")]
    Transaction(Transaction),
    /// Block
    #[serde(rename = "block")]
    Block(Block),
}
```

@returns: boolean indicating success, or parse error

Example:

```
{"jsonrpc": "2.0", "method": "inventory", "params": {"block":{"block_header":{"version":1,"beacon":{"checkpoint":2,"hash_prev_block":{"SHA256":[4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4]}},"hash_merkle_root":{"SHA256":[3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3]}},"proof":{"block_sig":null,"influence":99999},"txns":[null]}}, "id": 1}
```

Response:

```
{"jsonrpc":"2.0","result":true,"id":1}
```

#### getBlockChain

Get the list of all the known block hashes.

Returns a list of `(epoch, block_hash)` pairs.

Example:

```
{"jsonrpc": "2.0","method": "getBlockChain", "id": 1}
```

Response:

```
{"jsonrpc":"2.0","result":[[0,"ed28899af8c3148a4162736af942bc68c4466da93c5124dabfaa7c582af49e30"],[1,"9c9038cfb31a7050796920f91b17f4a68c7e9a795ee8962916b35d39fc1efefc"]],"id":1}
```

#### getOutputPointer
Get the outputPointer that matches with the input provided.

Returns an `OuputPointer`.

Example:

```
{"jsonrpc": "2.0","method": "getOutput", "params": {"transaction_id":{"SHA256":[17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17]},"output_index":1}, "id": "1"}
```

Response:

```
{"jsonrpc":"2.0","result":{"DataRequest":{"backup_witnesses":0,"commit_fee":0,"data_request":{"aggregate":{"script":[0]},"consensus":{"script":[0]},"deliver":[{"kind":"HTTP-GET","url":"https://hooks.zapier.com/hooks/catch/3860543/l2awcd/"}],"not_before":0,"retrieve":[{"kind":"HTTP-GET","script":[0],"url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22"}]},"pkh":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"reveal_fee":0,"tally_fee":0,"time_lock":0,"value":0,"witnesses":0}},"id":"1"}
```

[json_rpc_server]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/json_rpc/server.rs
[noders]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/node.rs
[json_rpc_methods]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/json_rpc/json_rpc_methods.rs
[json_rpc_specs]: https://www.jsonrpc.org/specification
[json_rpc_docs]: ../../interface/json-rpc/
[configuration]: ../../configuration/toml-file/