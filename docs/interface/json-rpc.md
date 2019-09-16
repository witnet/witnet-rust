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

### Subscriptions

The Witnet node provides a pub/sub API, [see here for more info][pubsub].

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

#### getBalance
Get the total balance of the given address.

Returns a `u64`, representing balance. The unit is 10^-8 wits.

Example:

```
{"jsonrpc":"2.0","id":1,"method":"getBalance","params":["wit19kljem70vfkkvx5vk9uhh6fxyn9psd0h43ec8w"]}
```

Response:

```
{"jsonrpc":"2.0","result":428150000000000,"id":1}
```


#### getBlockChain

Get the list of all the known block hashes.

Returns a list of `(epoch, block_hash)` pairs.

These parameters can be used to limit to some epoch range.
There are two optional parameters: epoch and limit. For example, to get the
blocks for epochs `100-104`, use `"epoch" 100` and `"limit": 5`:

```json
"params": {
    "epoch": 100,
    "limit": 5,
}
```

If a negative epoch is supplied, it is interpreted as "the last N epochs".
For instance, to get the block for the last epoch:

```json
"params": {
    "epoch": -1,
}
```

Example:

```
{"jsonrpc": "2.0","method": "getBlockChain", "id": 1}
```

Response:

```
{"jsonrpc":"2.0","result":[[0,"ed28899af8c3148a4162736af942bc68c4466da93c5124dabfaa7c582af49e30"],[1,"9c9038cfb31a7050796920f91b17f4a68c7e9a795ee8962916b35d39fc1efefc"]],"id":1}
```


#### getBlock
Get the block with the provided hash.

Returns a `Block`.

Example:

```
{"jsonrpc":"2.0","id":1,"method":"getBlock","params":["c0002c6b25615c0f71069f159dffddf8a0b3e529efb054402f0649e969715bdb"]}
```

Response:

```
{"jsonrpc":"2.0","result":{"block_header":{"beacon":{"checkpoint":279256,"hash_prev_block":{"SHA256":[255,198,135,145,253,40,66,175,226,220,119,243,233,210,25,119,171,217,215,188,185,190,93,116,164,234,217,67,30,102,205,46]}},"hash_merkle_root":{"SHA256":[213,120,146,54,165,218,119,82,142,198,232,156,45,174,34,203,107,87,171,204,108,233,223,198,186,218,93,102,190,186,216,27]},"version":0},"proof":{"block_sig":{"Secp256k1":{"r":[112,102,21,231,95,88,196,37,189,190,121,79,13,61,106,45,53,191,114,223,172,133,64,85,96,96,61,17,125,86,4,149],"s":[112,102,21,231,95,88,196,37,189,190,121,79,13,61,106,45,53,191,114,223,172,133,64,85,96,96,61,17,125,86,4,149],"v":0}},"influence":0},"txns":[{"inputs":[],"outputs":[{"ValueTransfer":{"pkh":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"value":50000000000}}],"signatures":[],"version":0}]},"id":1}
```


#### getOutput
Get the outputPointer that matches with the input provided.

Returns an `OutputPointer`.

Example:

```
{"jsonrpc": "2.0","method": "getOutput", "params": {"transaction_id":{"SHA256":[17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17]},"output_index":1}, "id": "1"}
```

Response:

```
{"jsonrpc":"2.0","result":{"DataRequest":{"backup_witnesses":0,"commit_fee":0,"data_request":{"aggregate":{"script":[0]},"consensus":{"script":[0]},"deliver":[{"kind":"HTTP-GET","url":"https://hooks.zapier.com/hooks/catch/3860543/l2awcd/"}],"not_before":0,"retrieve":[{"kind":"HTTP-GET","script":[0],"url":"https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22"}]},"pkh":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"reveal_fee":0,"tally_fee":0,"time_lock":0,"value":0,"witnesses":0}},"id":"1"}
```

#### getPkh
Get the public key hash of the node. This pkh is used for mining blocks and resolving data requests.

Returns a `PublicKeyHash`.

Example:

```
{"jsonrpc":"2.0","id":1,"method":"getPkh"}
```

Response:

```
{"jsonrpc":"2.0","result":"wit1zgt506j2yypm8zmjzwkx6elffxkajm8m9z2cdu","id":1}
```

#### getReputation
Get the reputation of the given identity by address.

Returns a tuple of `(Reputation, bool)`, where `Reputation` is a `u32` and the boolean
indicates whether this identity is active.

Example:

```
{"jsonrpc":"2.0","id":1,"method":"getReputation","params":["wit1x23wtfjyh6l47lywazefjpr3vq25lyspjf0p8z"]}
```

Response:

```
{"jsonrpc":"2.0","result":[1,true],"id":1}
```

In this case, the identity `wit1x23wtfjyh6l47lywazefjpr3vq25lyspjf0p8z` has 1 reputation
point and is active.

#### getReputationAll
Get the reputation of all the identities in the system.

Returns a map of `PublicKeyHash => (Reputation, bool)`, where `Reputation` is a `u32` and the boolean
indicates whether this identity is active.

Example:

```
{"jsonrpc":"2.0","id":1,"method":"getReputationAll"}
```

Response:

```
{"jsonrpc":"2.0","result":{"wit1x23wtfjyh6l47lywazefjpr3vq25lyspjf0p8z":[1,true],"wit1hzm7jutjazy2yh9n7fmaq30dxf5k6py39uwq0x":[1,true]},"id":1}
```

In this case, there are two identities with 1 reputation point each, and both are active.

### sendRequest
Create and broadcast a data request transaction with the given `DataRequestOutput` and fee.

Returns the transaction `Hash`, or an error.

Example:

```
{"jsonrpc":"2.0","method":"sendRequest","id":1,"params":{"dro":{"data_request":{"not_before":0,"retrieve":[{"kind":"HTTP-GET","url":"https://api.coindesk.com/v1/bpi/currentprice.json","script":[152, 83, 204, 132, 146, 1, 163, 98, 112, 105, 204, 132, 146, 1, 163, 85, 83, 68, 204, 132, 146, 1, 170, 114, 97, 116, 101, 95, 102, 108, 111, 97, 116, 204, 130]}],"aggregate":{"script":[145,  146,  102,  32]},"consensus":{"script":[145,  146, 102,  32]},"deliver":[{"kind":"HTTP-GET","url":"https://hooks.zapier.com/hooks/catch/3860543/l2awcd/"},{"kind":"HTTP-GET","url":"https://hooks.zapier.com/hooks/catch/3860543/l1awcw/"}]},"value":1002,"witnesses":2,"backup_witnesses":1,"commit_fee":0,"reveal_fee":0,"tally_fee":0,"time_lock":0},"fee":10}}
```

Response:

```
{"jsonrpc":"2.0","result":"d0843d21f5b4185741c0bf1f9c05432079ea901f28516578dd2f5cc58f98b443","id":1}
```

#### sendValue
Create and broadcast a value transfer transaction with the given list of `ValueTransferOutput`s and fee.

A `ValueTransferOutput` is defined as

```
{
  "pkh": "wit173fkrq3cnxvsw93j6hudhafjdu6xgct6lcgm9w",
  "value: 1000,
}
```

Returns the transaction `Hash`, or an error.

Example:

```
{"jsonrpc":"2.0","method":"buildValueTransfer","id":1,"params":{"vto":[{"pkh":"wit173fkrq3cnxvsw93j6hudhafjdu6xgct6lcgm9w","value":1000}],"fee":10}}
```

Response:

```
{"jsonrpc":"2.0","result":"ab556296e88ca53a6a8a0a71dcc2acc8589a576aa3fd4c9fd33a3e9dd62c64ac","id":1}
```

#### status
Returns an object containing some information about the node:

* Chain beacon (current epoch and hash of the previous block)
* Number of inbound and outbound peers
* Is the node fully synchronized?
* How many active identities are there in the system?
* What is the total active reputation?

```json
{
  "chainBeacon": {
    "checkpoint": 576448,
    "hashPrevBlock":"eb1a106824538b226454423d7e988b0ec72ce74b9b28f5d0252de2381d41d405"
  },
  "numPeersInbound": 1,
  "numPeersOutbound": 1,
  "synchronized": true,
  "numActiveIdentities": 2,
  "totalActiveReputation": 2,
}
```

Example:

```
{"jsonrpc":"2.0","id":1,"method":"status"}
```

Response:

```
{"jsonrpc":"2.0","result":{"chainBeacon":{"checkpoint":33559,"hashPrevBlock":"04b362872b4b47c40c120982c8c89c75d422e5ccb0adbc8643a7f0b44c6495eb"},"numActiveIdentities":2,"numPeersInbound":2,"numPeersOutbound":1,"synchronized":true,"totalActiveReputation":2},"id":1}
```

[json_rpc_server]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/json_rpc/server.rs
[noders]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/node.rs
[json_rpc_methods]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/json_rpc/json_rpc_methods.rs
[json_rpc_specs]: https://www.jsonrpc.org/specification
[json_rpc_docs]: ../../interface/json-rpc/
[configuration]: ../../configuration/toml-file/
[pubsub]: ../../interface/pub-sub/
