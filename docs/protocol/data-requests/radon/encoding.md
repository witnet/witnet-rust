# RADON encoding

RADON scripts are encoded using [MessagePack], a very efficient, compact and widely supported data structure encoding.

Look for example at this impressively short (22 bytes) serialized RADON script:
```ts
// As bytes
96 43 74 92 61 A7 77 65 61 74 68 65 72 74 92 61 A4 74 65 6D 70 72

// As Base64 string
"lkN0kmGnd2VhdGhlcnSSYaR0ZW1wcg=="

```

Once decoded, the resulting structure will actually represent this RADON script:
```ts
[
    STRING_PARSEJSON,       // 0x43
    MIXED_TOMAP,            // 0x74
    [ MAP_GET, "weather" ], // [ 0x61, "weather" ]
    MIXED_TOMAP,            // 0x74
    [ MAP_GET, "temp" ],    // [ 0x61, "temp" ]
    MIXED_TOFLOAT           // 0x72
]
```

!!! tip
    RADON scripts are pure byte code sequences but at the same time represent high-level abstractions.
    In an hypothetical Javascript-like representation of RADON operators, the script above may resemble:
    
    ```ts
    retrieve(url)
        .parseJSON()
        .toMap()
        .get("weather")
        .toMap()
        .get("temp")
        .toFloat()
    ```

!!! info "Constants"
    All across this documentation, unquoted uppercase names like `STRING_PARSEJSON` identify different operators and
    constants that equate to a single byte when encoded.

    A list of constants can be found in the [Constants section][constants].

[constants]: ../constants
[MessagePack]: https://msgpack.org
