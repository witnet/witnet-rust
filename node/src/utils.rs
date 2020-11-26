use std::{collections::HashMap, hash::Hash};

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

#[cfg(test)]
/// Helper function used to test actors
pub fn test_actix_system<F: 'static + FnOnce() -> Fut, Fut: futures03::Future>(f: F) {
    actix::System::run(|| {
        let fut03 = async move {
            f().await;

            actix::System::current().stop();
            Result::<(), ()>::Ok(())
        };

        // Magic conversion from std::future::Future (futures 0.3) and futures::Future (futures 0.1)
        let fut = futures_util::compat::Compat::new(Box::pin(fut03));

        actix::Arbiter::spawn(fut);
    })
    .unwrap();
}
