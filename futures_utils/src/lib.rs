#![allow(clippy::type_complexity)]
//! Helper functions to improve developer experience when working with `Future` and `ActorFuture`
//! traits.
//!
//! The implementation of `and_then`, `map_err` and `map_ok` is based on `map` and `then` from the
//! actix project:
//!
//! https://github.com/actix/actix/blob/fdaa5d50e25ffc892f5c1c6fcc51097796debecf/src/fut/map.rs
//! https://github.com/actix/actix/blob/fdaa5d50e25ffc892f5c1c6fcc51097796debecf/src/fut/then.rs

use actix::{Actor, ActorFuture};
use futures::future::{Either, Ready};
use pin_project_lite::pin_project;
use std::{future::Future, marker::PhantomData, pin::Pin, task, task::Poll};

/// `ActorFuture` helpers
pub trait ActorFutureExt2<A: Actor>: ActorFuture<A> {
    fn and_then<F, B, T, T2, E>(self, f: F) -> AndThen<A, Self, B, F, T2, E>
    where
        Self: ActorFuture<A, Output = Result<T, E>> + Sized,
        F: FnOnce(T, &mut A, &mut <A as Actor>::Context) -> B,
        B: ActorFuture<A, Output = Result<T2, E>>,
    {
        AndThen::new(self, f)
    }

    fn map_ok<F, T, T2, E>(self, f: F) -> MapOk<A, Self, F>
    where
        Self: ActorFuture<A, Output = Result<T, E>> + Sized,
        F: FnOnce(T, &mut A, &mut <A as Actor>::Context) -> T2,
    {
        MapOk::new(self, f)
    }

    fn map_err<F, T, T2, E>(self, f: F) -> MapErr<A, Self, F>
    where
        Self: ActorFuture<A, Output = Result<E, T>> + Sized,
        F: FnOnce(T, &mut A, &mut <A as Actor>::Context) -> T2,
    {
        MapErr::new(self, f)
    }
}

impl<Act: Actor, A: ActorFuture<Act>> ActorFutureExt2<Act> for A {}

pin_project! {
    #[derive(Debug)]
    #[must_use = "futures do nothing unless polled"]
    pub struct AndThen<Act, A, B, F: 'static, T2, E>
        where
            Act: Actor,
            A: ActorFuture<Act>,
            B: ActorFuture<Act, Output = Result<T2, E>>,
    {
        #[pin]
        state: Chain<Act, A, Either<B, Ready<Result<T2, E>>>, F>,
        _phantom: PhantomData<Act>,
    }
}

impl<Act, A, B, F: 'static, T2, E> AndThen<Act, A, B, F, T2, E>
where
    Act: Actor,
    A: ActorFuture<Act>,
    B: ActorFuture<Act, Output = Result<T2, E>>,
{
    pub fn new(future: A, f: F) -> AndThen<Act, A, B, F, T2, E> {
        Self {
            state: Chain::new(future, f),
            _phantom: PhantomData,
        }
    }
}

impl<Act, A, B, F, T, T2, E> ActorFuture<Act> for AndThen<Act, A, B, F, T2, E>
where
    Act: Actor,
    A: ActorFuture<Act, Output = Result<T, E>>,
    B: ActorFuture<Act, Output = Result<T2, E>>,
    F: FnOnce(T, &mut Act, &mut <Act as Actor>::Context) -> B,
{
    type Output = B::Output;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut Act,
        ctx: &mut <Act as Actor>::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<B::Output> {
        self.project()
            .state
            .poll(act, ctx, task, |item, f, act, ctx| match item {
                Ok(item) => Either::Left(f(item, act, ctx)),
                Err(e) => Either::Right(actix::fut::result(Err(e))),
            })
    }
}

pin_project! {
    #[project = ChainProj]
    #[must_use = "futures do nothing unless polled"]
    #[derive(Debug)]
    pub enum Chain<Act, A, B, C>
    where
        Act: Actor,
        A: ActorFuture<Act>,
        B: ActorFuture<Act>,
    {
        First { #[pin] fut1: A, data: Option<C>, _phantom: PhantomData<Act>, },
        Second { #[pin] fut2: B },
        Empty,
    }
}

impl<Act, A, B, C> Chain<Act, A, B, C>
where
    Act: Actor,
    A: ActorFuture<Act>,
    B: ActorFuture<Act>,
{
    pub fn new(fut1: A, data: C) -> Chain<Act, A, B, C> {
        Chain::First {
            fut1,
            data: Some(data),
            _phantom: PhantomData,
        }
    }

    pub fn poll<F>(
        mut self: Pin<&mut Self>,
        srv: &mut Act,
        ctx: &mut <Act as Actor>::Context,
        task: &mut task::Context,
        f: F,
    ) -> Poll<B::Output>
    where
        F: FnOnce(A::Output, C, &mut Act, &mut <Act as Actor>::Context) -> B,
    {
        let mut f = Some(f);

        loop {
            let this = self.as_mut().project();
            let (output, data) = match this {
                ChainProj::First {
                    fut1,
                    data,
                    _phantom,
                } => {
                    let output = match fut1.poll(srv, ctx, task) {
                        Poll::Ready(t) => t,
                        Poll::Pending => return Poll::Pending,
                    };
                    (output, data.take().unwrap())
                }
                ChainProj::Second { fut2 } => {
                    return fut2.poll(srv, ctx, task);
                }
                ChainProj::Empty => unreachable!(),
            };

            self.set(Chain::Empty);
            let fut2 = (f.take().unwrap())(output, data, srv, ctx);
            self.set(Chain::Second { fut2 })
        }
    }
}

pin_project! {
    #[derive(Debug)]
    #[must_use = "futures do nothing unless polled"]
    pub struct MapOk<Act, A, F>
        where
            Act: Actor,
            A: ActorFuture<Act>,
    {
        #[pin]
        future: A,
        f: Option<F>,
        _phantom: PhantomData<Act>,
    }
}

impl<Act, A, F> MapOk<Act, A, F>
where
    Act: Actor,
    A: ActorFuture<Act>,
{
    pub fn new(future: A, f: F) -> MapOk<Act, A, F> {
        MapOk {
            future,
            f: Some(f),
            _phantom: PhantomData,
        }
    }
}

impl<Act, U, A, F, E, T> ActorFuture<Act> for MapOk<Act, A, F>
where
    Act: Actor,
    A: ActorFuture<Act, Output = Result<T, E>>,
    F: FnOnce(T, &mut Act, &mut <Act as Actor>::Context) -> U,
{
    type Output = Result<U, E>;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut Act,
        ctx: &mut <Act as Actor>::Context,
        task: &mut task::Context,
    ) -> Poll<Self::Output> {
        let this = self.project();
        let e = match this.future.poll(act, ctx, task) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(e) => e,
        };

        let res = match e {
            Ok(x) => Ok(this.f.take().expect("cannot poll MapOk twice")(x, act, ctx)),
            Err(e) => Err(e),
        };

        Poll::Ready(res)
    }
}

pin_project! {
    #[derive(Debug)]
    #[must_use = "futures do nothing unless polled"]
    pub struct MapErr<Act, A, F>
        where
            Act: Actor,
            A: ActorFuture<Act>,
    {
        #[pin]
        future: A,
        f: Option<F>,
        _phantom: PhantomData<Act>,
    }
}

impl<Act, A, F> MapErr<Act, A, F>
where
    Act: Actor,
    A: ActorFuture<Act>,
{
    pub fn new(future: A, f: F) -> MapErr<Act, A, F> {
        MapErr {
            future,
            f: Some(f),
            _phantom: PhantomData,
        }
    }
}

impl<Act, U, A, F, E, T> ActorFuture<Act> for MapErr<Act, A, F>
where
    Act: Actor,
    A: ActorFuture<Act, Output = Result<E, T>>,
    F: FnOnce(T, &mut Act, &mut <Act as Actor>::Context) -> U,
{
    type Output = Result<E, U>;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut Act,
        ctx: &mut <Act as Actor>::Context,
        task: &mut task::Context,
    ) -> Poll<Self::Output> {
        let this = self.project();
        let e = match this.future.poll(act, ctx, task) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(e) => e,
        };

        let res = match e {
            Err(x) => Err(this.f.take().expect("cannot poll MapErr twice")(
                x, act, ctx,
            )),
            Ok(e) => Ok(e),
        };

        Poll::Ready(res)
    }
}

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
