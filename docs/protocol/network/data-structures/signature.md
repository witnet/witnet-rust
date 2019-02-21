# Signature

__Signatures__ are a tagged union of the supported cryptosystems:

| Kind                 | Description          |
|:---------------------|:---------------------|
| `Secp256k1Signature` | ECDSA over secp256k1 |


__Keyed signatures__ augment the previous format by adding a field for the public key that was used for producing the signature:

| Field        | Type        | Description                                                                |
|:-------------|:------------|:---------------------------------------------------------------------------|
| `signature`  | `Signature` | A variable-length digital signature                                        |
| `public_key` | `bytes`     | The public key matching the private key used for producing the `signature` |


## Cryptosystems

Currently supported cryptosystems within the Witnet network protocol:

| Cryptosystem         | Signature size | Public key size |
|----------------------|----------------|-----------------|
| None                 | 0 bytes        | 0 bytes         |
| ECDSA over secp256k1 | 65 bytes       | 33 bytes        |


### Secp256k1Signature

ECDSA signatures over the `secp256k1` curve consist of:

| Field | Type       | Description           |
|-------|------------|-----------------------|
| `r`   | `bytes` | The signature value R (32 bytes) |
| `s`   | `bytes` | The signature value S (33 bytes) |

ECDSA public keys must always use compression and thus their length is 33 bytes.