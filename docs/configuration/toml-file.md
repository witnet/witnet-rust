# Custom TOML configuration file

A custom `witnet.toml` file can be used to configure parameters of the node. In order for the node to be able to read this file, it should exist in the current working directory where the node is run. Another way is to just tell the node where the config file resides using a command line option. See the CLI reference for more info.

## TOML file example

``` toml
environment = "testnet-1" # or "mainnet"

[connections] # section for connections-related params
server_addr = "127.0.0.1:1234"
inbound_limit = 30

[storage] # section for storage-related params
db_path = ".wit"

# ... more options
```

## Configuration params

| Section     | Param          | Default Value       | Description                                                       |
| ---------   | ----------     | --------------      | -----------------------------------                               |
|             | environment    | `"testnet-1"`       | Environment in which the Witnet protocol will run                 |
| connections | server_addr    | `"127.0.0.1:21337"` | Server socket address to which it should bind to                  |
| connections | inbound_limit  | `128`               | Maximum number of concurrent connections the server should accept |
| connections | outbound_limit | `8`                 | Maximum number of opened connections to other peers this node has |
| connections | known_peers    | `[]`                | Other peer addresses this node knows about at start               |
| storage     | db_path        | `".wit"`            | Directory containing the dabase files                             |

