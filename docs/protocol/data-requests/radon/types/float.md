# `Float` type

## `Float.absolute()`
```ts
absolute(): Float
```
```ts
OP_FLOAT_ABS
```
The `absolute` operator returns the absolute value of the input `Float`
number. That is, its distance from zero, without regard of its sign.

## `Float.ceil()`
```ts
ceiling(): Integer
```
```ts
OP_FLOAT_CEILING
```
The `ceiling` operator returns the smallest `Integer` number greater
than or equal to the input `Float` number.

## `Float.floor()`
```ts
floor(): Integer
```
```ts
OP_FLOAT_FLOOR
```
The `floor` operator returns the largest `Integer` number less than or
equal to the input `Float` number.

## `Float.modulo(modulus)`
```ts
modulo(modulus: Integer): Float
```
```ts
[ OP_FLOAT_MODULO, modulus ]
```
The `modulo` operator returns the remainder after the division of the
input `Float` value by the `modulus: Float` value supplied as an
argument.

!!! info ""
    The resulting value always takes the same sign as the input `Float` 
    value.

## `Float.multiply(factor)`
```ts
multiply(factor: Float): Float
```
```ts
[ OP_FLOAT_MULTIPLY, factor ]
```
The `multiply` operator returns the multiplication of the input `Float`
value and the `factor: Float` value supplied as an argument.

!!! tip "Where is the division operator?"
    Division is not an elementary operator in RADON.
    It is instead achieved by composing using the `multiply` operator 
    with a reciprocal factor, i.e. computing the division by two is the
    same as multiplying by `0.5`.
    
!!! danger ""
    This operator can throw a runtime exception if the resulting value 
    overflows or underflows the range of the `Float` type.
    Exceptions are handled as specified in the [Exception handling] 
    section. 

## `Float.negate()`
```ts
negate(): Float
```
```ts
OP_FLOAT_NEGATE
```
The `negate` operator returns the additive inverse, opposite, sign
change or negation of the input `Float` number. That is, the number
that, when added to the input number, yields zero.

## `Float.power(exponent)`
```ts
power(exponent: Float): Float
```
```ts
[ OP_FLOAT_POWER, exponent ]
```
The `power` operator returns the value of the input `Float` as base,
exponentiated to the `exponent: Float` power.

!!! tip "Where is the *nth*-root operator?"
    The *nth*-root is not an elementary operator in RADON.
    It is instead achieved by composing using the `power` operator with 
    a reciprocal exponent, i.e. computing the square root is the same as 
    exponentiating to the power of `0.5`.
    
    ```ts
    92 36 CB 3F E0 00 00 00 00 00 00
    ```
    ```ts
    [ OP_FLOAT_POWER, 0.5 ]     // [ 0x36, 0.5]
    ```
    
!!! danger ""
    This operator can throw a runtime exception if the resulting value 
    overflows or underflows the range of the `Float` type.
    Exceptions are handled as specified in the [Exception handling] 
    section. 

## `Float.reciprocal()`
```ts
reciprocal(): Float
```
```ts
OP_FLOAT_RECIPROCAL
```
The `recip` operator returns the multiplicative inverse or reciprocal of
the input `Float` number. That is, the number which multiplied by the
input number, yields 1.

!!! danger ""
    This operator will throw a runtime exception if the input `Float` 
    is `0`, given that the reciprocal would be infinity, which is way 
    beyond the bounds of a `Float` number. Exceptions are handled as 
    specified in the [Exception handling] section. 

## `Float.round()`
```ts
round(): Integer
```
```ts
OP_FLOAT_ROUND
```
The `round` operator returns the value of the input `Float` number as an
`Integer` by rounding to the nearest integer.

## `Float.sum(addend)`
```ts
sum(addend: Float): Float
```
```ts
[ OP_FLOAT_SUM, addend ]
```
The `sum` operator returns the sum of the input `Float` value and the
`addend: Float` value supplied as an argument.

!!! tip "Where is the difference operator?"
    Difference is not an elementary operator in RADON.
    It is instead achieved by composing the negation (`neg`) and 
    summation (`sum`) operators.

!!! danger ""
    This operator can throw a runtime exception if the resulting value 
    overflows or underflows the range of the `Float` type.
    Exceptions are handled as specified in the [Exception handling] 
    section. 

## `Float.toString()`
```ts
toString(decimals: Integer): String
```
```ts
[ OP_FLOAT_TOSTRING, decimals ]
```
The `toString` operator returns a `String` representing the input
`Float` value using the provided base and the minimum number of
fractional digits possible.

The accepted bases are the same as in
[`String::toInteger(base)`][StringToInteger].

!!! tip ""
    If no base is specified, the default base will be `10` (decimal).

## `Float.trunc()`
```ts
trunc(): Integer
```
```ts
OP_FLOAT_TRUNCATE
```
The `truncate` operator returns the integer part of the input `Float`
number as an `Integer` by removing any fractional digits.
