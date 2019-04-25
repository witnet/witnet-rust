# RADON data types

The basic data types (also called *value types*) existing in RADON are 
modelled to resemble those of most typed programming languages:

- `Boolean`
- `Integer`
- `Float`
- `String`

Additionaly, there exist six complex data types or *structure types*:

- `Array<T>`
- `Map<T>`
- `Mixed`
- `Null`
- `Result<T>`

Each of these nine types and their available operators are explained 
below.

!!! tip "Reading data types documentation"
    Operators for each of the data types in this documentation are 
    specified as:
    ```ts
    // TypeScript-alike function signature
    nameOfTheMethod(argument: TypeOfArgument): ReturnTypeOfMethod
    ```
    ```ts
    // Actual usage in RADON
    OPERATORNAME // Operators without arguments, aka "simple calls"
    [ OPERATORNAME, argument ] // Operators with arguments, aka "compound calls"
    ```

!!! info "Constants"
    All across this documentation, unquoted uppercase names like 
    `STRING_PARSEJSON` identify different operators and constants that 
    equate to a single byte when encoded.

    A list of constants can be found in the 
    [Contants section][constants].

[constants]: /protocol/data-requests/radon/constants/