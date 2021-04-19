# How to run a Witnet-Ethereum bridge node

This tutorial explains how to run a brige node in the **centralized Witnet-Ethereum bridge**.

The goal of the bridge nodes is to monitor the WitnetRequestBoard contract looking for data request candidates to be introduced in Witnet and to deliver the result derived by the witnesses.

Before running the bridge node you will need to have the WintetRequestsBoard contract deployed in an Ethereum enviroment. If you already have those contracts deployed, skip the *contract deployment* steps.

Both the WitnetRequestBoard and the BlockRelay contracts have been deployed in Rinkeby and Goerli testnets. The addresses can be found [here](https://github.com/witnet/witnet-requests-js/blob/master/src/ethereum/addresses.js).

## Ethereum node

The following sections describe how to connect to an ethereum client and how to deploy the contracts both locally and in testnet environments.

### Connecting to a local Ethereum node

The following steps are for local testing of the bridge with Ganache. These assume that you have both [ganache-cli](https://www.trufflesuite.com/ganache) and [truffle](https://www.trufflesuite.com/docs/truffle/getting-started/installation) installed in your system. If you would like to test the bridge node with contracts deployed in an Ethereum environment, check the following sections.

    ganache-cli -b 10 -networkId <Id> -m scatter clap print else pioneer goat monster clay mystery question deposit measure

This will start an Ethereum client at 127.0.0.1:8545 with network id `<Id>`.
The `-b 10` flag is important because it sets the block time to 10 seconds.
This allows us to wait for confirmations instead of blindly assuming that the
transactions get accepted.

#### Deploy the contracts

In order to deploy the contracts, clone the [witnet-ethereum-bridge](https://github.com/witnet/witnet-ethereum-bridge)
Enter the `witnet-ethereum-bridge` directory and run

    truffle deploy --network local

If the command results in an error complaining about "network local does not exist", just add it to `truffle-config.js`, under networks:

    local: {
      host: "127.0.0.1",     // Localhost (default: none)
      port: 8545,            // Standard Ethereum port (default: none)
      network_id: <Id>,       // The network Id we spun ganache-cli with
    },

If everything goes well you should see ganache-cli responding to the deployments.

### Connecting to an Ethereum testnet node (Rinkeby/Goerli/Ropsten)

In this section it is assumed you have [geth](https://github.com/ethereum/go-ethereum/wiki/Installing-Geth) installed in your machine. The first thing you need to do is setup an account if you do not have one yet. This can be done by typing:
    
    ./geth account new

This will create an account that you will need to unlock when synchronizing with the appropriate testnet. The key from which this account has been created is stored in your /home/user/.ethereum/keystore path.

It is time to connect to one of the Ethereum testnets. In this case we are going to connect to *goerli*, but you can change the name to connect to a testnet of your choice. It is recommended use a light node.

    ./geth --goerli --rpc --rpcaddr 0.0.0.0 --rpcport 8545 --syncmode light --cache 2048 --rpcapi=eth,web3,net,personal --allow-insecure-unlock --unlock YOUR_ACCOUNT --keystore /home/user/.ethereum/keystore/

#### Deploying the contracts

In order to deploy the contracts, clone the [witnet-ethereum-bridge](https://github.com/witnet/witnet-ethereum-bridge)
Enter the `witnet-ethereum-bridge` directory and run

    truffle deploy --network goerli

If the command results in an error complaining about "network local does not exist", just add it to `truffle-config.js`, under networks:

    goerli: {
      network_id: 5,       // Goerli's id
      host: "127.0.0.1",   // Localhost (default: none)
      port: 8545,          // Standard Ethereum port (default: none)
      gas: 5500000,        // Goerli has a lower block limit than mainnet
      confirmations: 2,    // # of confs to wait between deployments. (default: 0)
      timeoutBlocks: 200,  // # of blocks before a deployment times out  (minimum/default: 50)
      skipDryRun: true     // Skip dry run before migrations? (default: false for public nets )
    },

Make sure you have enough eth to deploy the aforementioned contracts.

If you want to check the addresses at which you deployed the contracts, you just need to type

    truffle networks

which will show you the networks Ids with the corresponding contract addresses.

## Witnet node
As menetioned in the beggining of this document, you need a Witnet node the bridge node can connect to.

Run the node:

    RUST_LOG=witnet=debug cargo run -- -c witnet.toml node run

This will start a JSONRPC client at 127.0.0.1:21338
This node will connect to the Witnet testnet. Wait until the node is SYNCED.
For the correct functionality, the node should have an account with enough
value to post the data requests. The simplest way to achieve that is to mine
a block.

## Bridge node

### Configure the witnet_ethereum_bridge.toml

We need to modify the configuration file that the bridge will use to establish connections to both the Witnet node and the ethereum client:

    witnet-rust/witnet_ethereum_bridge.toml


There are some key fields that need to be edited here to make the bridge connect properly to your Ethereum/Witnet nodes. These are:

- *witnet_jsonrpc_addr*: make sure this address is identical to the jsonRPC address of your Witnet node.
- *eth_client_url*: make sure this field points to address where your ethereum client is running.
- *wrb_contract_addr*: this field should contain the address of the WitnetRequestsBoard contract you wish your node to connect to.
- *block_relay_contract_add*: this field should contain the address of the BlockRelay contract you wish your node to connect to.
- *eth_account*: this is the account you are using in your ethereum client. 
    - **NOTE**: In the case of using ganache-cli, this needs to be the first account, since currently the only account that can relay Witnet blocks to Ethereum is the one that
    deployed the BlockRelay contract, which by default is the first account.

### Copying the ABIs
If the api of the contracts was modified recently, you have to manually copy
the ABI from

    witnet-ethereum-bridge/build/contracts/BlockRelay.json

(only the contents of the "abi" key of the JSON) to

    witnet-rust/bridges/ethereum/block_relay_abi.json

(the contents of block_relay_abi.json should be a JSON array)

Same for the WRB: copy

    witnet-ethereum-bridge/build/contracts/WitnetBridgeInterface.json

to

    witnet-rust/bridges/ethereum/wrb_abi.json

### Start the bridge:
In order to start the bridge node you just need to execute the following command:

    RUST_LOG=witnet_ethereum_bridge=debug cargo run -p witnet-ethereum-bridge

It will try to subscribe to blocks from Witnet and WRB events from Ethereum.

If that's successful, we can post a data request to the WRB:

    cargo run -p witnet-ethereum-bridge -- --post-dr

This will send a data request querying the price of bitcoin.

* The bridge will be listening to events from the WRB, so when the transaction
gets accepted into a block, the WRB contract will emit a `PostDataRequest` event.

* The bridge will call `claimDataRequests` in order to claim the data request,
and when that transaction is accepted, the bridge will post the data request to
Witnet.

* When this data request is included in a Witnet block, the bridge will send the
proof of inclusion to the WRB contract. This will emit an `InclusionDataRequest`
event, which indicates to all the bridge nodes that they should start checking
all the new Witnet blocks for a tally which resolves that data request.

* Once a tally has been included in a block, any bridge node can sent the proof
of inclusion. If that proof is valid, the WRB contract will emit a `PostResult`
event indicating that the data request has been resolved.
