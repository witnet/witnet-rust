# `Int` type

## `Int.abs()`
```ts
abs(): Int
```
```ts
INT_ABS
```
The `abs` operator returns the absolute value of the input `Int` number. That is, its distance from zero, without regard
of its sign.

## `Int.match(categories, default)`
```ts
match<V>(categories: Map<Int, V>, default?: V): V
```
```ts
[ INT_MATCH, [ /** `[key, value]` pairs **/ ] , default]
```
The `match` operator maps the input `Int` into different `V` values as defined in a `Map<Int, V>` by
checking if it matches against any of its `Int` keys. That is, it classifies the input `Int` value into
separate *compartments* or *buckets*.

If the input `Int` value is found as a key of `categories: Map<Int, V>`, it returns the `V` value associated
to such key. It returns the `default: V` value otherwise.

!!! example
    ```ts
    [ INT_MATCH, [ [ 1, "One" ], [ 2, "Two" ], [ 3, "Three" ] ], "Other" ]
    ```

!!! danger ""
    This operator will throw a runtime exception if no `default` value is provided and the input `Int` value
    is not found as a key of `categories: Map<Int, V>`.
    Exceptions are handled as specified in the [Exception handling] section. 

## `Int.modulo(modulus)`
```ts
modulo(modulus: Int): Int
```
```ts
[ INT_MODULO, modulus ]
```
The `modulo` operator returns the remainder after the division of the input `Int` value by the `modulus: Int` value
supplied as an argument.

!!! info ""
    The resulting value always takes the same sign as the input `Int` value.

## `Int.mult(factor)`
```ts
mult(factor: Int): Int
```
```ts
[ INT_MULT, factor ]
```
The `mult` operator returns the multiplication of the input `Int` value and the `factor: Int` value supplied as an
argument.

!!! tip "Where is the division operator?"
    Division is not an elementary operator in RADON.
    It is instead achieved by composing the reciprocal (`recip`) and multiplication (`mult`) operators.
    
!!! danger ""
    This operator can throw a runtime exception if the resulting value overflows or underflows the range of the `Int`
    type.
    Exceptions are handled as specified in the [Exception handling] section. 

## `Int.neg()`
```ts
neg(): Int
```
```ts
INT_NEG
```
The `neg` operator returns the additive inverse, opposite, sign change or negation of the input `Int` number.
That is, the number that, when added to the input number, yields zero.

## `Int.pow(exponent)`
```ts
pow(exponent: Float): Float
```
```ts
[ INT_POW, exponent ]
```
The `pow` operator returns the value of the input `Int` as base, exponentiated to the `exponent: Float` power.

!!! tip "Where is the *nth*-root operator?"
    The *nth*-root is not an elementary operator in RADON.
    It is instead achieved by composing the reciprocal (`recip`) and *nth*-power (`pow`) operators.

!!! danger ""
    This operator can throw a runtime exception if the resulting value overflows or underflows the range of the `Float`
    type.
    Exceptions are handled as specified in the [Exception handling] section.

## `Int.recip()`
```ts
recip(): Float
```
```ts
INT_RECIP
```
The `recip` operator returns the multiplicative inverse or reciprocal of the input `Int` number. That is, the number
which multiplied by the input number, yields 1.

!!! danger ""
    This operator will throw a runtime exception if the input `Int` is `0`, given that the reciprocal would be infinity,
    which is way beyond the bounds of a `Float` number.
    Exceptions are handled as specified in the [Exception handling] section. 

## `Int.sum(addend)`
```ts
sum(addend: Int): Int
```
```ts
[ INT_SUM, addend ]
```
The `sum` operator returns the sum of the input `Int` value and the `addend: Int` value supplied as an argument.

!!! tip "Where is the difference operator?"
    Difference is not an elementary operator in RADON.
    It is instead achieved by composing the negation (`neg`) and summation (`sum`) operators.

!!! danger ""
    This operator can throw a runtime exception if the resulting value overflows or underflows the range of the `Int`
    type.
    Exceptions are handled as specified in the [Exception handling] section.

## `Int.toFloat()`
```ts
toFloat(): Float
```
```ts
INT_TOFLOAT
```
The `toFloat` operator returns the value of the input `Int` as a floating point number.

## `Int.toString()`
```ts
toString(base?: Int): String
```
```ts
[ INT_TOSTRING, base ]
```
The `toString` operator returns a `String` representing the input `Int` value using the provided base.

The accepted bases are the same as in [`String::toInt(base)`][StringToInt].

!!! tip ""
    If no base is specified, the default base will be `10` (decimal).
