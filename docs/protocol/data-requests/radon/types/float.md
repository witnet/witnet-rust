# `Float` type

## `Float.abs()`
```ts
abs(): Float
```
```ts
FLOAT_ABS
```
The `abs` operator returns the absolute value of the input `Float` number. That is, its distance from zero, without
regard of its sign.

## `Float.ceil()`
```ts
ceil(): Int
```
```ts
FLOAT_CEIL
```
The `ceil` operator returns the smallest `Int` number greater than or equal to the input `Float` number.

## `Float.floor()`
```ts
floor(): Int
```
```ts
FLOAT_FLOOR
```
The `floor` operator returns the largest `Int` number less than or equal to the input `Float` number.

## `Float.modulo(modulus)`
```ts
modulo(modulus: Int): Float
```
```ts
[ FLOAT_MODULO, modulus ]
```
The `modulo` operator returns the remainder after the division of the input `Float` value by the `modulus: Float` value
supplied as an argument.

!!! info ""
    The resulting value always takes the same sign as the input `Float` value.

## `Float.mult(factor)`
```ts
mult(factor: Float): Float
```
```ts
[ FLOAT_MULT, factor ]
```
The `mult` operator returns the multiplication of the input `Float` value and the `factor: Float` value supplied as an
argument.

!!! tip "Where is the division operator?"
    Division is not an elementary operator in RADON.
    It is instead achieved by composing the reciprocal (`recip`) and multiplication (`mult`) operators.
    
!!! danger ""
    This operator can throw a runtime exception if the resulting value overflows or underflows the range of the `Float`
    type.
    Exceptions are handled as specified in the [Exception handling] section. 

## `Float.neg()`
```ts
neg(): Float
```
```ts
FLOAT_NEG
```
The `neg` operator returns the additive inverse, opposite, sign change or negation of the input `Float` number.
That is, the number that, when added to the input number, yields zero.

## `Float.pow(exponent)`
```ts
pow(exponent: Float): Float
```
```ts
[ FLOAT_POW, exponent ]
```
The `pow` operator returns the value of the input `Float` as base, exponentiated to the `exponent: Float` power.

!!! tip "Where is the *nth*-root operator?"
    The *nth*-root is not an elementary operator in RADON.
    It is instead achieved by composing the reciprocal (`recip`) and *nth*-power (`pow`) operators.

!!! danger ""
    This operator can throw a runtime exception if the resulting value overflows or underflows the range of the `Float`
    type.
    Exceptions are handled as specified in the [Exception handling] section. 

## `Float.recip()`
```ts
recip(): Float
```
```ts
FLOAT_RECIP
```
The `recip` operator returns the multiplicative inverse or reciprocal of the input `Float` number. That is, the number
which multiplied by the input number, yields 1.  

!!! danger ""
    This operator will throw a runtime exception if the input `Float` is `0`, given that the reciprocal would be
    infinity, which is way beyond the bounds of a `Float` number.
    Exceptions are handled as specified in the [Exception handling] section. 

## `Float.round()`
```ts
round(): Int
```
```ts
FLOAT_ROUND
```
The `round` operator returns the value of the input `Float` number as an `Int` by rounding to the nearest integer.

## `Float.sum(addend)`
```ts
sum(addend: Float): Float
```
```ts
[ FLOAT_SUM, addend ]
```
The `sum` operator returns the sum of the input `Float` value and the `addend: Float` value supplied as an argument.

!!! tip "Where is the difference operator?"
    Difference is not an elementary operator in RADON.
    It is instead achieved by composing the negation (`neg`) and summation (`sum`) operators.

!!! danger ""
    This operator can throw a runtime exception if the resulting value overflows or underflows the range of the `Float`
    type.
    Exceptions are handled as specified in the [Exception handling] section. 

## `Float.toString()`
```ts
toString(decimals: Int): String
```
```ts
[ FLOAT_TOSTRING, decimals ]
```
The `toString` operator returns a `String` representing the input `Float` value using the provided base and the minimum
number of fractional digits possible.

The accepted bases are the same as in [`String::toInt(base)`][StringToInt].

!!! tip ""
    If no base is specified, the default base will be `10` (decimal).

## `Float.trunc()`
```ts
trunc(): Int
```
```ts
FLOAT_TRUNC
```
The `trunc` operator returns the integer part of the input `Float` number as an `Int` by removing any fractional digits.
