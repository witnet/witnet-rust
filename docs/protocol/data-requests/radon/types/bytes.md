# `Bytes` type
The `Bytes` type represents a value or structure whose type is undecided
and cannot be automatically inferred by the interpreter.

The operators available for this type assist the interpreter to handle
`Bytes` values and structures in a deterministic way so that it can be
safely casted to other, more useful types.

## `Bytes.toArray()`
```ts
toArray(): Array<Bytes>
```
```ts
[ OP_BYTES_TOARRAY, type ]
```
The `toArray` operator tries to cast the input `Bytes` to an 
`Array<Bytes>` structure.

!!! danger ""
    This operator will throw a runtime exception if the input `Bytes` 
    cannot be casted to a valid `Array<Bytes>` value. Exceptions are 
    handled as specified in the [Exception handling] section.

## `Bytes.toBoolean()`
```ts
toBoolean(): Boolean
```
```ts
OP_BYTES_TOBOOLEAN
```
The `toBoolean` operator tries to cast the input `Bytes` to a `Boolean`
value. That is, it returns `true` if the input is `true` as either
`Boolean` or `String`; or `false` as `Boolean` if the input `Bytes` is
`false` as either `Boolean` or `String`.

!!! danger ""
    This operator will throw a runtime exception if the input `Bytes` 
    cannot be casted to a valid `Boolean` value. Exceptions are handled 
    as specified in the [Exception handling] section.

## `Bytes.toFloat()`
```ts
toFloat(): Float
```
```ts
OP_BYTES_TOFLOAT
```
The `toFloat` operator tries to cast the input `Bytes` to a `Float`
value.

!!! danger ""
    This operator will throw a runtime exception if the input `Bytes` 
    cannot be casted to a valid `Float` value for the specified base or 
    if the value overflows or underflows the range of the `Float` type.
    Exceptions are handled as specified in the [Exception handling] 
    section.

## `Bytes.toInteger()`
```ts
toInteger(base?: Integer): Integer
```
```ts
[ OP_BYTES_TOINTEGER, base ]
```
The `toInteger` operator parses the input `Bytes` as an integer of the
specified base.

The accepted bases are the same as in
[`String::toInteger(base)`][StringToInteger].

!!! tip ""
    If no base is specified, the default base will be `10` (decimal).

!!! danger ""
    This operator will throw a runtime exception if:
    
    - The input `Bytes` cannot be casted to a valid `Integer` value for 
    the specified base.
    - The value overflows or underflows the range of the `Integer` type.

    Exceptions are handled as specified in the [Exception handling] 
    section.

## `Bytes.toMap()`
```ts
toMap(): Map<String, Bytes>
```
```ts
OP_BYTES_TOMAP
```
The `toMap` operator tries to cast the input `Bytes` to a 
`Map<String, Bytes>` structure.

!!! danger ""
    This operator will throw a runtime exception if the input `Bytes` 
    cannot be casted to a valid `Map<String, Bytes>` value. Exceptions 
    are handled as specified in the [Exception handling] section.
