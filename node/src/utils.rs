use actix::{Actor, ActorFuture, System};
use std::{
    collections::HashMap,
    fs::File,
    future::Future,
    hash::Hash,
    io::BufReader,
    path::{Path, PathBuf},
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

/// Similar to an `Option`, but has a third case that signals the forced nature of some value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Force<T> {
    /// A forced value ("all force!").
    All(T),
    /// A regular value ("some force").
    Some(T),
    /// No value ("no force").
    None,
}

impl<T> Force<T> {
    /// Tells whether the `Force` has some non-`None` degree of force.
    #[inline]
    pub fn has_force(&self) -> bool {
        match self {
            Force::All(_) | Force::Some(_) => true,
            Force::None => false,
        }
    }

    /// Creates a new non-forced `Force` value with forced if specified.
    #[inline]
    pub fn new(value: T, force: bool) -> Force<T> {
        if force {
            Self::All(value)
        } else {
            Self::Some(value)
        }
    }

    /// If a `Force` is `None`, return a different given `Force`, otherwise, return the original.
    #[inline]
    pub fn or(self, value: Force<T>) -> Force<T> {
        match self {
            Force::None => value,
            other => other,
        }
    }

    /// Wraps a value in a `Force` with the same degree of force than an existing `Force`.
    #[inline]
    pub fn same<V>(&self, value: V) -> Force<V> {
        match self {
            Force::All(_) => Force::All(value),
            Force::Some(_) => Force::Some(value),
            Force::None => Force::None,
        }
    }

    /// Equivalent to `Option::take`.
    #[inline]
    pub fn take(&mut self) -> Force<T> {
        std::mem::replace(self, Self::None)
    }

    /// Access the inner value directly, together with a flag signaling the use of all force.
    #[inline]
    pub fn unwrap(self) -> (T, bool) {
        match self {
            Force::All(value) => (value, true),
            Force::Some(value) => (value, false),
            Force::None => panic!("Unwrapped a Force::None"),
        }
    }

    /// Access the inner value directly, or a given `Force` value if the original was `None`,
    /// together with a flag signaling the use of all force.
    #[inline]
    pub fn unwrap_or(self, value: Force<T>) -> (T, bool) {
        self.or(value).unwrap()
    }
}

impl<T> Default for Force<T> {
    #[inline]
    fn default() -> Self {
        Self::None
    }
}

impl<T> From<Option<Force<T>>> for Force<T> {
    fn from(value: Option<Force<T>>) -> Self {
        match value {
            None => Force::None,
            Some(value) => value,
        }
    }
}

/// Compose a file name out of a existing path, and a suffix that will be inserted between the file
/// stem and the file extension.
///
/// Without a suffix, this function does nothing.
pub fn file_name_compose(mut path: PathBuf, suffix: Option<String>) -> PathBuf {
    // Interpolate suffix if needed
    if let (Some(file_name), Some(extension), Some(suffix)) = (
        path.file_stem().and_then(std::ffi::OsStr::to_str),
        path.extension().and_then(std::ffi::OsStr::to_str),
        suffix,
    ) {
        path.set_file_name(format!("{}-{}.{}", file_name, suffix, extension))
    }

    path
}

/// Efficiently write data into the file system as it gets encoded on the fly using `bincode`.
pub fn serialize_to_file<D>(data: &D, path: &Path) -> Result<(), failure::Error>
where
    D: serde::Serialize,
{
    // Create file, serialize and write
    let file = witnet_util::files::create_file(path)?;
    let writer = std::io::BufWriter::new(file);
    bincode::serialize_into(writer, data)?;

    Ok(())
}

/// Efficiently read data from the file system as it gets decoded on the fly using `bincode`.
pub fn deserialize_from_file<D, E>(path: &PathBuf) -> Result<D, E>
where
    D: serde::de::DeserializeOwned,
    E: From<std::io::Error> + From<bincode::Error>,
{
    // Read file, deserialize and return
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let data = bincode::deserialize_from(reader)?;

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_name_compose() {
        let base_path = PathBuf::from("./everything/everywhere/at.once");

        let unchanged = file_name_compose(base_path.clone(), None);
        assert_eq!(unchanged, base_path);

        let changed = file_name_compose(base_path, Some("not-exactly".into()));
        let expected = PathBuf::from("./everything/everywhere/at-not-exactly.once");
        assert_eq!(changed, expected);
    }
}
