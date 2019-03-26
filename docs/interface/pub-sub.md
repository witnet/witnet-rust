# Publish/Subscribe API

The Witnet JSON-RPC API has support for subscriptions to certain events.

## Overview

Example: subscribe to new blocks. The client will get a notification every
time a new block is consolidated.

## Methods

### witnet_subscribe

Used to add new subscriptions.

#### Parameters

`witnet_subscribe` expects an array:

`[methodName, methodParams]`

`methodName` must be a string.

`methodParams` is optional, as some subscriptions don't accept any parameters.
It must be either an object, an array, or null, depending on `methodName`.

#### Returns

On success returns a string, the `subscriptionId`.

#### Example

Request: subscribe to new blocks.

```json
{"jsonrpc":"2.0","method":"witnet_subscribe","params":["newBlocks"],"id":"1"}
```

Response: subscription id "9876"

```json
{"jsonrpc":"2.0","result":"9876","id":"1"}
```

### witnet_unsubscribe

Used to remove subscriptions.

#### Parameters

`witnet_unsubscribe` expects a one-element array:

`[subscriptionId]`

Where `subscriptionId` is a string, previously returned by the
`witnet_subscribe` method.

#### Returns

`true` on success, `false` if the subscription id does not exist.

#### Example

Request: unsubscribe from subscription id "9876"

```json
{"jsonrpc":"2.0","method":"witnet_unsubscribe","params":["9876"],"id":1}
```

Response: success
```json
{"jsonrpc":"2.0","id":1,"result":true}
```

## Subscriptions

These are currently the available subscriptions.

### newBlocks

Receive a notification every time a block is consolidated.

#### Parameters

None.

#### Returns

A block.

#### Example

Notification: a new block has been consolidated.

```json
{"jsonrpc":"2.0","method":"witnet_subscription","params":{"result":{"block_header":{"beacon":{"checkpoint":274297,"hash_prev_block":{"SHA256":[147,238,4,62,34,70,88,121,107,43,13,106,167,20,108,200,207,29,183,254,26,98,89,183,233,58,76,76,20,61,47,165]}},"hash_merkle_root":{"SHA256":[213,120,146,54,165,218,119,82,142,198,232,156,45,174,34,203,107,87,171,204,108,233,223,198,186,218,93,102,190,186,216,27]},"version":0},"proof":{"block_sig":{"Secp256k1":{"r":[235,115,251,78,16,196,71,30,21,236,76,153,62,165,6,59,177,159,23,82,111,42,134,242,189,83,91,212,155,97,88,57],"s":[235,115,251,78,16,196,71,30,21,236,76,153,62,165,6,59,177,159,23,82,111,42,134,242,189,83,91,212,155,97,88,57],"v":0}},"influence":0},"txns":[{"inputs":[],"outputs":[{"ValueTransfer":{"pkh":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"value":50000000000}}],"signatures":[],"version":0}]},"subscription":"9876"}}
```
