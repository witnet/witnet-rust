# Signature

Signatures are formatted as follows:

| Field        | Type        | Description               |
| ------------ | :---------: | ------------------------- |
| `crypto_sys` | `u32`       | A cryptosystem identifier |
| `signature`  | `signature` | A digital signature       |

## Cryptosystems

Currently supported cryptosystems within the Witnet network protocol:

| Identifier (dec) | Identifier (hex) | Cryptosystem         | Signature size |
| ---------------- | :--------------: | -------------------- | -------------- |
| `0`              | `0x00000000`     | None                 | 0 bytes        |
| `1`              | `0x00000001`     | ECDSA over secp256k1 | 65 bytes       |

### ECDSA over secp256k1

ECDSA signatures over the curve secp256k1 consist of:

| Field | Type       | Description           |
| ----- | :--------: | --------------------- |
| `r`   | `[u8; 32]` | The signature value R |
| `s`   | `[u8; 33]` | The signature value S |
