# `Boolean` type

The `Boolean` data type can only take one of two possible values: `true` or `false`.

## `Boolean.match(categories, default)`
```ts
match<V>(categories: Map<Boolean, V>, default: V): V
```
```ts
[ BOOLEAN_MATCH, [ /** `[key, value]` pairs  **/ ] ]
```
The `match` operator maps the input `Boolean` into different `V` values as defined in a `Map<Boolean, V>` by checking
whether it matches against any of its `Boolean` keys. That is, it classifies the input `Boolean` value into
separate *compartments* or *buckets*.

If the input `Boolean` value is found as a key of `categories: Map<Boolean, V>`, it returns the `V` value associated
to such key. It returns the `default: V` value otherwise.

The `V` type must be one of the value types: `Boolean`, `Int`, `Float` or `String`.

!!! example
    ```ts
    [ BOOLEAN_MATCH, [ [ True, "Valid" ], [ False, "Invalid" ] ] ]
    ```

!!! danger ""
    This operator will throw a runtime exception if no `default` value is provided and the input `Boolean` value
    is not found as a key of `categories: Map<Boolean, V>`.
    Exceptions are handled as specified in the [Exception handling] section. 

## `Boolean.neg()`
```ts
neg(): Boolean
```
```ts
BOOLEAN_NEG
```
The `neg` operator returns the negation of the input `Boolean` value. That is, it returns `True` as `Boolean` only if
the input `Boolean` is `False`. It returns `False` as `Boolean` otherwise.

## `Boolean.toString()`
```ts
toString(): String
```
```ts
BOOLEAN_TOSTRING
```
The `toString` operator returns a `String` representing the input `Boolean` value. That is, it returns `True` as
`String` only if the input `Boolean` is `True`. It returns `False` as `String` otherwise.
