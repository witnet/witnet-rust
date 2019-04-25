# `Boolean` type

The `Boolean` data type can only take one of two possible values: `true`
or `false`.

## `Boolean.match(categories, default)`
```ts
match<V>(categories: Map<Boolean, V>, default: V): V
```
```ts
[ OP_BOOLEAN_MATCH, [ /** `[key, value]` pairs  **/ ], default ]
```
The `match` operator maps the input `Boolean` into different `V` values
as defined in a `Map<Boolean, V>` by checking whether it matches against
any of its `Boolean` keys. That is, it classifies the input `Boolean`
value into separate *compartments* or *buckets*.

If the input `Boolean` value is found as a key of `categories:
Map<Boolean, V>`, it returns the `V` value associated to such key. It
returns the `default: V` value otherwise.

The `V` type must be one of the value types: `Boolean`, `Int`, `Float`
or `String`.

!!! example
    ```ts
    93 10 92 92 C3 A5 56 61 6C 69 64 92 C2 A7 49 6E 76 61 6C 69 64 C2
    ```
    ```ts
    [ OP_BOOLEAN_MATCH, [       // [ 0x10, [
        [ true, "Valid" ],      //      [ true, "Valid" ],
        [ false, "Invalid" ]    //      [ false, "Invalid" ]
    ], false ]                  // ], false ]
    ```

!!! danger ""
    This operator will throw a runtime exception if no `default` value 
    is provided and the input `Boolean` value is not found as a key of 
    `categories: Map<Boolean, V>`. Exceptions are handled as specified 
    in the [Exception handling] section. 

## `Boolean.negate()`
```ts
neg(): Boolean
```
```ts
OP_BOOLEAN_NEGATE
```
The `negate` operator returns the negation of the input `Boolean` value.
That is, it returns `true` as `Boolean` only if the input `Boolean` is
`false`. It returns `false` as `Boolean` otherwise.

## `Boolean.toString()`
```ts
toString(): String
```
```ts
OP_BOOLEAN_TOSTRING
```
The `toString` operator returns a `String` representing the input
`Boolean` value. That is, it returns `true` as `String` only if the
input `Boolean` is `true`. It returns `false` as `String` otherwise.
