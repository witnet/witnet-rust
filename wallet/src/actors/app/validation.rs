/// A list of errors. An error is a pair of (field, error msg).
pub type ValidationErrors = Vec<(String, String)>;

/// Create an error message associated to a field name.
pub fn field_error<F: ToString, M: ToString>(field: F, msg: M) -> ValidationErrors {
    vec![(field.to_string(), msg.to_string())]
}

/// Combine two Results but accumulate their errors.
pub fn combine_field_errors<A, B, C, F>(
    res1: Result<A, ValidationErrors>,
    res2: Result<B, ValidationErrors>,
    combinator: F,
) -> Result<C, ValidationErrors>
where
    F: FnOnce(A, B) -> C,
{
    match (res1, res2) {
        (Err(mut err1), Err(err2)) => {
            err1.extend(err2);
            Err(err1)
        }
        (Err(err1), _) => Err(err1),
        (_, Err(err2)) => Err(err2),
        (Ok(a), Ok(b)) => Ok(combinator(a, b)),
    }
}
