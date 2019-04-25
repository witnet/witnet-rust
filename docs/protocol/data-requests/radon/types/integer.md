# `Integer` type

## `Integer.absolute()`
```ts
absolute(): Integer
```
```ts
OP_INTEGER_ABSOLUTE
```
The `absolute` operator returns the absolute value of the input
`Integer` number. That is, its distance from zero, without regard of its
sign.

## `Integer.match(categories, default)`
```ts
match<V>(categories: Map<Integer, V>, default?: V): V
```
```ts
[ OP_INTEGER_MATCH, [ /** `[key, value]` pairs **/ ] , default]
```
The `match` operator maps the input `Integer` into different `V` values
as defined in a `Map<Integer, V>` by checking if it matches against any
of its `Integer` keys. That is, it classifies the input `Integer` value
into separate *compartments* or *buckets*.

If the input `Integer` value is found as a key of `categories:
Map<Integer, V>`, it returns the `V` value associated to such key. It
returns the `default: V` value otherwise.

!!! example
    ```ts
    91 93 21 93 92 01 A3 4F 6E 65 92 02 A3 54 77 6F 92 03 A5 54 68 72 65 65 A5 4F 74 68 65 72
    ```
    ```ts
    [ [ OP_INTEGER_MATCH, [     // [ [ 0x21, [
        [ 1, "One" ],           //      [ 1, "One" ],
        [ 2, "Two" ],           //      [ 2, "Two" ],
        [ 3, "Three" ]          //      [ 3, "Three" ]
    ], "Other" ] ]              // ], "Other" ] ] 
    ```

!!! danger ""
    This operator will throw a runtime exception if no `default` value 
    is provided and the input `Integer` value is not found as a key of 
    `categories: Map<Integer, V>`. Exceptions are handled as specified 
    in the [Exception handling] section. 

## `Integer.modulo(modulus)`
```ts
modulo(modulus: Integer): Integer
```
```ts
[ OP_INTEGER_MODULO, modulus ]
```
The `modulo` operator returns the remainder after the division of the
input `Integer` value by the `modulus: Integer` value supplied as an
argument.

!!! info ""
    The resulting value always takes the same sign as the input 
    `Integer` value.

## `Integer.multiply(factor)`
```ts
multiply(factor: Float): Float
```
```ts
[ OP_INTEGER_MULTIPLY, factor ]
```
The `multiply` operator returns the multiplication of the input 
`Integer` value and the `factor: Integer` value supplied as an argument.

!!! tip "Where is the division operator?"
    Division is not an elementary operator in RADON.
    It is instead achieved by composing using the `multiply` operator 
    with a reciprocal factor, i.e. computing the division by two is the
    same as multiplying by `0.5`.
    
!!! danger ""
    This operator can throw a runtime exception if the resulting value 
    overflows or underflows the range of the `Integer` type. Exceptions 
    are handled as specified in the [Exception handling] section. 

## `Integer.negate()`
```ts
negate(): Integer
```
```ts
OP_INTEGER_NEGATE
```
The `negate` operator returns the additive inverse, opposite, sign 
change or negation of the input `Integer` number. That is, the number 
that, when added to the input number, yields zero.

## `Integer.power(exponent)`
```ts
power(exponent: Float): Float
```
```ts
[ OP_INTEGER_POW, exponent ]
```
The `power` operator returns the value of the input `Integer` as base, 
exponentiated to the `exponent: Float` power.

!!! tip "Where is the *nth*-root operator?"
    The *nth*-root is not an elementary operator in RADON.
    It is instead achieved by composing using the `power` operator with 
    a reciprocal exponent, i.e. computing the square root is the same as 
    exponentiating to the power of `0.5`.

!!! danger ""
    This operator can throw a runtime exception if the resulting value 
    overflows or underflows the range of the `Float` type. Exceptions 
    are handled as specified in the [Exception handling] section.

## `Integer.reciprocal()`
```ts
reciprocal(): Float
```
```ts
OP_INTEGER_RECIPROCAL
```
The `reciprocal` operator returns the multiplicative inverse or
reciprocal of the input `Integer` number. That is, the number which
multiplied by the input number, yields 1.

!!! danger ""
    This operator will throw a runtime exception if the input `Integer` 
    is `0`, given that the reciprocal would be infinity, which is way 
    beyond the bounds of a `Float` number. Exceptions are handled as 
    specified in the [Exception handling] section. 

## `Integer.sum(addend)`
```ts
sum(addend: Integer): Integer
```
```ts
[ OP_INTEGER_SUM, addend ]
```
The `sum` operator returns the sum of the input `Integer` value and the
`addend: Integer` value supplied as an argument.

!!! tip "Where is the difference operator?"
    Difference is not an elementary operator in RADON.
    It is instead achieved by composing the negation (`neg`) and 
    summation (`sum`) operators.

!!! danger ""
    This operator can throw a runtime exception if the resulting value 
    overflows or underflows the range of the `Integer` type. Exceptions 
    are handled as specified in the [Exception handling] section.

## `Integer.toFloat()`
```ts
toFloat(): Float
```
```ts
OP_INTEGER_TOFLOAT
```
The `toFloat` operator returns the value of the input `Integer` as a
floating point number.

## `Integer.toString()`
```ts
toString(base?: Integer): String
```
```ts
[ OP_INTEGER_TOSTRING, base ]
```
The `toString` operator returns a `String` representing the input
`Integer` value using the provided base.

The accepted bases are the same as in
[`String::toInteger(base)`][StringToInteger].

!!! tip ""
    If no base is specified, the default base will be `10` (decimal).
