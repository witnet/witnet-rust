# RADON encoding

RADON scripts are encoded using [MessagePack], a very efficient, compact and widely supported data structure encoding.

Before encoding, a RADON script looks like this:
```ts
[
    STRING_PARSEJSON,
    MIXED_TOMAP,
    [ MAP_GET, "weather" ],
    MIXED_TOMAP,
    [ MAP_GET, "temp" ],
    MIXED_TOFLOAT
]
```

After encoding, we get an impressively compact (22 bytes long) output:
```
// Base64
lgMEkgCnd2VhdGhlcgSSAKR0ZW1wAg
```

!!! info "Constants"
    All across this documentation, unquoted uppercase names like `STRING_PARSEJSON` identify different operators and
    constants that equate to a single byte when encoded.

    A list of constants can be found in the [Constants section][constants].

[constants]: ../constants
[MessagePack]: https://msgpack.org
