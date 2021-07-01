#![allow(clippy::type_complexity)]
//! Helper functions to improve developer experience when working with `Future` and `ActorFuture`
//! traits.

use std::future::Future;

pub trait TryFutureExt2: Future {
    /// Flattens one level of nested results: converts a
    /// `Future<Output = Result<Result<T, E1>, E2>>` into a `Future<Output = Result<T, E>`.
    #[allow(clippy::type_complexity)]
    fn flatten_err<T, E1, E2, E>(
        self,
    ) -> futures::future::Map<Self, fn(Result<Result<T, E1>, E2>) -> Result<T, E>>
    where
        Self: Sized,
        Self: Future<Output = Result<Result<T, E1>, E2>>,
        E: From<E1>,
        E: From<E2>,
    {
        fn flatten_err_inner<T, E1, E2, E>(res: Result<Result<T, E1>, E2>) -> Result<T, E>
        where
            E: From<E1>,
            E: From<E2>,
        {
            match res {
                Ok(Ok(x)) => Ok(x),
                Ok(Err(e)) => Err(e.into()),
                Err(e) => Err(e.into()),
            }
        }
        futures::FutureExt::map(self, flatten_err_inner)
    }
}

impl<T: Future> TryFutureExt2 for T {}
