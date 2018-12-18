# Environment defaults

Environments create contention between different instances of the Witnet network (e.g.: testnet vs. mainnet).

Each environment comes with a set of default values which you can later override in the configuration file. You can specify which environment to use in the `witnet.toml` configuration file.

At the moment, the available environments are: `testnet-1` and `mainnet`.

## Defaults for Testnet-1

| Section               | Param                            | Default Value              | Description                                                         |
|-----------------------|----------------------------------|----------------------------|---------------------------------------------------------------------|
| `connections`         | `server_addr`                    | `"127.0.0.1:21337"`        | Server socket address to which it should bind to                    |
| `connections`         | `inbound_limit`                  | `128`                      | Maximum number of concurrent connections the server should accept   |
| `connections`         | `outbound_limit`                 | `8`                        | Maximum number of opened connections to other peers this node has   |
| `connections`         | `known_peers`                    | `[]`                       | Other peer addresses this node knows about at start                 |
| `connections`         | `bootstrap_peers_period_seconds` | `5`                        | Period of the outbound peer bootstrapping process (in seconds)      |
| `connections`         | `storage_peers_period_seconds`   | `30`                       | Period of the known peers backup into storage process (in seconds)  |
| `connections`         | `handshake_timeout_seconds`      | `5`                        | Timeout for the handshake process (in seconds)                      |
| `storage`             | `db_path`                        | `".witnet-rust-testnet-1"` | Directory containing the database files                             |
| `consensus_constants` | `checkpoint_zero_timestamp`      | `9_999_999_999_999`        | Timestamp at checkpoint 0 (the start of epoch 0)                    |
| `consensus_constants` | `checkpoints_period_seconds`     | `90`                       | Seconds between the start of an epoch and the start of the next one |
| `jsonrpc`             | `enabled`                        | `true`                     | Enable JSON-RPC server                                              |
| `jsonrpc`             | `server_address`                 | `"127.0.0.1:21338"`        | JSON-RPC server socket address                                      |
| `mining`              | `enabled`                        | `true`                     | Enable MiningManager                                                |

## Defaults for Mainnet

| Section               | Param                            | Default Value              | Description                                                         |
|-----------------------|----------------------------------|----------------------------|---------------------------------------------------------------------|
| `connections`         | `server_addr`                    | `"127.0.0.1:11337"`        | Server socket address to which it should bind to                    |
| `connections`         | `inbound_limit`                  | `128`                      | Maximum number of concurrent connections the server should accept   |
| `connections`         | `outbound_limit`                 | `8`                        | Maximum number of opened connections to other peers this node has   |
| `connections`         | `known_peers`                    | `[]`                       | Other peer addresses this node knows about at start                 |
| `connections`         | `bootstrap_peers_period_seconds` | `5`                        | Period of the outbound peer bootstrapping process (in seconds)      |
| `connections`         | `storage_peers_period_seconds`   | `30`                       | Period of the known peers backup into storage process (in seconds)  |
| `connections`         | `handshake_timeout_seconds`      | `5`                        | Timeout for the handshake process (in seconds)                      |
| `storage`             | `db_path`                        | `".witnet-rust-mainnet"`   | Directory containing the database files                             |
| `consensus_constants` | `checkpoint_zero_timestamp`      | `19_999_999_999_999`       | Timestamp at checkpoint 0 (the start of epoch 0)                    |
| `consensus_constants` | `checkpoints_period_seconds`     | `90`                       | Seconds between the start of an epoch and the start of the next one |
| `jsonrpc`             | `enabled`                        | `true`                     | Enable JSON-RPC server                                              |
| `jsonrpc`             | `server_address`                 | `"127.0.0.1:11338"`        | JSON-RPC server socket address                                      |
| `mining`              | `enabled`                        | `true`                     | Enable MiningManager                                                |
