# Exception handling

When a call in a RADON script causes a runtime exception, the script execution flow is immediately stopped. But do not
panic: this does not seem the entire data request will fail. RADON has a solid strategy for recovering from those
situations. 

Exceptions generated in a certain phase in the data request life cycle do progress to the next phase wrapped in a
`Result<V>` with value `Err`.

This provides a type safe API for handling success and errors in a uniform way, and gives the developer the choice to
recover from exceptions as appropriate for the use case (dropping errors, mapping them into default values, etc.)
