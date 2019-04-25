# Exception handling

When a call in a RADON script causes a runtime exception, the script
execution flow is immediately stopped. But do not panic: this will not
cause the entire data request to fail. RADON has a solid strategy for
recovering from those situations.

Exceptions generated in a certain stage in the data request life cycle
do progress to the next stage wrapped in a `Result<V>` with an `Err`
value.

This provides a type safe API for handling success and errors in a
uniform way, and gives the developer the choice to recover from
exceptions as appropriate for the use case (dropping errors, mapping
them into default values, etc.)

`Err` values can not be inspected inside the context of RADON, but as
soon as they are reported to the outside or to other network through a
bridge, they turn into a convenient structure like this:

```ts
{
  code: -1,                     // RADON error code
  stage: STAGE_RETRIEVAL,       // Stage in which the exception happened
  callIndex: 2,                 // Position of the failed call inside the script
  callOperator: OP_ARRAY_GET,   // Code of the operator in the failed call 
  callArguments: [ 7 ],         // Arguments in the failed call
}
``` 

Error codes and user-friendly messages for all of them are defined under
the [Constants section][constants].

[constants]: /protocol/data-requests/radon/constants/#errors