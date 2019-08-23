# How to run a Witnet-Ethereum bridge node

## Ethereum (local)

The following steps are for local testing with Ganache.
For instructions on how to run in a public Ethereum testnet, see below.

    ganache-cli -b 10 -m scatter clap print else pioneer goat monster clay mystery question deposit measure

This will start an Ethereum client at 127.0.0.1:8545
The `-b 10` flag is important because it sets the block time to 10 seconds.
This allows us to wait for confirmations instead of blindly assuming that the
transactions get accepted.

Copy the first account to witnet-rust/witnet_ethereum_bridge.toml as eth_account
It must be the first account if we want to use the BlockRelay, since currently
the only account that can relay Witnet blocks to Ethereum is the one that
deployed the BlockRelay contract, which by default is the first account.

In order to deploy the contracts, clone the [witnet-ethereum-bridge](https://github.com/witnet/witnet-ethereum-bridge)
Enter the `witnet-ethereum-bridge` directory and run

    truffle deploy --network local

If the command results in an error complaining about "network local does not exist", just add it to `truffle-config.js`, under networks:

    local: {
      host: "127.0.0.1",     // Localhost (default: none)
      port: 8545,            // Standard Ethereum port (default: none)
      network_id: "*",       // Any network (default: none)
    },


Copy BlockRelay contract address to witnet-rust/witnet_ethereum_bridge.toml
Copy WBI contract address to witnet-rust/witnet_ethereum_bridge.toml

If the api of the contracts was modified recently, you have to manually copy
the ABI from

    witnet-ethereum-bridge/build/contracts/BlockRelay.json

(only the contents of the "abi" key of the JSON) to

    witnet-rust/bridges/ethereum/block_relay_abi.json

(the contents of block_relay_abi.json should be a JSON array)

Same for the WBI: copy

    witnet-ethereum-bridge/build/contracts/WitnetBridgeInterface.json

to

    witnet-rust/bridges/ethereum/wbi_abi.json

## Ethereum (Ropsten)

First you have to synchronize with a ethereum node. It is recommended use a light node.

    ./geth --testnet --syncmode "light" --cache=2048 --rpc --rpcport=8545 --rpcapi=eth,web3,net,personal --allow-insecure-unlock

After your node is synced, open a new terminal and connect with the node

    ./geth attach <ethereum_path>/.ethereum/testnet/geth.ipc

Create a new account and unlocked it:

    personal.newAccount(<password>)
    personal.unlockAccount(<account>, <password>, 999999)

Copy this account to witnet-rust/witnet_ethereum_bridge.toml as eth_account. To deploy the contracts it is necessary first to send Eth to this account (using Metamask for example)

In order to deploy the contracts, clone the [witnet-ethereum-bridge](https://github.com/witnet/witnet-ethereum-bridge)
Enter the `witnet-ethereum-bridge` directory and run

    truffle deploy --network ropsten

If the command results in an error complaining about "network local does not exist", just add it to `truffle-config.js`, under networks:

    ropsten: {
      network_id: 3,       // Ropsten's id
      host: "127.0.0.1",   // Localhost (default: none)
      port: 8545,          // Standard Ethereum port (default: none)
      gas: 5500000,        // Ropsten has a lower block limit than mainnet
      confirmations: 2,    // # of confs to wait between deployments. (default: 0)
      timeoutBlocks: 200,  // # of blocks before a deployment times out  (minimum/default: 50)
      skipDryRun: true     // Skip dry run before migrations? (default: false for public nets )
    },


Copy BlockRelay contract address to witnet-rust/witnet_ethereum_bridge.toml
Copy WBI contract address to witnet-rust/witnet_ethereum_bridge.toml

If the api of the contracts was modified recently, you have to manually copy
the ABI from

    witnet-ethereum-bridge/build/contracts/BlockRelay.json

(only the contents of the "abi" key of the JSON) to

    witnet-rust/bridges/ethereum/block_relay_abi.json

(the contents of block_relay_abi.json should be a JSON array)

Same for the WBI: copy

    witnet-ethereum-bridge/build/contracts/WitnetBridgeInterface.json

to

    witnet-rust/bridges/ethereum/wbi_abi.json
## Witnet

Run the node:

    RUST_LOG=witnet=debug cargo run -- -c witnet.toml node run

This will start a JSONRPC client at 127.0.0.1:21338
This node will connect to the Witnet testnet. Wait until the node is SYNCED.
For the correct functionality, the node should have an account with enough
value to post the data requests. The simplest way to achieve that is to mine
a block.

## Bridge

Start the bridge:

    RUST_LOG=witnet_ethereum_bridge=debug cargo run -p witnet-ethereum-bridge

It will try to subscribe to blocks from Witnet and WBI events from Ethereum.

If that's successful, we can post a data request to the WBI:

    cargo run -p witnet-ethereum-bridge -- --post-dr

This will send a data request querying the price of bitcoin.

* The bridge will be listening to events from the WBI, so when the transaction
gets accepted into a block, the WBI contract will emit a `PostDataRequest` event.

* The bridge will call `claimDataRequests` in order to claim the data request,
and when that transaction is accepted, the bridge will post the data request to
Witnet.

* When this data request is included in a Witnet block, the bridge will send the
proof of inclusion to the WBI contract. This will emit an `InclusionDataRequest`
event, which indicates to all the bridge nodes that they should start checking
all the new Witnet blocks for a tally which resolves that data request.

* Once a tally has been included in a block, any bridge node can sent the proof
of inclusion. If that proof is valid, the WBI contract will emit a `PostResult`
event indicating that the data request has been resolved.

## Block Relay

A crucial component needed for the correct functionality of the bridge is the
block relay: an Ethereum contract that stores the headers of Witnet blocks.
The current version of the bridge also acts as a block relay, but that can be
disabled in the configuration file.
