# `String` type

## `String.hash(function)`
```ts
hash(function: String): String
```
```ts
[ STRING_HASH, function ]
```
Applies a hash function on the input `String` and returns its digest as an hexadecimal string.

The available hash functions are listed in the [Predefined functions section][hash].

## `String.length()`
```ts
length(): Int
```
```ts
STRING_LENGTH
```
The `length` operator returns the number of `UTF-8` code units in the input `String`. 

## `String.match(categories, default)`
```ts
match<T>(categories: Map<String, T>, default?: T): T
```
```ts
[ STRING_CATEGORIZE, [ /** `[key, value]` pairs **/ ], default]
```
The `match` operator maps the input `String` into different `T` values as defined in a `Map<String, T>` by
checking if it matches against any of its `String` keys. That is, it classifies the input `String` value into
separate *compartments* or *buckets*.

If the input `String` value is found as a key of `categories: Map<String, T>`, it returns the `T` value associated
to such key. It returns the `default: T` value otherwise.

!!! example
    ```ts
    [ STRING_CATEGORIZE, [ [ "rainy", 0 ], [ "stormy", 0 ], [ "sunny", 1 ] ], 2 ]
    ```

!!! danger ""
    This operator will throw a runtime exception if no `default` value is provided and the input `String` value
    is not found as a key of `categories: Map<String, T>`.
    Exceptions are handled as specified in the [Exception handling] section. 


## `String.parseJSON()`
```ts
parseJSON(): Mixed
```
```ts
STRING_PARSEJSON
```
Parses the input `String` into a `Map<String, Mixed>` assuming it is a correctly formed JSON document.

!!! danger ""
    This operator can throw a runtime exception if:

    - The input `String` is not a well-formed JSON document.
    - The type of some value in the document cannot be inferred.
    
    Exceptions are handled as specified in the [Exception handling] section. 

## `String.parseXML()`
```ts
parseXML(): Map<String, Mixed>
```
```ts
STRING_PARSEXML
```
Parses the input `String` into a `Map<String, Mixed>` assuming it is a correctly formed XML document.

!!! danger ""
    This operator can throw a runtime exception if:
    
    - The input `String` is not a well-formed XML document.
    - The type of some value in the document cannot be inferred.
    Exceptions are handled as specified in the [Exception handling] section.

## `String.toBoolean()`
```ts
toBoolean(): Boolean
```
```ts
STRING_TOBOOLEAN
```
The `toBoolean` operator parses the input `String` as a `Boolean` value. That is, it returns `True` as `Boolean` if the
input `String` is `True`; or `False` as `Boolean` if the input `String` is `False`.

!!! danger ""
    This operator will throw a runtime exception if the input `String` is not a valid `Boolean` value.
    Exceptions are handled as specified in the [Exception handling] section. 

## `String.toFloat()`
```ts
toFloat(): Float
```
```ts
STRING_TOFLOAT
```
The `toFloat` operator parses the input `String` as a floating point number.

!!! danger ""
    This operator will throw a runtime exception if:
    
    - The input `String` is not a valid `Float` value for the specified base.
    - The value overflows or underflows the range of the `Float` type.
    Exceptions are handled as specified in the [Exception handling] section. 

## `String.toInt()`
```ts
toInt(base?: Int): Int
```
```ts
[ STRING_TOINT, base ]
```
The `toInt` operator parses the input `String` as an integer of the specified base.

The accepted bases are:

| Base | Name        | Example            |
|------|-------------|--------------------|
| `2`  | Binary      | `1011111011101111` |
| `8`  | Octal       | `137357`           |
| `10` | Decimal     | `48879`            |
| `16` | Hexadecimal | `BEEF`             |
| `32` | Base32      | `X3XQ`             |
| `64` | Base64      | `vu8`              |

!!! tip ""
    If no base is specified, the default base will be `10` (decimal).

!!! danger ""
    This operator will throw a runtime exception if:
    
    - The input `String` is not a valid `Int` value for the specified base.
    - The value overflows or underflows the range of the `Int` type.

    Exceptions are handled as specified in the [Exception handling] section. 

## `String.toLowerCase()`
```ts
toLowerCase(): String
```
```ts
STRING_TOLOWERCASE
```
Returns the input `String` value converted to uppercase.

## `String.toUpperCase()`
```ts
toUpperCase(): String
```
```ts
STRING_TOUPPERCASE
```
Returns the input `String` value converted to lowercase.

[hash]: ../../functions#hash-functions