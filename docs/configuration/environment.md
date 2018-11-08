# Environment defaults

Environments create contention between different instances of the Witnet network (e.g.: testnet vs. mainnet).

Each environment comes with a set of default values which you can later override in the configuration file. You can specify which environment to use in the `witnet.toml` configuration file.

At the moment, the available environments are: `testnet-1` and `mainnet`.

## Defaults for Testnet-1

| Section     | Param          | Default Value       | Description                                                       |
| ---------   | ----------     | --------------      | -----------------------------------                               |
| connections | server_addr    | `"127.0.0.1:21337"` | Server socket address to which it should bind to                  |
| connections | inbound_limit  | `128`               | Maximum number of concurrent connections the server should accept |
| connections | outbound_limit | `8`                 | Maximum number of opened connections to other peers this node has |
| connections | known_peers    | `[]`                | Other peer addresses this node knows about at start               |
| storage     | db_path        | `".wit"`            | Directory containing the dabase files                             |


## Defaults for Mainnet

| Section     | Param          | Default Value       | Description                                                       |
| ---------   | ----------     | --------------      | -----------------------------------                               |
| connections | server_addr    | `"127.0.0.1:11337"` | Server socket address to which it should bind to                  |
| connections | inbound_limit  | `128`               | Maximum number of concurrent connections the server should accept |
| connections | outbound_limit | `8`                 | Maximum number of opened connections to other peers this node has |
| connections | known_peers    | `[]`                | Other peer addresses this node knows about at start               |
| storage     | db_path        | `".wit"`            | Directory containing the dabase files                             |

