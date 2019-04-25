# Implicit `Result<V>` wrapping

When the last call in a RADON script is successfully executed, its
result does not progress directly to the next stage in the data request
life cycle. Instead, it is first wrapped in a `Result<T>` with value
`Ok<T>`, where `T` is the return data type of the last call in the
script.

If on the contrary the script [failed][exceptions] during the execution
of any of its calls, the error will progress to the next stage as a
negative `Result<T>` containing an `Err` with the information of the
error.

[exceptions]: /protocol/data-requests/radon/exceptions/