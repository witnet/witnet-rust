# Constants

## Types

| Byte   | Decimal | Constant       |
|:-------|:--------|:---------------|
| `0x00` | `0`     | `TYPE_BOOLEAN` |
| `0x01` | `1`     | `TYPE_INT`     |
| `0x02` | `2`     | `TYPE_FLOAT`   |
| `0x03` | `3`     | `TYPE_STRING`  |
| `0x04` | `4`     | `TYPE_ARRAY`   |
| `0x05` | `5`     | `TYPE_MAP`     |
| `0x06` | `6`     | `TYPE_BYTES`   |
| `0x07` | `7`     | `TYPE_NULL`    |
| `0x08` | `8`     | `TYPE_RESULT`  |

## Operators

### Universal operators
Range `0x00` to `0x0F` is reserved for operators that operate on any of
the RADON type.

| Byte   | Decimal | Constant      |
|:-------|:--------|:--------------|
| `0x00` | `0`     | `OP_ANY_NOOP` |

### `Boolean` operators
Range `0x10` to `0x1F` is reserved for operators that operate
exclusively on the `Boolean` type.

| Byte   | Decimal | Constant              |
|:-------|:--------|:----------------------|
| `0x10` | `16`    | `OP_BOOLEAN_MATCH`    |
| `0x11` | `17`    | `OP_BOOLEAN_NEGATE`   |
| `0x12` | `18`    | `OP_BOOLEAN_TOSTRING` |

### `Integer` operators
Range `0x20` to `0x2F` is reserved for operators that operate
exclusively on the `Integer` type.

| Byte   | Decimal | Constant                |
|:-------|:--------|:------------------------|
| `0x20` | `32`    | `OP_INTEGER_ABSOLUTE`   |
| `0x21` | `33`    | `OP_INTEGER_MATCH`      |
| `0x22` | `34`    | `OP_INTEGER_MODULO`     |
| `0x23` | `35`    | `OP_INTEGER_MULTIPLY`   |
| `0x24` | `36`    | `OP_INTEGER_NEGATE`     |
| `0x25` | `37`    | `OP_INTEGER_POWER`      |
| `0x26` | `38`    | `OP_INTEGER_RECIPROCAL` |
| `0x27` | `39`    | `OP_INTEGER_SUM`        |
| `0x28` | `40`    | `OP_INTEGER_TOFLOAT`    |
| `0x29` | `41`    | `OP_INTEGER_TOSTRING`   |

### `Float` operators
Range `0x30` to `0x3F` is reserved for operators that operate
exclusively on the `Float` type.

| Byte   | Decimal | Constant              |
|:-------|:--------|:----------------------|
| `0x30` | `48`    | `OP_FLOAT_ABSOLUTE`   |
| `0x31` | `49`    | `OP_FLOAT_CEILING`    |
| `0x32` | `50`    | `OP_FLOAT_FLOOR`      |
| `0x33` | `51`    | `OP_FLOAT_MODULO`     |
| `0x34` | `52`    | `OP_FLOAT_MULTIPLY`   |
| `0x35` | `53`    | `OP_FLOAT_NEGATE`     |
| `0x36` | `54`    | `OP_FLOAT_POWER`      |
| `0x37` | `55`    | `OP_FLOAT_RECIPROCAL` |
| `0x38` | `56`    | `OP_FLOAT_ROUND`      |
| `0x39` | `57`    | `OP_FLOAT_SUM`        |
| `0x3A` | `58`    | `OP_FLOAT_TOSTRING`   |
| `0x3B` | `59`    | `OP_FLOAT_TRUNCATE`   |

### `String` operators
Range `0x40` to `0x4F` is reserved for operators that operate
exclusively on the `String` type.

| Byte   | Decimal | Constant                |
|:-------|:--------|:------------------------|
| `0x40` | `64`    | `OP_STRING_HASH`        |
| `0x41` | `65`    | `OP_STRING_LENGTH`      |
| `0x42` | `66`    | `OP_STRING_MATCH`       |
| `0x43` | `67`    | `OP_STRING_PARSEJSON`   |
| `0x44` | `68`    | `OP_STRING_PARSEXML`    |
| `0x45` | `69`    | `OP_STRING_TOBOOLEAN`   |
| `0x46` | `70`    | `OP_STRING_TOFLOAT`     |
| `0x47` | `71`    | `OP_STRING_TOINTEGER`   |
| `0x48` | `72`    | `OP_STRING_TOLOWERCASE` |
| `0x49` | `73`    | `OP_STRING_TOUPPERCASE` |

### `Array` operators
Range `0x50` to `0x5F` is reserved for operators that operate
exclusively on the `Array` type.

| Byte   | Decimal | Constant           |
|:-------|:--------|:-------------------|
| `0x50` | `80`    | `OP_ARRAY_COUNT`   |
| `0x51` | `81`    | `OP_ARRAY_EVERY`   |
| `0x52` | `82`    | `OP_ARRAY_FILTER`  |
| `0x53` | `83`    | `OP_ARRAY_FLATTEN` |
| `0x54` | `84`    | `OP_ARRAY_GET`     |
| `0x55` | `85`    | `OP_ARRAY_MAP`     |
| `0x56` | `86`    | `OP_ARRAY_REDUCE`  |
| `0x57` | `87`    | `OP_ARRAY_SOME`    |
| `0x58` | `88`    | `OP_ARRAY_SORT`    |
| `0x59` | `89`    | `OP_ARRAY_TAKE`    |

### `Map` operators
Range `0x60` to `0x6F` is reserved for operators that operate
exclusively on the `Map` type.

| Byte   | Decimal | Constant         |
|:-------|:--------|:-----------------|
| `0x60` | `96`    | `OP_MAP_ENTRIES` |
| `0x61` | `97`    | `OP_MAP_GET`     |
| `0x62` | `98`    | `OP_MAP_KEYS`    |
| `0x63` | `99`    | `OP_MAP_VALUES`  |

### `Bytes` operators
Range `0x70` to `0x7F` is reserved for operators that operate
exclusively on the `Bytes` type.

| Byte   | Decimal | Constant             |
|:-------|:--------|:---------------------|
| `0x70` | `112`   | `OP_BYTES_TOARRAY`   |
| `0x71` | `113`   | `OP_BYTES_TOBOOLEAN` |
| `0x72` | `114`   | `OP_BYTES_TOFLOAT`   |
| `0x73` | `115`   | `OP_BYTES_TOINTEGER` |
| `0x74` | `116`   | `OP_BYTES_TOMAP`     |

### `Result` operators
Range `0x80` to `0x8F` is reserved for operators that operate
exclusively on the `Result` type.

| Byte   | Decimal | Constant          |
|:-------|:--------|:------------------|
| `0x80` | `128`   | `OP_RESULT_GET`   |
| `0x81` | `129`   | `OP_RESULT_GETOR` |
| `0x82` | `130`   | `OP_RESULT_ISOK`  |

## Hash functions

| Byte   | Decimal | Constant             |
|:-------|:--------|:---------------------|
| `0x00` | `0`     | `HASH_BLAKE_256`     |
| `0x01` | `1`     | `HASH_BLAKE_512`     |
| `0x02` | `2`     | `HASH_BLAKE2S_256`   |
| `0x03` | `3`     | `HASH_BLAKE2B_512`   |
| `0x04` | `4`     | `HASH_MD5_128`       |
| `0x05` | `5`     | `HASH_RIPEMD_128`    |
| `0x06` | `6`     | `HASH_RIPEMD_160`    |
| `0x07` | `7`     | `HASH_RIPEMD_320`    |
| `0x08` | `8`     | `HASH_SHA1_160`      |
| `0x09` | `9`     | `HASH_SHA2_224`      |
| `0x0a` | `10`    | `HASH_SHA2_256`      |
| `0x0b` | `11`    | `HASH_SHA2_384`      |
| `0x0c` | `12`    | `HASH_SHA2_512`      |
| `0x0d` | `13`    | `HASH_SHA3_224`      |
| `0x0e` | `14`    | `HASH_SHA3_256`      |
| `0x0f` | `15`    | `HASH_SHA3_384`      |
| `0x10` | `16`    | `HASH_SHA3_512`      |
| `0x11` | `17`    | `HASH_WHIRLPOOL_512` |

## Filtering functions

| Byte   | Decimal | Constant             |
|:-------|:--------|:---------------------|
| `0x00` | `0`     | `FILTER_GT`          |
| `0x01` | `1`     | `FILTER_LT`          |
| `0x02` | `2`     | `FILTER_EQ`          |
| `0x03` | `3`     | `FILTER_DEV_ABS`     |
| `0x04` | `4`     | `FILTER_DEV_REL`     |
| `0x05` | `5`     | `FILTER_DEV_STD`     |
| `0x06` | `6`     | `FILTER_TOP`         |
| `0x07` | `7`     | `FILTER_BOTTOM`      |
| `0x80` | `128`   | `FILTER_NOT_GT`      |
| `0x81` | `129`   | `FILTER_NOT_LT`      |
| `0x82` | `130`   | `FILTER_NOT_EQ`      |
| `0x83` | `131`   | `FILTER_NOT_DEV_ABS` |
| `0x84` | `132`   | `FILTER_NOT_DEV_REL` |
| `0x85` | `133`   | `FILTER_NOT_DEV_STD` |
| `0x86` | `134`   | `FILTER_NOT_TOP`     |
| `0x87` | `135`   | `FILTER_NOT_BOTTOM`  |

!!! tip Negation
    Negative filtering functions constants always equate to the value of their positive counterpart plus `128`.

## Reducing functions

| Byte   | Decimal | Constant               |
|:-------|:--------|:-----------------------|
| `0x00` | `0`     | `REDUCER_MIN`          |
| `0x01` | `1`     | `REDUCER_MAX`          |
| `0x02` | `2`     | `REDUCER_MODE`         |
| `0x03` | `3`     | `REDUCER_AVG_MEAN`     |
| `0x04` | `4`     | `REDUCER_AVG_MEAN_W`   |
| `0x05` | `5`     | `REDUCER_AVG_MEDIAN`   |
| `0x06` | `6`     | `REDUCER_AVG_MEDIAN_W` |
| `0x07` | `7`     | `REDUCER_DEV_STD`      |
| `0x08` | `8`     | `REDUCER_DEV_AVG`      |
| `0x09` | `9`     | `REDUCER_DEV_MED`      |
| `0x0a` | `10`    | `REDUCER_DEV_MAX`      |
