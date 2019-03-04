# JsonRpcServer

The [JSON-RPC interface][json_rpc_docs] is implemented using a `JsonRpcServer` actor
which handles the new incoming connections and spawns a `JsonRpc` actor for each
new connection.

The `JsonRpc` actor handles the JSON-RPC protocol, parses the input stream as JSON-RPC,
executes the request and generates an appropriate response.

The supported JSON-RPC methods are implemented in [`json_rpc_methods.rs`][json_rpc_methods].

See [JSON-RPC][json_rpc_docs] for further information.

## API

The `JsonRpcServer` does not expose any public API, once it is started
it will read the necessary parameters from the `ConfigManager` and
automatically handle all the incoming connections.

### Incoming: Others -> JsonRpcServer

These are the messages supported by the `JsonRpcServer`:

| Message             | Input type      | Output type | Description                                                   |
|---------------------|-----------------|-------------|---------------------------------------------------------------|
| `InboundTcpConnect` | `TcpStream`     | `()`        | Request to create a session from an incoming TCP connection   |
| `Unregister`        | `Addr<JsonRpc>` | `()`        | Removes a closed connection from the list of open connections |

However, they are internal messages: the `InboundTcpConnect` is sent
by the stream listener and the `Unregister` is sent by the `JsonRpc`
actor.

### Outgoing messages: JsonRpcServer -> Others

These are the messages sent by the EpochManager:

| Message                | Destination     | Input type | Output type                 | Description                                             |
|------------------------|-----------------|------------|-----------------------------|---------------------------------------------------------|
| `GetConfig`            | `ConfigManager` | `()`       | `Result<Config, io::Error>` | Request the configuration                               |

#### GetConfig

This message is sent to the [`ConfigManager`][config_manager] actor when the `JsonRpcServer` actor is started.

The return value is used to initialize the protocol constants (checkpoint period and
epoch zero timestamp).
For further information, see [`ConfigManager`][config_manager].

### Incoming: Others -> JsonRpc

The `JsonRpc` actor does not have any message handlers.

### Outgoing messages: JsonRpc -> Others

| Message             | Destination     | Input type      | Output type | Description                                                   |
|---------------------|-----------------|-----------------|-------------|---------------------------------------------------------------|
| `Unregister`        | `JsonRpcServer` | `Addr<JsonRpc>` | `()`        | Removes a closed connection from the list of open connections |

The `JsonRpc` actor only sends one `Unregister` message to its parent
when the connection closes.

## Further information

The full source code of the `JsonRpcServer` can be found at [`server.rs`][json_rpc_server].

[json_rpc_server]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/json_rpc/server.rs
[config_manager]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/config_manager.rs
[noders]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/node.rs
[json_rpc_methods]: https://github.com/witnet/witnet-rust/blob/master/node/src/actors/json_rpc/json_rpc_methods.rs
[json_rpc_specs]: https://www.jsonrpc.org/specification
[json_rpc_docs]: ../../interface/json-rpc/