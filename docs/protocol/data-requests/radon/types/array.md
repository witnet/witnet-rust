# `Array<T>` type

An `Array<T>` is an ordered sequence of zero, one or more values or data structures of the same type, `T`.
    
## `Array.count()`
```ts
count(): Integer
```
```ts
OP_ARRAY_COUNT
```
The `count` operator just takes an `Array<T>` and returns its length as
an `Integer`.
    
## `Array.every(function)`
```ts
every(function: (item: T) => Boolean)): Boolean
```
```ts
[ OP_ARRAY_EVERY, function ]
```
The `every` operator returns `true` if the result of applying a function
on every each of the items in a given `Array<T>` is `true`. It returns
`false` otherwise.

The supplied `(input: T): O` function can be either be a valid
[subscript] over type `T` or one of the
[predefined filtering functions][filters].

!!! danger ""
    This operator can throw a runtime exception under several
    circumstances, including:
    
    - `T` in the input `Array<T>` is `Bytes`

## `Array.filter(function)`
```ts
filter(function: (item: T) => Boolean): Array<T>
```
```ts
[ OP_ARRAY_FILTER, function ]
```
This operator applies a filtering function on an `Array`. That is, it
will apply the function on every item in the `Array` and drop those
returning `false` values.

The supplied `(input: T): O` function can be either be a valid
[subscript] over type `T` or one of the
[predefined filtering functions][filters].

!!! danger ""
    This operator can throw a runtime exception under several
    circumstances, including:
    
    - `T` in the input `Array<T>` is `Bytes`

## `Array.flatten()`
```ts
flatten(depth?: Integer): Array<T>
```
```ts
OP_ARRAY_FLATTEN
```
The `flatten` operator returns a new `Array<T>` with all the contained
`Array<T>` concatenated into it recursively up to the supplied `depth:
Integer`.

!!! tip "Flattening `Array<Result<T>>` into `Array<T>`"
    As a `Result<T>` is roughly equivalent to an `Array<T>` with zero 
    or one item, applying `flatten` on an `Array<Result<T>>` 
    conveniently returns an `Array<T>` containing only the unwrapped 
    positive results, that is, it will drop all the errors.

!!! info
    If the `depth` argument is not specified, it is assumed to be `1`, 
    i.e. `flatten` will only concatenate the first level of nesting.
    
!!! info
    Applying `flatten` on an `Array<T>` will yield exactly the same 
    `Array<T>` if `T` is not a `Array<U>` or `Result<U>`, where `U` 
    can be any type.
    

## `Array.get(index)`
```ts
get(index: Integer): T
```
```ts
[ OP_ARRAY_GET, index ]
```
The `get` operator returns the `T` item at `index: Integer` in an
`Array<T>`.

!!! danger ""
    This operator can throw a runtime exception if the supplied 
    `index: Integer` is out of the range of the input `Array<T>`.
    Exceptions are handled as specified in the [Exception handling] 
    section.

!!! danger "Incentive safety"
    This operator may introduce adverse incentives if used in the 
    aggregation or consensus stages.

## `Array.map(operator)`
```ts
map<O>(function: (item: T) => O): Array<O>
```
```ts
[ OP_ARRAY_MAP, subscript ]
```
The `map` operator returns a new `Array` with the results of executing a
supplied `(item: T): O` function on every `T` element in the input
`Array<T>`.

The supplied `(input: T): O` function must be a valid [subscript] over
type `T`.

!!! example
    ```ts
    95 43 70 92 55 92 72 3B 53 92 56 03
    ```
    ```ts
    [
        OP_STRING_PARSEJSON,                    // 0x43,
        OP_BYTES_TOARRAY,                       // 0x70,
        [ OP_ARRAY_MAP, [                       // [ 0x55, [
            OP_BYTES_TOFLOAT,                   //      0x72,
            OP_FLOAT_TRUNCATE                   //      0x3B
        ] ],                                    // ] ]
        [ OP_ARRAY_REDUCE, REDUCER_AVG_MEAN ]   // [ 0x56, 0x03 ]
    ]
    ```

## `Array.reduce(function)`
```ts
reduce(function: (item: T) => T)): T
```
```ts
[ OP_ARRAY_REDUCE, function ]
```
The `reduce` operator aggregates the items in the input `Array<T>` using
a the supplied `(input: T): O` function and returns a single item of
type `T`.

The supplied `(input: T): O` function can be either a valid [subscript]
over type `T` or one of the [predefined reducing functions][reducer].

!!! danger ""
    This operator can throw a runtime exception under several
    circumstances, including:
    
    - `T` in the input `Array<T>` is `Bytes`
    - the reducing function is not `mode` and `T` in the input
    `Array<T>` is not `Integer` or `Float`
    

## `Array.some(function)`
```ts
some(function: (item: T) => Boolean): Boolean
```
```ts
[ OP_ARRAY_SOME, function ]
```
The `some` operator returns `true` if the result of applying a function
on at least one of the items in a given `Array<T>` is `true`. It returns
`false` otherwise.

The supplied `(item: T): Boolean` function can be either one of the
[predefined filtering functions][filters] or a valid [subscript] with
input type `T` and output type `Boolean`.

## `Array.sort(mapFunction, ascending)`
```ts
sort<V>(
    mapFunction: (item: T) => V,
    ascending: Boolean = true
): Array<T>
```
```ts
[ OP_ARRAY_SORT, mapFunction, ascending ]
```
The `sort` operator returns a new `Array<T>` with the very same items
from the input `Array<T>` but ordered according to the sorting criteria
defined by the supplied `mapFunction: (item: T) => V` and `ascending:
Boolean` arguments.

The supplied `mapFunction: (item: T) => V` must be a valid [subscript]
over type `T`, and its `V` output type must be one of the value types:
`Boolean`, `Integer`, `Float` or `String`. This function gives the
`sort` operator the power to sort the items in the input `Array<T>` not
by their values but by the values resulting from applying some
computation on them.

!!! example
    ```ts
    93 58 92 61 A3 61 67 65 C3
    ```
    ```ts
    [
        [ OP_ARRAY_SORT, [          // [ 0x58, [
            [ OP_MAP_GET, "age" ]   //      [ 0x61, "age" ]
        ], false ]                  // ], false ]
    ]
    ```

!!! tip "Remember"

    The "identity" subscript (one that returns its own input without 
    any transformation) is expressed in RADON as an empty `Array`:

    ```ts
    92 58 90
    ```
    ```ts
    [ OP_ARRAY_SORT, [] ]   // [ 0x58, [] ]
    ```

!!! danger "Incentive safety"
    This operator may introduce adverse incentives if used in the
    aggregation or consensus stages.

## `Array.take(min, max)`
```ts
take(min?: Integer, max?: Integer): Array<T>
```
```ts
[ OP_ARRAY_TAKE, min, max ] // Providing both a minimum and a maximum
```
The `take` operator returns a new `Array<T>` with at least the `min:
Integer` first items in the input `Array<T>` and at most `max: Integer`
items.

!!! tip "Take at least / Take at most / Take exactly"

    This operator can be easily used to reproduce the *"take at least N
    items"* and *"take at most N items"* behaviors separately:

    ```ts
    [ OP_ARRAY_TAKE, 5 ]    // "take at least 5 items"
    ```
    ```ts
    [ OP_ARRAY_TAKE, 0, 10 ]    // "take at most 10 items"
    ```
    Conversely, this will take **only** the item at position `7` (or
    fail if there are not enough items):

    ```ts
    [ OP_ARRAY_TAKE, 7, 7 ]
    ```

!!! danger ""
    This operator can throw a runtime exception if the input `Array<T>` 
    does not contain enough items to satisfy the minimum amount of
    items required by the supplied `min: Integer` argument.

!!! danger "Incentive safety"
    This operator may introduce adverse incentives if used in the 
    aggregation or consensus stages.

[subscript]: ../../subscripts
[filters]: ../../functions#filtering-functions
[reducers]: ../../functions#reducing-functions