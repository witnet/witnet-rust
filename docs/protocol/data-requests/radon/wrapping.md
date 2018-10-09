# Implicit `Result<V>` wrapping

When the last call in a RADON script is successfully executed, its result does not progress directly to the next phase
in the data request life cycle. Instead, it is first wrapped in a `Result<T>` with value `Ok<T>`, where `T` is the
return data type of the last call in the script.
