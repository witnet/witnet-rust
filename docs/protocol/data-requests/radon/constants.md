# Constants

## Types

| Byte   | Decimal | Constant       |
|--------|---------|----------------|
| `0x00` | `0`     | `TYPE_BOOLEAN` |
| `0x01` | `1`     | `TYPE_INT`     |
| `0x02` | `2`     | `TYPE_FLOAT`   |
| `0x03` | `3`     | `TYPE_STRING`  |
| `0x04` | `4`     | `TYPE_ARRAY`   |
| `0x05` | `5`     | `TYPE_MAP`     |
| `0x06` | `6`     | `TYPE_MIXED`   |
| `0x07` | `7`     | `TYPE_NULL`    |
| `0x08` | `8`     | `TYPE_RESULT`  |

## Operators

### `Boolean` operators

| Byte   | Decimal | Constant           |
|--------|---------|--------------------|
| `0x00` | `0`     | `BOOLEAN_MATCH`    |
| `0x01` | `1`     | `BOOLEAN_NEG`      |
| `0x02` | `2`     | `BOOLEAN_TOSTRING` |

### `Int` operators

| Byte   | Decimal | Constant         |
|--------|---------|------------------|
| `0x00` | `0`     | `INT_ABS`        |
| `0x01` | `1`     | `INT_MATCH`      |
| `0x02` | `2`     | `INT_MODULO`     |
| `0x03` | `3`     | `INT_MULT`       |
| `0x04` | `4`     | `INT_NEG`        |
| `0x05` | `5`     | `INT_POW`        |
| `0x06` | `6`     | `INT_RECIP`      |
| `0x07` | `7`     | `INT_SUM`        |
| `0x08` | `8`     | `INT_TOFLOAT`    |
| `0x09` | `9`     | `INT_TOSTRING`   |

### `Float` operators

| Byte   | Decimal | Constant         |
|--------|---------|------------------|
| `0x00` | `0`     | `FLOAT_ABS`      |
| `0x01` | `1`     | `FLOAT_CEIL`     |
| `0x02` | `2`     | `FLOAT_FLOOR`    |
| `0x03` | `3`     | `FLOAT_MODULO`   |
| `0x04` | `4`     | `FLOAT_MULT`     |
| `0x05` | `5`     | `FLOAT_NEG`      |
| `0x06` | `6`     | `FLOAT_POW`      |
| `0x07` | `7`     | `FLOAT_RECIP`    |
| `0x08` | `8`     | `FLOAT_ROUND`    |
| `0x09` | `9`     | `FLOAT_SUM`      |
| `0x0a` | `10`    | `FLOAT_TOSTRING` |
| `0x0b` | `11`    | `FLOAT_TRUNC`    |

### `String` operators

| Byte   | Decimal | Constant             |
|--------|---------|----------------------|
| `0x00` | `0`     | `STRING_HASH`        |
| `0x01` | `1`     | `STRING_LENGTH`      |
| `0x02` | `2`     | `STRING_MATCH`       |
| `0x03` | `3`     | `STRING_PARSEJSON`   |
| `0x04` | `4`     | `STRING_PARSEXML`    |
| `0x05` | `5`     | `STRING_TOBOOLEAN`   |
| `0x06` | `6`     | `STRING_TOFLOAT`     |
| `0x07` | `7`     | `STRING_TOINT`       |
| `0x08` | `8`     | `STRING_TOLOWERCASE` |
| `0x09` | `9`     | `STRING_TOUPPERCASE` |

### `Array` operators

| Byte   | Decimal | Constant        |
|--------|---------|-----------------|
| `0x00` | `0`     | `ARRAY_COUNT`   |
| `0x01` | `1`     | `ARRAY_EVERY`   |
| `0x02` | `2`     | `ARRAY_FILTER`  |
| `0x03` | `3`     | `ARRAY_FLATTEN` |
| `0x04` | `4`     | `ARRAY_GET`     |
| `0x05` | `5`     | `ARRAY_MAP`     |
| `0x06` | `6`     | `ARRAY_REDUCE`  |
| `0x07` | `7`     | `ARRAY_SOME`    |
| `0x08` | `8`     | `ARRAY_SORT`    |
| `0x09` | `9`     | `ARRAY_TAKE`    |

### `Map` operators

| Byte   | Decimal | Constant      |
|--------|---------|---------------|
| `0x00` | `0`     | `MAP_ENTRIES` |
| `0x00` | `0`     | `MAP_GET`     |
| `0x00` | `0`     | `MAP_KEYS`    |
| `0x00` | `0`     | `MAP_VALUES`  |

### `Mixed` operators

| Byte   | Decimal | Constant          |
|--------|---------|-------------------|
| `0x00` | `0`     | `MIXED_TOARRAY`   |
| `0x01` | `1`     | `MIXED_TOBOOLEAN` |
| `0x02` | `2`     | `MIXED_TOFLOAT`   |
| `0x03` | `3`     | `MIXED_TOINT`     |
| `0x04` | `4`     | `MIXED_TOMAP`     |

### `Result` operators

| Byte   | Decimal | Constant       |
|--------|---------|----------------|
| `0x00` | `0`     | `RESULT_GET`   |
| `0x01` | `1`     | `RESULT_GETOR` |
| `0x02` | `2`     | `RESULT_ISOK`  |

## Hash functions

| Byte   | Decimal | Constant        |
|--------|---------|-----------------|
| `0x00` | `0`     | `BLAKE_256`     |
| `0x01` | `1`     | `BLAKE_512`     |
| `0x02` | `2`     | `BLAKE2S_256`   |
| `0x03` | `3`     | `BLAKE2B_512`   |
| `0x04` | `4`     | `MD5_128`       |
| `0x05` | `5`     | `RIPEMD_128`    |
| `0x06` | `6`     | `RIPEMD_160`    |
| `0x07` | `7`     | `RIPEMD_320`    |
| `0x08` | `8`     | `SHA1_160`      |
| `0x09` | `9`     | `SHA2_224`      |
| `0x0a` | `10`    | `SHA2_256`      |
| `0x0b` | `11`    | `SHA2_384`      |
| `0x0c` | `12`    | `SHA2_512`      |
| `0x0d` | `13`    | `SHA3_224`      |
| `0x0e` | `14`    | `SHA3_256`      |
| `0x0f` | `15`    | `SHA3_384`      |
| `0x10` | `16`    | `SHA3_512`      |
| `0x11` | `17`    | `WHIRLPOOL_512` |

## Filtering functions

| Byte   | Decimal | Constant             |
|--------|---------|----------------------|
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
|--------|---------|------------------------|
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