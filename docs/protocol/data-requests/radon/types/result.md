# `Result<T>` type

`Result<T>` is one of the RADON complex data types. It can be thought as
an `Array<T>` that can only contain zero or one item of type `T`. In
that sense, it is somehow similar to the `Option<T>` type found in other
programming languages.

## `Result.get()`
```ts
get<T>(): T
```
```ts
OP_RESULT_GET
```
The `get` operator unwraps the input `Result<T>`. That is, it returns
its contained value assuming the `Result<T>` is positive (`Ok<T>`).

!!! danger ""
    This operator can throw a runtime exception if the input `Result<T>` 
    is not positive (`Ok<T>`) but negative
    (`Err`).
    Exceptions are handled as specified in the [Exception handling] 
    section.

## `Result.getOr()`
```ts
getOr<T>(default: T): T
```
```ts
OP_RESULT_GETOR
```
The `getOr` operator returns the `T` value enclosed in the input
`Result<T>` if it is positive (`Ok<T>`). It returns the supplied
`default: T` value otherwise.

## `Result.isOk()`
```ts
isOk(): Boolean
```
```ts
OP_RESULT_ISOK
```
The `isOk` operator returns `true` as `Boolean` if the input `Result<T>`
is positive (`Ok<T>`). It returns `false` as `Boolean` otherwise.

!!! tip ""
    Checking if a `Result` is negative (`Err`) is not an elementary 
    operator in RADON.
    It is instead achieved by composing the `isOk` and negation (`neg`) 
    operators.
