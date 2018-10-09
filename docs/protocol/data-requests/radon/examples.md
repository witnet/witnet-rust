# Examples

## Retrieval phase
```ts
[ // String
    STRING_PARSEJSON, // Mixed
    MIXED_TOMAP, // Map<String, Mixed>
    [ MAP_GET, "main" ], // Mixed
    MIXED_TOMAP, // Map<String, Mixed>
    [ MAP_GET, "temp" ], // Mixed
    MIXED_TOFLOAT // Float
] // Result<Float>
```
This example retrieval script does the following on the result of
[this OpenWeatherMap API call][openweathermap]:

1. Parse the input `String` as a JSON document (retrieval always starts with `String`),
2. Treat the structure as a `Map<String, Mixed>`,
3. Take the value of the `"main"` key,
4. Treat the structure as a `Map<String, Mixed>`.
5. Take the value of the `"temp"` key,
6. Emit the value as a `Float`.

## Aggregation phase
```ts
[ // Array<Result<Float>>
    ARRAY_FLATMAP, // Array<Float>
    [ ARRAY_FILTER, FILTER_GT, -30 ], // Array<Float>
    [ ARRAY_FILTER, FILTER_LT, 50 ], // Array<Float>
    [ ARRAY_FILTER, FILTER_DEV_ABS, 2 ], // Array<Float>
    [ ARRAY_REDUCE, REDUCER_AVG_MEAN ] // Float
] // Result<Float>
```
This example aggregation script does the following:

1. Drop every negative `Result` (`Err` items) from the input `Array`,
2. Drop values less or equal than `-30`,
3. Drop values greater or equal than `50`,
4. Drop values deviating from the average more than `2`,
5. Calculate and emit the arithmetic mean of the remaining values in the `Array`.

## Consensus phase
```ts
[ // Array<Result<Float>>
    ARRAY_FLATMAP,
    [ ARRAY_REDUCE, REDUCE_AVG_MEAN ] // Float
] // Result<Float>
```
This example consensus script does the following:

1. Drop every negative `Result` (`Err` items) from the input `Array`,
2. Calculate and emit the arithmetic mean of the remaining values in the `Array`.

[openweathermap]: https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22