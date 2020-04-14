Refer to the [Run as systemd service][systemd-doc] documentation to read the steps on how to create a `systemd` service for your Witnet node.

Run `runner.sh` to automatically download and install the latest version of the Witnet node software (`witnet-rust`). It can also be used to update it to the latest version because it will keep all the block chain data and the configuration file in the hidden `.witnet` directory.

For Testnet 7.3 and greater, the configuration file needs to be customized with the public IP and port of the node, which must be set at the `public_addr` field in `witnet.toml` .

[systemd-doc]: https://docs.witnet.io/node-operators/systemd-service/
