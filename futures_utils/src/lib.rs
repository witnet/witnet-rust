#![allow(clippy::type_complexity)]
//! Helper functions to improve developer experience when working with `Future` and `ActorFuture`
//! traits.
//!
//! The implementation of `and_then`, `map_err` and `map_ok` is based on `map` and `then` from the
//! actix project:
//!
//! https://github.com/actix/actix/blob/fdaa5d50e25ffc892f5c1c6fcc51097796debecf/src/fut/map.rs
//! https://github.com/actix/actix/blob/fdaa5d50e25ffc892f5c1c6fcc51097796debecf/src/fut/then.rs

use actix::{fut::IntoActorFuture, Actor, ActorFuture};
use pin_project_lite::pin_project;
use std::{future::Future, pin::Pin, task, task::Poll};

/// `ActorFuture` helpers
pub trait ActorFutureExt: ActorFuture {
    fn and_then<F, B, T, T2, E>(self, f: F) -> AndThen<Self, B, F, T2, E>
    where
        Self: ActorFuture<Output = Result<T, E>> + Sized,
        F: FnOnce(T, &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> B,
        B: IntoActorFuture<Actor = Self::Actor, Output = Result<T2, E>>,
    {
        AndThen::new(self, f)
    }

    fn map_ok<F, T, T2, E>(self, f: F) -> MapOk<Self, F>
    where
        Self: ActorFuture<Output = Result<T, E>> + Sized,
        F: FnOnce(T, &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> T2,
    {
        MapOk::new(self, f)
    }

    fn map_err<F, T, T2, E>(self, f: F) -> MapErr<Self, F>
    where
        Self: ActorFuture<Output = Result<E, T>> + Sized,
        F: FnOnce(T, &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> T2,
    {
        MapErr::new(self, f)
    }
}

impl<A: ActorFuture> ActorFutureExt for A {}

pin_project! {
    #[derive(Debug)]
    #[must_use = "futures do nothing unless polled"]
    pub struct AndThen<A, B, F: 'static, T2, E>
        where
            A: ActorFuture,
            B: IntoActorFuture<Actor = A::Actor, Output = Result<T2, E>>,
    {
        #[pin]
        state: Chain<A, actix::fut::Either<B::Future, actix::fut::FutureResult<T2, E, A::Actor>>, F>,
    }
}

impl<A, B, F: 'static, T2, E> AndThen<A, B, F, T2, E>
where
    A: ActorFuture,
    B: IntoActorFuture<Actor = A::Actor, Output = Result<T2, E>>,
{
    pub fn new(future: A, f: F) -> AndThen<A, B, F, T2, E> {
        Self {
            state: Chain::new(future, f),
        }
    }
}

impl<A, B, F, T, T2, E> ActorFuture for AndThen<A, B, F, T2, E>
where
    A: ActorFuture<Output = Result<T, E>>,
    B: IntoActorFuture<Actor = A::Actor, Output = Result<T2, E>>,
    F: FnOnce(T, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> B,
{
    type Output = B::Output;
    type Actor = A::Actor;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<B::Output> {
        self.project()
            .state
            .poll(act, ctx, task, |item, f, act, ctx| match item {
                Ok(item) => actix::fut::Either::left(f(item, act, ctx).into_future()),
                Err(e) => actix::fut::Either::right(actix::fut::result(Err(e))),
            })
    }
}

pin_project! {
    #[project = ChainProj]
    #[must_use = "futures do nothing unless polled"]
    #[derive(Debug)]
    pub enum Chain<A, B, C> {
        First { #[pin] fut1: A, data: Option<C> },
        Second { #[pin] fut2: B },
        Empty,
    }
}

impl<A, B, C> Chain<A, B, C>
where
    A: ActorFuture,
    B: ActorFuture<Actor = A::Actor>,
{
    pub fn new(fut1: A, data: C) -> Chain<A, B, C> {
        Chain::First {
            fut1,
            data: Some(data),
        }
    }

    pub fn poll<F>(
        mut self: Pin<&mut Self>,
        srv: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut task::Context,
        f: F,
    ) -> Poll<B::Output>
    where
        F: FnOnce(A::Output, C, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> B,
    {
        let mut f = Some(f);

        loop {
            let this = self.as_mut().project();
            let (output, data) = match this {
                ChainProj::First { fut1, data } => {
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
    pub struct MapOk<A, F>
        where
            A: ActorFuture,
    {
        #[pin]
        future: A,
        f: Option<F>,
    }
}

impl<A, F> MapOk<A, F>
where
    A: ActorFuture,
{
    pub fn new(future: A, f: F) -> MapOk<A, F> {
        MapOk { future, f: Some(f) }
    }
}

impl<U, A, F, E, T> ActorFuture for MapOk<A, F>
where
    A: ActorFuture<Output = Result<T, E>>,
    F: FnOnce(T, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> U,
{
    type Output = Result<U, E>;
    type Actor = A::Actor;
    fn poll(
        self: Pin<&mut Self>,
        act: &mut Self::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
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
    pub struct MapErr<A, F>
        where
            A: ActorFuture,
    {
        #[pin]
        future: A,
        f: Option<F>,
    }
}

impl<A, F> MapErr<A, F>
where
    A: ActorFuture,
{
    pub fn new(future: A, f: F) -> MapErr<A, F> {
        MapErr { future, f: Some(f) }
    }
}

impl<U, A, F, E, T> ActorFuture for MapErr<A, F>
where
    A: ActorFuture<Output = Result<E, T>>,
    F: FnOnce(T, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> U,
{
    type Output = Result<E, U>;
    type Actor = A::Actor;
    fn poll(
        self: Pin<&mut Self>,
        act: &mut Self::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
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
