# Examples

## What's the weather in Berlin?

The following retrieval, aggregation and tally scripts operate on the
result of [this query to the OpenWeatherMap API ][openweathermap] that
returns the current weather conditions in Berlin.

### Retrieval stage

```ts
96 43 74 92 61 A7 77 65 61 74 68 65 72 74 92 61 A4 74 65 6D 70 72
```
```ts
[
    STRING_PARSEJSON,       // 0x43
    BYTES_TOMAP,            // 0x74
    [ MAP_GET, "weather" ], // [ 0x61, "weather" ]
    BYTES_TOMAP,            // 0x74
    [ MAP_GET, "temp" ],    // [ 0x61, "temp" ]
    BYTES_TOFLOAT           // 0x72
]
```

1. Parse the input `String` as a JSON document (retrieval always starts
   with `String`),
2. Treat the structure as `Map<String, Bytes>`,
3. Take the value of the `"main"` key as `Bytes`,
4. Treat the structure as `Map<String, Bytes>`.
5. Take the value of the `"temp"` key as `Bytes`,
6. Emit the value as `Float`.

### Aggregation stage

```ts
95 53 93 52 00 E2 93 52 01 32 93 52 03 02 92 56 03
```
```ts
[
    ARRAY_FLATTEN,                          // 0x53,
    [ ARRAY_FILTER, FILTER_GT, -30 ],       // [ 0x52, 0x00, -30 ], 
    [ ARRAY_FILTER, FILTER_LT, 50 ],        // [ 0x52, 0x01, 50 ],
    [ ARRAY_FILTER, FILTER_DEV_ABS, 2 ],    // [ 0x52, 0x03, 2 ], 
    [ ARRAY_REDUCE, REDUCER_AVG_MEAN ]      // [ 0x56, 0x03 ] 
]
```

1. Drop every negative `Result` (`Err` items) from the input `Array`,
2. Drop values less or equal than `-30`,
3. Drop values greater or equal than `50`,
4. Drop values deviating from the average more than `2`,
5. Calculate and emit the arithmetic mean of the remaining values in the
   `Array`.

### Tally stage

The following tally script is quite generic but should work for most
cases in which we are trying to build consensus on `Integer` or `Float`
data points.

```ts
93 53 93 52 05 02 92 56 03
```
```ts
[ 
    ARRAY_FLATTEN,                      // 0x53,
    [ ARRAY_FILTERFILTER_DEV_STD, 2 ],  // [ 0x52, 0x05, 2 ], 
    [ ARRAY_REDUCE, REDUCER_AVG_MEAN ]  // [ 0x56, 0x03 ] 
]
```

1. Drop every negative `Result` (`Err` items) from the input `Array`,
2. Drop values deviating from the average more than twice the standard
   deviation of the remaining values in the `Array`,
3. Calculate and emit the arithmetic mean of the remaining values in the
   `Array`.

## What's the USD price of a bitcoin?

The following retrieval, aggregation and tally scripts operate on the
result of [this query to the Coinbase price API][coinbase] that returns
the current price of a bitcoin in US dollars.

### Retrieval stage
```ts
98 43 74 92 61 A3 62 70 69 74 92 61 A3 55 53 44 74 92 61 AA 72 61 74 65
5F 66 6C 6F 61 74 72
```
```ts
[
  OP_STRING_PARSEJSON,          // 0x43,
  OP_BYTES_TOMAP,               // 0x74.
  [ OP_MAP_GET , "bpi" ],       // [ 0x61, "bpi" ].
  OP_BYTES_TOMAP,               // 0x74.
  [ OP_MAP_GET, "USD" ],        // [ 0x61, "USD" ].
  OP_BYTES_TOMAP,               // 0x74.
  [ OP_MAP_GET, "rate_float" ], // [ 0x61, "rate_float" ].
  OP_BYTES_TOFLOAT              // 0x72
]
```

2. Treat the structure as `Map<String, Bytes>`,
3. Take the value of the `"bpi"` key as `Bytes`,
4. Treat the structure as `Map<String, Bytes>`.
5. Take the value of the `"USD"` key as `Bytes`,
6. Treat the structure as `Map<String, Bytes>`.
7. Take the value of the `"rate_float"` key as `Bytes`,
8. Emit the value as `Float`.

### Aggregation stage

The following tally script is quite generic but should work for most
cases in which we are trying to build consensus on `Integer` or `Float`
data points.

```ts
93 53 93 52 05 02 92 56 03
```
```ts
[ 
    ARRAY_FLATTEN,                      // 0x53,
    [ ARRAY_FILTERFILTER_DEV_STD, 2 ],  // [ 0x52, 0x05, 2 ], 
    [ ARRAY_REDUCE, REDUCER_AVG_MEAN ]  // [ 0x56, 0x03 ] 
]
```

1. Drop every negative `Result` (`Err` items) from the input `Array`,
2. Drop values deviating from the average more than twice the standard
   deviation of the remaining values in the `Array`,
3. Calculate and emit the arithmetic mean of the remaining values in the
   `Array`.
   
### Tally stage

For the tally stage we can safely use the same generic script as for the
aggregation stage.

## Heads or tails?

The following retrieval, aggregation and tally scripts operate on the
result of
[this query to the Australian National University Quantum Random Numbers Server][random]
that returns true random numbers in the `[0, 255]` range generated in
real-time by measuring the quantum fluctuations of the vacuum in a
laboratory.

The tally stage computes the average of the values reported by multiple
witness nodes, which will produce a point in the `[0, 255]` range that
is normally distributed around the half-range, i.e. it will fall in any
of the `[0, 127]` or `[128, 255]` sub-ranges with a 50% probability.

Finally, it checks in which side of the half-range did the point
actually fall and maps that into a `String` with value `heads` or
`tails`.

### Retrieval stage
```ts
95 43 74 92 61 A4 64 61 74 61 70 92 54 00
```
```ts
[
  OP_STRING_PARSEJSON,      // 0x43,
  OP_BYTES_TOMAP,           // 0x74,
  [ OP_MAP_GET, "data" ],   // [ 0x61, "data" ],
  OP_BYTES_TOARRAY,         // 0x70,
  [ OP_ARRAY_GET, 0 ]       // [ 0x54, 0 ]
]
```

1. Parse the input `String` as a JSON document (retrieval always starts
   with `String`),
2. Treat the structure as `Map<String, Bytes>`,
3. Take the value of the `"data"` key as `Bytes`,
4. Treat the structure as `Array<Bytes>`.
5. Take the value at index `0` as `Bytes`,
6. Emit the value as `Float`.

### Aggregation stage
```ts
96 53 92 52 92 CC 81 00 92 52 92 CC 81 00 92 56 03 92 32 7F 92 10 92 92
C2 A5 68 65 61 64 73 92 C3 A5 74 61 69 6C 73
```
```ts
[
    OP_ARRAY_FLATTEN,                               // 0x53,
    [ OP_ARRAY_FILTER, [ FILTER_NOT_LT, 0 ] ],      // [ 0x52, [ 0x81, 0 ] ],
    [ OP_ARRAY_FILTER, [ FILTER_NOT_GT, 255 ] ],    // [ 0x52, [ 0x81, 0 ] ],
    [ OP_ARRAY_REDUCE, REDUCER_AVERAGE_MEAN ],      // [ 0x56, 0x03 ],
    [ OP_FLOAT_GREATER, 127 ],                      // [ 0x32, 127 ],
    [ OP_BOOLEAN_MATCH, [                           // [ 0x10, [
        [ false, "heads" ],                         //     [ false, "heads" ],
        [ true, "tails" ]                           //     [ true, "tails" ]
    ] ]                                             // ]
]
```

1. Drop every negative `Result` (`Err` items) from the input `Array`,
2. Remove any items with value under `0` from the remaining `Array`,
3. Remove any items with value over `255` from the remaining `Array`,
4. Calculate the arithmetic mean of the remaining `Array`,
5. Check if the resulting `Float` is greater than `127`, and continue
   with a `Boolean` of value `true` or `false` accordingly,
6. Map the `Boolean` to `String` by converting `false` into `"heads"`
   and `true` into `"tails"`.



[openweathermap]: https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22
[coinbase]: https://api.coindesk.com/v1/bpi/currentprice.json
[random]: http://qrng.anu.edu.au/API/jsonI.php?length=1&type=uint8