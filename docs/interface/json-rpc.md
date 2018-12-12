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

@returns: boolean indicating success, or parse error

Example:

```
{"jsonrpc": "2.0", "method": "inventory", "params": {"block":{"block_header":{"version":1,"beacon":{"checkpoint":2,"hash_prev_block":{"SHA256":[4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4]}},"hash_merkle_root":{"SHA256":[3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3]}},"proof":{"block_sig":null,"influence":99999},"txns":[null]}}, "id": 1}
```

Response:

```
{"jsonrpc":"2.0","result":true,"id":1}
```

[json_rpc_server]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/json_rpc/server.rs
[noders]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/node.rs
[json_rpc_methods]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/json_rpc/json_rpc_methods.rs
[json_rpc_specs]: https://www.jsonrpc.org/specification
[json_rpc_docs]: ../../interface/json-rpc/
[configuration]: ../../configuration/toml-file/