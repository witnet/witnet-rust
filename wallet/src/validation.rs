//! # Validation-related types and functions.

/// A list of errors. An error is a pair of (field, error msg).
pub type Error = Vec<(&'static str, &'static str)>;

/// Create an error message associated to a field name.
pub fn error(field: &'static str, msg: &'static str) -> Error {
    vec![(field, msg)]
}

/// Combine two Results but accumulate their error if it's not Ok.
pub fn combine<A, B, C, F>(
    res1: Result<A, Error>,
    res2: Result<B, Error>,
    combinator: F,
) -> Result<C, Error>
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
