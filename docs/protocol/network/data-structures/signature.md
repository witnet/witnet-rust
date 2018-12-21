# Signature

__Signatures__ are formatted as follows:

| Field        | Type           | Description                                       |
|--------------|----------------|---------------------------------------------------|
| `crypto_sys` | `cryptosystem` | A cryptosystem identifier (see enumeration below) |
| `signature`  | `[u8]`         | A variable-length digital signature               |

__Keyed signatures__ augment the previous format by adding a field for the public key that was used for producing the signature:

| Field        | Type           | Description                                                                |
|--------------|----------------|----------------------------------------------------------------------------|
| `crypto_sys` | `cryptosystem` | A cryptosystem identifier (see enumeration below)                          |
| `signature`  | `[u8]`         | A variable-length digital signature                                        |
| `public_key` | `[u8]`         | The public key matching the private key used for producing the `signature` |


## Cryptosystems

Currently supported cryptosystems within the Witnet network protocol:

| Identifier (dec) | Identifier (hex) | Cryptosystem         | Signature size | Public key size |
|------------------|------------------|----------------------|----------------|-----------------|
| `0`              | `0x00000000`     | None                 | 0 bytes        | 0 bytes         |
| `1`              | `0x00000001`     | ECDSA over secp256k1 | 65 bytes       | 33 bytes        |

### ECDSA over secp256k1

ECDSA signatures over the `secp256k1` curve consist of:

| Field | Type       | Description           |
|-------|------------|-----------------------|
| `r`   | `[u8; 32]` | The signature value R |
| `s`   | `[u8; 33]` | The signature value S |

ECDSA public keys must always use compression and thus their type is `[u8; 33]`.