use super::*;

mod error;
mod validations;

pub use error::ValidationErrors;

type Result<T> = std::result::Result<T, ValidationErrors>;

#[inline]
pub fn error(field: impl Into<String>, msg: impl Into<String>) -> ValidationErrors {
    vec![(field.into(), msg.into())].into()
}

/// Combine three Results accumulating their errors.
fn join_errors<A, B, C, F>(res1: Result<A>, res2: Result<B>, f: F) -> Result<C>
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
        (Ok(a), Ok(b)) => Ok(f(a, b)),
    }
}

fn join3_errors<A, B, C, D, F>(res1: Result<A>, res2: Result<B>, res3: Result<C>, f: F) -> Result<D>
where
    F: FnOnce(A, B, C) -> D,
{
    join_errors(
        join_errors(res1, res2, |val1, val2| (val1, val2)),
        res3,
        |(val1, val2), val3| f(val1, val2, val3),
    )
}
