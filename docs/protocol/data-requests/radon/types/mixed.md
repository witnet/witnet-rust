# `Mixed` type
The `Mixed` type represents a value or structure whose type is undecided and cannot be automatically inferred by the
interpreter.

The operators available for this type assist the interpreter to handle `Mixed` values and structures in a deterministic
way so that it can be safely casted to other, more useful types.

## `Mixed.toArray(type)`
```ts
toArray<T>(type?: String): Array<T>
```
```ts
[ MIXED_TOARRAY, type ]
```
The `toArray` operator tries to cast the input `Mixed` to an `Array<T>` structure, where `T` is any of the RADON types,
supplied by name as the `type: String` argument.

!!! info ""
    If no `type: String` is supplied, the output type will be `Array<Mixed>` by default.

!!! danger ""
    This operator will throw a runtime exception if the input `Mixed` cannot be casted to a valid `Boolean` value.
    Exceptions are handled as specified in the [Exception handling] section.

## `Mixed.toBoolean()`
```ts
toBoolean(): Boolean
```
```ts
MIXED_TOBOOLEAN
```
The `toBoolean` operator tries to cast the input `Mixed` to a `Boolean` value. That is, it returns `True` if the input
is `True` as either `Boolean` or `String`; or `False` as `Boolean` if the input `Mixed` is `False` as either `Boolean`
or `String`.

!!! danger ""
    This operator will throw a runtime exception if the input `Mixed` cannot be casted to a valid `Boolean` value.
    Exceptions are handled as specified in the [Exception handling] section.

## `Mixed.toFloat()`
```ts
toFloat(): Float
```
```ts
MIXED_TOFLOAT
```
The `toFloat` operator tries to cast the input `Mixed` to a `Float` value.

!!! danger ""
    This operator will throw a runtime exception if the input `Mixed` cannot be casted to a valid `Float` value for the
    specified base or if the value overflows or underflows the range of the `Float` type.
    Exceptions are handled as specified in the [Exception handling] section.

## `Mixed.toInt()`
```ts
toInt(base?: Int): Int
```
```ts
[ MIXED_TOINT, base ]
```
The `toInt` operator parses the input `Mixed` as an integer of the specified base.

The accepted bases are the same as in [`String::toInt(base)`][StringToInt].

!!! tip ""
    If no base is specified, the default base will be `10` (decimal).

!!! danger ""
    This operator will throw a runtime exception if:
    
    - The input `Mixed` cannot be casted to a valid `Int` value for the specified base
    - The value overflows or underflows the range of the `Int` type.

    Exceptions are handled as specified in the [Exception handling] section.

## `Mixed.toMap(keyType, valueType)`
```ts
toMap<K, T>(keyType?: String, valueType?: String): Map<K, T>
```
```ts
[ MIXED_TOMAP, keyType, valueType ]
```
The `toArray` operator tries to cast the input `Mixed` to a `Map<K, T>` structure, where `K` is only one of the RADON
*[value types]*, supplied by name as the `keyType: String` argument; and `T` is any of the RADON types, supplied by
name as the `valueType: String` argument.

!!! info ""
    - If no `keyType: String` is supplied, it will be assumed to be `String` by default.
    - If no `valueType: String` is supplied, it will be assumed to be `Mixed` by default.

!!! danger ""
    This operator will throw a runtime exception if the input `Mixed` cannot be casted to a valid `Map<K, T>` value.
    Exceptions are handled as specified in the [Exception handling] section.
