# `Mixed` type
The `Mixed` type represents a value or structure whose type is undecided
and cannot be automatically inferred by the interpreter.

The operators available for this type assist the interpreter to handle
`Mixed` values and structures in a deterministic way so that it can be
safely casted to other, more useful types.

## `Mixed.toArray()`
```ts
toArray(): Array<Mixed>
```
```ts
[ OP_MIXED_TOARRAY, type ]
```
The `toArray` operator tries to cast the input `Mixed` to an 
`Array<Mixed>` structure.

!!! danger ""
    This operator will throw a runtime exception if the input `Mixed` 
    cannot be casted to a valid `Array<Mixed>` value. Exceptions are 
    handled as specified in the [Exception handling] section.

## `Mixed.toBoolean()`
```ts
toBoolean(): Boolean
```
```ts
OP_MIXED_TOBOOLEAN
```
The `toBoolean` operator tries to cast the input `Mixed` to a `Boolean`
value. That is, it returns `true` if the input is `true` as either
`Boolean` or `String`; or `false` as `Boolean` if the input `Mixed` is
`false` as either `Boolean` or `String`.

!!! danger ""
    This operator will throw a runtime exception if the input `Mixed` 
    cannot be casted to a valid `Boolean` value. Exceptions are handled 
    as specified in the [Exception handling] section.

## `Mixed.toFloat()`
```ts
toFloat(): Float
```
```ts
OP_MIXED_TOFLOAT
```
The `toFloat` operator tries to cast the input `Mixed` to a `Float`
value.

!!! danger ""
    This operator will throw a runtime exception if the input `Mixed` 
    cannot be casted to a valid `Float` value for the specified base or 
    if the value overflows or underflows the range of the `Float` type.
    Exceptions are handled as specified in the [Exception handling] 
    section.

## `Mixed.toInteger()`
```ts
toInteger(base?: Integer): Integer
```
```ts
[ OP_MIXED_TOINTEGER, base ]
```
The `toInteger` operator parses the input `Mixed` as an integer of the
specified base.

The accepted bases are the same as in
[`String::toInteger(base)`][StringToInteger].

!!! tip ""
    If no base is specified, the default base will be `10` (decimal).

!!! danger ""
    This operator will throw a runtime exception if:
    
    - The input `Mixed` cannot be casted to a valid `Integer` value for 
    the specified base.
    - The value overflows or underflows the range of the `Integer` type.

    Exceptions are handled as specified in the [Exception handling] 
    section.

## `Mixed.toMap()`
```ts
toMap(): Map<String, Mixed>
```
```ts
OP_MIXED_TOMAP
```
The `toMap` operator tries to cast the input `Mixed` to a 
`Map<String, Mixed>` structure.

!!! danger ""
    This operator will throw a runtime exception if the input `Mixed` 
    cannot be casted to a valid `Map<String, Mixed>` value. Exceptions 
    are handled as specified in the [Exception handling] section.
