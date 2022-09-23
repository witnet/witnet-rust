use actix::{Actor, ActorFuture, System};
use std::{
    collections::HashMap,
    future::Future,
    hash::Hash,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

/// Given a list of elements, return the most common one. In case of tie, return `None`.
pub fn mode_consensus<I, V>(pb: I, threshold: usize) -> Option<V>
where
    I: Iterator<Item = V>,
    V: Eq + Hash,
{
    let mut bp = HashMap::new();
    let mut len_pb = 0;
    for k in pb {
        *bp.entry(k).or_insert(0) += 1;
        len_pb += 1;
    }

    let mut bpv: Vec<_> = bp.into_iter().collect();
    // Sort (beacon, peers) by number of peers
    bpv.sort_unstable_by(|a, b| b.1.cmp(&a.1));

    if bpv.len() >= 2 && (bpv[0].1 * 100) / len_pb < threshold {
        // In case of tie, no consensus
        None
    } else {
        // Otherwise, the first element is the most common
        bpv.into_iter().map(|(k, _count)| k).next()
    }
}

/// Helper function to stop the actor system if the current thread is panicking.
/// This should be used in the `Drop` implementation of essential actors.
pub fn stop_system_if_panicking(actor_name: &str) {
    if std::thread::panicking() {
        // If no actix system is running, this method does nothing
        if let Some(system) = System::try_current() {
            log::error!("Panic in {}, shutting down system", actor_name);
            system.stop_with_code(1);
        }
    }
}

/// Helper function used to test actors.
/// This should use the same code that the node uses to start the actor system.
pub fn test_actix_system<F: FnOnce() -> Fut, Fut: Future>(test_function: F) {
    // Use this flag to ensure that the test has been run, because you can never trust
    // asynchronous code
    let done = Arc::new(AtomicBool::new(false));

    // Init system
    let system = System::new();

    // Init actors
    system.block_on(async {
        test_function().await;
        done.store(true, Ordering::Relaxed);
        System::current().stop_with_code(0);
    });

    // Run system
    let res = system.run();
    res.expect("test system stop with error code");

    // Calling stop_with_code somewhere else will stop the test system, potentially skipping some
    // asserts in the test function.
    // This check ensures that the system has been stopped after running the test function.
    assert!(
        done.load(Ordering::Relaxed),
        "test system has stopped for an unknown reason"
    );
}

/// Allow to flatten Result<generic_type, error> into generic_type.
/// This is used to implement the message handlers of `StorageManagerAdapter` and other actors.
pub trait FlattenResult {
    /// Output type
    type OutputResult;
    /// Flatten result
    fn flatten_result(self) -> Self::OutputResult;
}

impl<T, E1, E2> FlattenResult for Result<Result<T, E1>, E2>
where
    E1: From<E2>,
{
    type OutputResult = Result<T, E1>;
    fn flatten_result(self) -> Self::OutputResult {
        match self {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(e.into()),
        }
    }
}

/// Helper trait to convert a `ResponseActFuture` into a normal future that can be `.await`ed.
pub trait ActorFutureToNormalFuture<A: Actor>: ActorFuture<A> {
    /// Convert an `ActorFuture` into a normal `Future` that can be `.await`ed.
    fn into_normal_future<'a>(
        mut self,
        act: &'a mut A,
        ctx: &'a mut <A as Actor>::Context,
    ) -> Pin<Box<dyn Future<Output = Self::Output> + 'a>>
    where
        Self: Sized + Unpin + 'a,
    {
        Box::pin(futures::future::poll_fn(move |task| {
            let pin_self = Pin::new(&mut self);

            ActorFuture::poll(pin_self, act, ctx, task)
        }))
    }
}

impl<T, A> ActorFutureToNormalFuture<A> for T
where
    T: ActorFuture<A>,
    A: Actor,
{
}
