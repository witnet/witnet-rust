# Wallet

The Witnet wallet backend is a Rust program which manages the user keys.
It can be used to sign transactions and send them to the Witnet node over JSON-RPC.

The wallet itself provides a JSON-RPC API over WebSockets, which is useful
for the Sheikah client.

## Subscriptions

The Witnet wallet provides a pub/sub API, [see here for more info][pubsub].

## Methods

The following methods are available:

    createDataRequest(data_request_args) -> DataRequest
    createMnemonics() -> Mnemonics
    createWallet(name, password) -> Wallet
    generateAddress(wallet_id) -> Address
    getTransactions(wallet_id, limit, page) -> Vec<Transaction>
    getWalletInfos() -> Vec<WalletInfos>
    importSeed(mnemonics / xpriv)
    lockWallet(wallet_id, wipe=false)
    runDataRequest(data_request) -> RadonValue
    sendDataRequest(data_request)
    sendVTT(wallet_id, to_address, amount, fee, subject) -> Transaction
    unlockWallet(id, password) -> Wallet

### createDataRequest

```
createDataRequest(data_request_args) -> DataRequest
```

Constructs a Data Request.

### createMnemonics

```
createMnemonics() -> Mnemonics
```

Returns new randomly-generated mnemonics compliant with BIP-39.

The mnemonics are a list of words like the following one:

```
choice spray absent olympic obey talk magnet exchange weekend skate camera segment nose canoe fatigue
```

### createWallet

```
createWallet(name, password) -> Wallet
```

Creates a new wallet with the given name and password.

### generateAddress

```
generateAddress(wallet_id) -> Address
```

Returns a new address freshly derived from the given wallet's master key.

### getTransactions

```
getTransactions(wallet_id, limit, page) -> Vec<Transaction>
```

Returns the list of transactions related to the given wallet.

### getWalletInfos

Returns the list of available wallets.

### importSeed

```
importSeed(mnemonics)
importSeed(xpriv)
```

### lockWallet

```
lockWallet(wallet_id, wipe=false)
```

Locks the given wallet.
### runDataRequest

```
runDataRequest(data_request) -> RadonValue
```

Executes a Data Request and returns the RadonValue.

### sendDataRequest

```
sendDataRequest(data_request)
```

Constructs a Data Request Transaction.

### sendVtt

```
sendVTT(wallet_id, to_address, amount, fee, subject) -> Transaction
```

Constructs a Value Transfer Transaction.

### unlockWallet

Unlocks the given wallet.

```
unlockWallet(id, password) -> Wallet
```

[pubsub]: ../../interface/pub-sub/
