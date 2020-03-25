Refer to the [Run as systemd service](https://github.com/witnet/witnet-rust/blob/master/docs/get-started/installation/from-source.md#run-as-systemd-service) documentation to read the steps on how to create a `systemd` service for your Witnet node.

Run `runner.sh` to automatically download and install the latest version of the Witnet node software (`witnet-rust`). It can also be used to update it to the latest version because it will keep all the block chain data and the configuration file in the hidden `.witnet` directory.

For Testnet 7.3 and greater, the configuration file needs to be customized with the public IP and port of the node, which must be set at the `public_addr` field in `witnet.toml` .
