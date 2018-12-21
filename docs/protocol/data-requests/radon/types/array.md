# `Array<T>` type

An `Array<T>` is an ordered sequence of zero, one or more values or data structures of the same type, `T`.
    
## `Array.count()`
```ts
count(): Int
```
```ts
ARRAY_COUNT
```
The `count` operator just takes an `Array<T>` and returns its length as an `Int`.
    
## `Array.every(function)`
```ts
every(function: (item: T) => Boolean)): Boolean
```
```ts
[ ARRAY_EVERY, function ]
```
The `.every` operator returns `True` if the result of applying a function on every each of the items in a given
`Array<T>` is `True`. It returns `False` otherwise.

The supplied `(input: T): O` function can be either be a valid [subscript] over type `T` or one of the
[predefined filtering functions][filters].

!!! danger ""
    This operator can throw a runtime exception under several circumstances, including:
    
    - `T` in the input `Array<T>` is `Mixed`

## `Array.filter(function)`
```ts
filter(function: (item: T) => Boolean): Array<T>
```
```ts
[ ARRAY_FILTER, function ]
```
This operator applies a filtering function on an `Array`. That is, it will apply the function on every item in the
`Array` and drop those returning `False` values.

The supplied `(input: T): O` function can be either be a valid [subscript] over type `T` or one of the
[predefined filtering functions][filters].

!!! danger ""
    This operator can throw a runtime exception under several circumstances, including:
    
    - `T` in the input `Array<T>` is `Mixed`

## `Array.flatten()`
```ts
flatten(depth: Int): Array<T>
```
```ts
ARRAY_FLATTEN
```
The `flatten` operator returns a new `Array<T>` with all the contained `Array<T>` concatenated into it recursively up
to the supplied `depth: Int`.

As `Result<T>` is equivalent to an `Array<T>` with zero or one item, applying `flatten` on an `Array<Result<T>>`
conveniently returns an `Array<T>` containing only the unwrapped positive results.

!!! example "Example: flattening `Array<Result<T>>` into `Array<T>`"

    ```ts
    [                                       // Array<Int>
        [ ARRAY_MAP, [                      // Int
            [ INT_MULT, 2 ]                 // Int
        ] ],                                // Array<Result<Int>>
        ARRAY_FLATTEN ,                     // Array<Int>>
        [ ARRAY_REDUCE, REDUCER_AVG_MEAN ]  // Int
    ]                                       // Result<Int>
    ```

## `Array.get(index)`
```ts
get(index: Int): T
```
```ts
[ ARRAY_GET, index ]
```
The `get` operator returns the `T` item at `index: Int` in an `Array<T>`.

!!! danger ""
    This operator can throw a runtime exception if the supplied `index: Int` is out of the range of the input
    `Array<T>`.
    Exceptions are handled as specified in the [Exception handling] section.

!!! danger "Incentive safety"
    This operator may introduce adverse incentives if used in the aggregation or consensus stages.

## `Array.map(operator)`
```ts
map<O>(function: (item: T) => O): Array<Result<O>>
```
```ts
[ ARRAY_MAP, operator ]
```
The `map` operator returns a new `Array` with the results of executing a supplied `(item: T): O` function on every `T`
element in the input `Array<T>`, each wrapped in a `Result`.

The supplied `(input: T): O` function must be a valid [subscript] over type `T`.

It could happen that the supplied operator failed on some of the `T` values in the input `Array<T>`. In such case,
breaking the data flow and throwing a runtime exception would be unacceptable. Instead, the `map` operator wraps each
of the items in the returned `Array` into a `Result`. Therefore, the return type of the `map` operator is
`Array<Result<O>>`, where `O` is the return type of the supplied `(item: T): O` operator.

!!! example

    ```ts
    [                                       // String
        STRING_PARSEJSON,                   // Mixed
        [ MIXED_TOARRAY, TYPE_FLOAT ],      // Array<Float>
        [ ARRAY_MAP, FLOAT_TRUNC ],         // Array<Result<Float>>
        [ ARRAY_FLATMAP ],                  // Array<Float>,
        [ ARRAY_REDUCE, REDUCER_AVG_MEAN ]  // Float
    ]                                       // Result<Float>
    ```

## `Array.reduce(function)`
```ts
reduce(function: (item: T) => T)): T
```
```ts
[ ARRAY_REDUCE, function ]
```
The `reduce` operator aggregates the items in the input `Array<T>` using a the supplied `(input: T): O` function and
returns a single item of type `T`.

The supplied `(input: T): O` function can be either a valid [subscript] over type `T` or one of the
[predefined reducing functions][reducer].

!!! danger ""
    This operator can throw a runtime exception under several circumstances, including:
    
    - `T` in the input `Array<T>` is `Mixed`
    - the reduction function is not `mode` and `T` in the input `Array<T>` is not `Int` or `Float`
    

## `Array.some(function)`
```ts
some(function: (item: T) => Boolean): Boolean
```
```ts
[ ARRAY_SOME, function ]
```
The `some` operator returns `True` if the result of applying a function on at least one of the items in a given
`Array<T>` is `True`. It returns `False` otherwise.

The supplied `(item: T): Boolean` function can be either one of the [predefined filtering functions][filters] or a
valid [subscript] with input type `T` and output type `Boolean`.

## `Array.sort(mapFunction, ascending)`
```ts
sort<V>(
    mapFunction: (item: T) => V,
    ascending: Boolean = True
): Array<T>
```
```ts
[ ARRAY_SORT, mapFunction, ascending ]
```
The `sort` operator returns a new `Array<T>` with the very same items from the input `Array<T>` but ordered according
to the sorting criteria defined by the supplied `mapFunction: (item: T) => V` and `ascending: Boolean` arguments.

The supplied `mapFunction: (item: T) => V` must be a valid [subscript] over type `T`, and its `V` output type must be
one of the value types: `Boolean`, `Int`, `Float` or `String`. This function gives the `sort` operator the power to
sort the items in the input `Array<T>` not by their values but by the values resulting from applying some computation
on them.

!!! example

    ```ts
    [                           // Array<Map<String, Int>>
        [ ARRAY_SORT, [         // Map<String, Int>
            [ MAP_GET, "age" ]  // Int
        ], False ]              // Array<Map<String, Int>>
    ]                           // Result<Array<Map<String, Int>>>
    ```

!!! tip "Remember"

    The "identity" subscript (one that returns its own input without any transformation) is expressed in RADON as an
    empty `Array`:

    ```ts
    [ ARRAY_SORT, [] ]
    ```

!!! danger "Incentive safety"
    This operator may introduce adverse incentives if used in the aggregation or consensus stages.

## `Array.take(min, max)`
```ts
take(min?: Int, max?: Int): Array<T>
```
```ts
[ ARRAY_TAKE, min, max ] // Providing both a minimum and a maximum
```
The `take` operator returns a new `Array<T>` with at least the `min: Int` first items in the input `Array<T>` and at
most `max: Int` items.

!!! tip "Take at least / Take at most / Take exactly"

    This operator can be easily used to reproduce the *"take at least N items"* and *"take at most N items"* behaviors
    separately:

    ```ts
    [ ARRAY_TAKE, 5 ] // "take at least 5 items"
    ```
    ```ts
    [ ARRAY_TAKE, 0, 10 ] // "take at most 10 items"
    ```
    Conversely, this will take **exactly** `7` item (or fail if there are not enough items):

    ```ts
    [ ARRAY_TAKE, 7, 7 ]
    ```

!!! danger ""
    This operator can throw a runtime exception if the input `Array<T>` does not contain enough items to satisfy the
    minimum amount of items required by the supplied `min: Int` argument.

[subscript]: ../../subscripts
[filters]: ../../functions#filtering-functions
[reducers]: ../../functions#reducing-functions