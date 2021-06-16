//! Total Reputation Set

use std::{
    cmp::Ordering,
    collections::{
        hash_map::{Entry, RandomState},
        HashMap, VecDeque,
    },
    hash::{BuildHasher, Hash},
    iter,
    ops::{AddAssign, SubAssign},
};

use crate::error::{NonSortedAlpha, RepError};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Total Reputation Set
///
/// This data structure keeps track of the total reputation `V`
/// associated to every identity `K`. Reputation is issued in "packets" which
/// expire over time `A`. In order to keep track of what to expire and when,
/// the reputation packets are stored in a queue ordered by expiration date.
///
/// The method `gain(alpha, vec![(id1, diff1)])` will add a coin with value
/// `diff1` to identity `id1`, which will expire at time `alpha`.
///
/// The method `expire(alpha)` will invalidate all the reputation packets with `expiration_time < alpha`.
///
/// The method `penalize(id, f)` will apply a penalization function `f` to an identity `id`.
/// The penalization amount will be subtracted from the most recent reputation packets (those which will
/// expire later).
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Debug)]
pub struct TotalReputationSet<K, V, A, S = RandomState>
where
    K: Clone + Eq + Hash,
    V: AddAssign + Clone + Default + Ord + SubAssign,
    A: Clone + Ord,
    S: BuildHasher + Default,
{
    // A cache of <identity: total_reputation>
    // All the identities with reputation are in the cache: identities
    // not in the cache must have null reputation
    #[cfg_attr(
        feature = "serde",
        serde(bound(
            serialize = "HashMap<K, V, S>: Serialize",
            deserialize = "HashMap<K, V, S>: Deserialize<'de>"
        ))
    )]
    map: HashMap<K, V, S>,
    // The list of reputation packets ordered by expiration
    #[cfg_attr(
        feature = "serde",
        serde(bound(
            serialize = " VecDeque<(A, HashMap<K, V, S>)>: Serialize",
            deserialize = " VecDeque<(A, HashMap<K, V, S>)>: Deserialize<'de>"
        ))
    )]
    queue: VecDeque<(A, HashMap<K, V, S>)>,
}

impl<K, V, A> TotalReputationSet<K, V, A, RandomState>
where
    K: Clone + Eq + Hash,
    V: AddAssign + Clone + Default + Ord + SubAssign,
    A: Clone + Ord,
{
    /// Builds a new empty Trs
    pub fn new() -> Self {
        Self::with_hasher()
    }

    /// Builds a new Trs from an ordered list
    pub fn from_queue<I, I2>(i1: I) -> Result<Self, NonSortedAlpha<A>>
    where
        I: IntoIterator<Item = (A, I2)>,
        I2: IntoIterator<Item = (K, V)>,
    {
        Self::from_queue_with_hasher(i1)
    }
}

impl<K, V, A, S> TotalReputationSet<K, V, A, S>
where
    K: Clone + Eq + Hash,
    V: AddAssign + Clone + Default + Ord + SubAssign,
    A: Clone + Ord,
    S: BuildHasher + Default,
{
    /// Builds a new empty Trs with a custom hasher
    pub fn with_hasher() -> Self {
        let map = HashMap::with_hasher(S::default());
        let queue = VecDeque::new();

        Self { map, queue }
    }

    /// Builds a new Trs from an ordered list with a custom hasher
    pub fn from_queue_with_hasher<I1, I2>(queue: I1) -> Result<Self, NonSortedAlpha<A>>
    where
        I1: IntoIterator<Item = (A, I2)>,
        I2: IntoIterator<Item = (K, V)>,
    {
        let mut trs = Self::with_hasher();
        for (alpha, diff) in queue {
            trs.gain(alpha, diff)?;
        }

        Ok(trs)
    }

    /// Provides an iterator over the underlying queue
    pub fn queue(&self) -> impl Iterator<Item = (&A, impl Iterator<Item = (&K, &V)>)> {
        self.queue.iter().map(|(a, h)| (a, h.iter()))
    }

    /// Insert reputation packets with expiration
    pub fn gain<I>(&mut self, expiration: A, diff: I) -> Result<(), NonSortedAlpha<A>>
    where
        I: IntoIterator<Item = (K, V)>,
    {
        let zero = V::default();
        // Insert diff into queue
        match self.queue.back_mut() {
            Some((max_alpha, _)) if *max_alpha > expiration => {
                // Sorry, this data structure is designed to work with ordered inputs
                // In order to support unordered inserts, the queue would
                // need to be replaced with an ordered map.
                // Or we could just implement O(n) insertion using the queue
                Err(NonSortedAlpha {
                    alpha: expiration,
                    max_alpha: max_alpha.clone(),
                })
            }
            Some((max_alpha, back)) if *max_alpha == expiration => {
                // Insert reputation packets with the same expiration time as the most recent
                // packet: merge the two maps
                for (k, v) in diff.into_iter().filter(|(_k, v)| *v > zero) {
                    // Update identity cache
                    increment_cache(&mut self.map, k.clone(), v.clone());
                    // Merge with previous entry, or insert new
                    *back.entry(k).or_default() += v;
                }
                Ok(())
            }
            _ => {
                // Empty queue or last entry with alpha < expiration: insert new entry
                let diff: HashMap<K, V, S> = diff.into_iter().filter(|(_k, v)| *v > zero).fold(
                    HashMap::default(),
                    |mut back, (k, v)| {
                        // Update identity cache
                        increment_cache(&mut self.map, k.clone(), v.clone());
                        *back.entry(k).or_default() += v;
                        back
                    },
                );
                self.queue.push_back((expiration, diff));
                Ok(())
            }
        }
    }

    /// Expire all reputation packets older than `alpha`, return the total expired amount
    // This assumes that the queue is sorted by expiration
    pub fn expire(&mut self, alpha: &A) -> V {
        let mut total_expired = V::default();
        // We could compare alpha with self.queue.back(),
        // because expiring all the reputation packets is equivalent to self.clear()
        // but in practice that shouldn't happen very ofter
        while let Some((expiration, _)) = self.queue.front() {
            if expiration > alpha {
                // Done
                break;
            }

            let (_, front) = self.queue.pop_front().unwrap();
            // Update identity cache
            for (k, v) in front {
                // If the cache is consistent, this unwrap cannot fail
                decrement_cache(&mut self.map, k, v.clone()).unwrap();
                total_expired += v;
            }
        }

        total_expired
    }

    /// Penalize one identity. It is always preferred to use `penalize_many`, when possible.
    /// `next_v` is a function that given the total reputation of an identity,
    /// returns the reputation remaining after the penalization.
    pub fn penalize<F>(&mut self, id: &K, next_v: F) -> Result<V, RepError<V>>
    where
        F: FnMut(V) -> V,
    {
        self.penalize_many(iter::once((id, next_v)))
    }

    /// The more efficient version of `penalize`.
    pub fn penalize_many<'a, F, I>(&mut self, ids_fs: I) -> Result<V, RepError<V>>
    where
        F: FnMut(V) -> V,
        I: IntoIterator<Item = (&'a K, F)>,
        K: 'a,
    {
        let mut total_subtracted = V::default();
        let mut to_subtract = ids_fs
            .into_iter()
            .filter_map(|(id, mut next_v)| {
                let mut old_v = self.get(id);
                // next_v returns the new value of v
                let new_v = next_v(self.get(id));
                match new_v.cmp(&old_v) {
                    Ordering::Greater => {
                        // Overflow: return error
                        Some(Err(RepError {
                            old_rep: old_v,
                            new_rep: new_v,
                        }))
                    }
                    Ordering::Equal => {
                        // When there is no reputation to subtract, we can skip this identity
                        None
                    }
                    Ordering::Less => {
                        old_v -= new_v;
                        let ts = old_v;
                        total_subtracted += ts.clone();
                        // Update cache. Cannot fail because we just checked for overflow
                        decrement_cache(&mut self.map, id.clone(), ts.clone()).unwrap();
                        Some(Ok((id.clone(), ts)))
                    }
                }
            })
            .collect::<Result<HashMap<K, V, S>, _>>()?;

        // Iterate back to front
        for (_, rep_diff) in self.queue.iter_mut().rev() {
            Self::expire_packets(rep_diff, &mut to_subtract);
            // All the identities have been penalized, done
            if to_subtract.is_empty() {
                break;
            }
        }

        assert!(to_subtract.is_empty(), "Mismatch between cache and queue");

        Ok(total_subtracted)
    }

    fn expire_packets(rep_diff: &mut HashMap<K, V, S>, to_subtract: &mut HashMap<K, V, S>) {
        // Retain those identities which still have some reputation to lose.
        // Here we are essentially operating on the intersection of the two maps,
        // removing some elements which pertain to both maps.
        // Iterate over the map with fewer elements:
        if to_subtract.len() < rep_diff.len() {
            to_subtract.retain(|id, ts| {
                if let Entry::Occupied(mut x) = rep_diff.entry(id.clone()) {
                    let (retain_rep_diff, retain_ts) = Self::spend_coin(x.get_mut(), ts);
                    if !retain_rep_diff {
                        x.remove();
                    }
                    retain_ts
                } else {
                    // This identity has not gained any reputation packet in this alpha, retain
                    true
                }
            });
        } else {
            rep_diff.retain(|id, x| {
                if let Entry::Occupied(mut ts) = to_subtract.entry(id.clone()) {
                    let (retain_rep_diff, retain_ts) = Self::spend_coin(x, ts.get_mut());
                    if !retain_ts {
                        ts.remove();
                    }
                    retain_rep_diff
                } else {
                    // This identity does not need to be penalized, retain
                    true
                }
            });
        }
    }

    // Subtract `ts` from coin `x`. Returns (retain_x, retain_ts).
    // if x > ts, keep coin but remove `ts`
    // if x == ts, remove both
    // if x < ts, remove coin but keep `ts`
    fn spend_coin(x: &mut V, ts: &mut V) -> (bool, bool) {
        match (*x).cmp(ts) {
            Ordering::Greater => {
                // Mutate this coin, subtracting the required value
                *x -= ts.clone();
                // Keep this coin
                (true, false)
            }
            Ordering::Equal => (false, false),
            Ordering::Less => {
                // Remove this coin and decrease the remaining value to subtract
                *ts -= x.clone();
                // The coin was fully spent, discard
                (false, true)
            }
        }
    }

    /// Get the reputation for this identity.
    /// If the identity does not exist, return the starting reputation value.
    pub fn get(&self, id: &K) -> V {
        self.map.get(id).cloned().unwrap_or_default()
    }

    /// Get the sum of the reputation of many identities.
    /// If an identity does not exist, it counts as default reputation.
    pub fn get_sum<'a, I>(&'a self, ids: I) -> V
    where
        I: IntoIterator<Item = &'a K>,
    {
        ids.into_iter().fold(V::default(), |mut acc, id| {
            acc += self.get(id);
            acc
        })
    }

    /// Get the sum of the reputation of all the identities
    /// assuming that the default reputation value is zero.
    pub fn get_total_sum(&self) -> V {
        self.map.iter().fold(V::default(), |mut acc, (_id, v)| {
            acc += v.clone();
            acc
        })
    }

    /// Get the number of identities with non-null reputation
    pub fn num_identities(&self) -> usize {
        self.map.len()
    }

    /// Iterator over all the identities and their corresponding reputation
    pub fn identities(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.iter()
    }

    /// Clear the Trs
    pub fn clear(&mut self) {
        self.map.clear();
        self.queue.clear();
    }
}

/// Tried to decrement a cache entry when there is not enough to subtract.
#[derive(Copy, Clone, Debug)]
pub struct InconsistentCacheError;

/// Increment a cache entry
pub fn increment_cache<K, V, S>(map: &mut HashMap<K, V, S>, k: K, v: V)
where
    K: Eq + Hash,
    V: AddAssign + Default + PartialEq,
    S: BuildHasher,
{
    let zero = V::default();
    if v != zero {
        *map.entry(k).or_default() += v;
    }
}

/// Decrement a cache entry.
/// This function returns an error when there is not enough to subtract,
/// or the identity does not exist
pub fn decrement_cache<K, V, S>(
    map: &mut HashMap<K, V, S>,
    k: K,
    v: V,
) -> Result<(), InconsistentCacheError>
where
    K: Eq + Hash,
    V: Default + SubAssign + Ord,
    S: BuildHasher,
{
    let zero = V::default();
    if v == zero {
        // Decrementing zero always succeeds
        Ok(())
    } else if let Entry::Occupied(mut x) = map.entry(k) {
        match x.get().cmp(&v) {
            Ordering::Greater => {
                // Decrement entry
                *x.get_mut() -= v;
                Ok(())
            }
            Ordering::Equal => {
                // Back to the default value, remove entry from cache
                x.remove_entry();
                Ok(())
            }
            Ordering::Less => {
                // Error: not enough to subtract
                Err(InconsistentCacheError)
            }
        }
    } else {
        // Error: identity does not exist
        Err(InconsistentCacheError)
    }
}

impl<K, V, A, S> PartialEq for TotalReputationSet<K, V, A, S>
where
    K: Clone + Eq + Hash,
    V: AddAssign + Clone + Default + Ord + SubAssign,
    A: Clone + Ord,
    S: BuildHasher + Default,
{
    fn eq(&self, other: &Self) -> bool {
        // Equality is fully defined by equality of queues
        self.queue == other.queue
    }
}

impl<K, V, A, S> Default for TotalReputationSet<K, V, A, S>
where
    K: Clone + Eq + Hash,
    V: AddAssign + Clone + Default + Ord + SubAssign,
    A: Clone + Ord,
    S: BuildHasher + Default,
{
    fn default() -> Self {
        Self::with_hasher()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ActiveReputationSet;

    #[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct Reputation(u32);

    impl AddAssign for Reputation {
        fn add_assign(&mut self, rhs: Self) {
            self.0 += rhs.0
        }
    }

    impl SubAssign for Reputation {
        fn sub_assign(&mut self, rhs: Self) {
            self.0 -= rhs.0
        }
    }

    #[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct Alpha(u32);

    // Example demurrage functions used in tests:
    // Factor: lose half of the reputation for each lie
    // FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn fctr(num_lies: u32) -> impl Fn(Reputation) -> Reputation {
        const PENALIZATION_FACTOR: f64 = 0.5;
        move |Reputation(r)| {
            Reputation((f64::from(r) * PENALIZATION_FACTOR.powf(f64::from(num_lies))) as u32)
        }
    }
    // Constant: lose a fixed amount of reputation (stopping at 0)
    fn cnst(x: u32) -> impl Fn(Reputation) -> Reputation {
        move |Reputation(r)| Reputation(if r > x { r - x } else { 0 })
    }

    #[test]
    fn insert_id_twice() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        let diff = Reputation(40);
        let expiration = Alpha(10);
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(expiration, vec![(id1.clone(), diff)]).unwrap();
        a.gain(expiration, vec![(id1.clone(), diff)]).unwrap();
        assert_eq!(a.get(&id1), Reputation(80));
        a.gain(expiration, vec![(id1.clone(), diff), (id1.clone(), diff)])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(160));
        assert_eq!(a.expire(&Alpha(10)), Reputation(160));
        assert_eq!(a.get(&id1), Reputation::default());
    }

    #[test]
    fn insert_id_different_alpha() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(Alpha(10), vec![(id1.clone(), Reputation(50))])
            .unwrap();
        a.gain(Alpha(11), vec![(id1.clone(), Reputation(30))])
            .unwrap();
        a.gain(Alpha(12), vec![(id1.clone(), Reputation(15))])
            .unwrap();
        a.gain(Alpha(13), vec![(id1.clone(), Reputation(70))])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(165));
        assert_eq!(a.expire(&Alpha(9)), Reputation(0));
        assert_eq!(a.get(&id1), Reputation(165));
        assert_eq!(a.expire(&Alpha(10)), Reputation(50));
        assert_eq!(a.get(&id1), Reputation(115));
        assert_eq!(a.expire(&Alpha(11)), Reputation(30));
        assert_eq!(a.get(&id1), Reputation(85));
        assert_eq!(a.expire(&Alpha(12)), Reputation(15));
        assert_eq!(a.get(&id1), Reputation(70));
        assert_eq!(a.expire(&Alpha(13)), Reputation(70));
        assert_eq!(a.get(&id1), Reputation(0));
    }

    #[test]
    fn insert_zero_coins() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(Alpha(10), vec![(id1.clone(), Reputation(0))])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(0));
        assert_eq!(a.num_identities(), 0);
        a.gain(
            Alpha(11),
            vec![(id1.clone(), Reputation(0)), (id1.clone(), Reputation(0))],
        )
        .unwrap();
        assert_eq!(a.get(&id1), Reputation(0));
        assert_eq!(a.num_identities(), 0);
        a.gain(Alpha(11), vec![(id1.clone(), Reputation(0))])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(0));
        assert_eq!(a.num_identities(), 0);
        a.gain(
            Alpha(11),
            vec![(id1.clone(), Reputation(0)), (id1.clone(), Reputation(0))],
        )
        .unwrap();
        assert_eq!(a.get(&id1), Reputation(0));
        assert_eq!(a.num_identities(), 0);
    }

    #[test]
    fn insert_unsorted() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        let diff = Reputation(40);
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(Alpha(10), vec![(id1.clone(), diff)]).unwrap();
        assert_eq!(
            a.gain(Alpha(9), vec![(id1, diff)]),
            Err(NonSortedAlpha {
                alpha: Alpha(9),
                max_alpha: Alpha(10),
            })
        );
    }

    #[test]
    fn expire_off_by_one() {
        // When expiration is 10, a.expire(9) should not expire that
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        let diff = Reputation(40);
        let expiration = Alpha(10);
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(expiration, vec![(id1.clone(), diff)]).unwrap();
        assert_eq!(a.get(&id1), diff);
        assert_eq!(a.expire(&Alpha(0)), Reputation(0));
        assert_eq!(a.get(&id1), diff);
        assert_eq!(a.expire(&Alpha(9)), Reputation(0));
        assert_eq!(a.get(&id1), diff);
        assert_eq!(a.expire(&Alpha(10)), Reputation(40));
        assert_eq!(a.get(&id1), Reputation::default());
    }

    #[test]
    fn expire_after_1000() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        let diff = Reputation(40);
        let expiration = Alpha(10);
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(expiration, vec![(id1.clone(), diff)]).unwrap();
        assert_eq!(a.expire(&Alpha(1000)), Reputation(40));
        assert_eq!(a.get(&id1), Reputation::default());
    }

    #[test]
    fn expire_all() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        let diff = Reputation(40);
        let expiration = Alpha(10);
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(expiration, vec![(id1.clone(), diff)]).unwrap();
        a.clear();
        assert_eq!(a.get(&id1), Reputation::default());
    }

    #[test]
    fn penalize_simple() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(Alpha(10), vec![(id1.clone(), Reputation(50))])
            .unwrap();
        a.gain(Alpha(11), vec![(id1.clone(), Reputation(30))])
            .unwrap();
        a.gain(Alpha(12), vec![(id1.clone(), Reputation(15))])
            .unwrap();
        a.gain(Alpha(13), vec![(id1.clone(), Reputation(70))])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(165));
        assert_eq!(a.penalize(&id1, cnst(5)), Ok(Reputation(5)));
        assert_eq!(a.get(&id1), Reputation(160));
        // Check that the reputation was removed from the most recent "coin"
        assert_eq!(a.queue.back().unwrap().1[&id1], Reputation(70 - 5));
        // Check that a null penalization does nothing
        assert_eq!(a.penalize(&id1, cnst(0)), Ok(Reputation(0)));
        assert_eq!(a.get(&id1), Reputation(160));
    }

    #[test]
    fn penalize_simple_exact() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(Alpha(13), vec![(id1.clone(), Reputation(70))])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(70));
        assert_eq!(a.penalize(&id1, cnst(70)), Ok(Reputation(70)));
        assert_eq!(a.get(&id1), Reputation(0));
        // Check that the reputation was removed from the most recent "coin"
        assert!(!a.queue.back().unwrap().1.contains_key(&id1));
    }

    #[test]
    fn penalize_two_coins() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(Alpha(10), vec![(id1.clone(), Reputation(50))])
            .unwrap();
        a.gain(Alpha(11), vec![(id1.clone(), Reputation(30))])
            .unwrap();
        a.gain(Alpha(12), vec![(id1.clone(), Reputation(15))])
            .unwrap();
        a.gain(Alpha(13), vec![(id1.clone(), Reputation(70))])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(165));
        assert_eq!(a.penalize(&id1, cnst(80)), Ok(Reputation(80)));
        assert_eq!(a.get(&id1), Reputation(85));
        // Check that the reputation was removed from the most recent "coin"
        assert!(!a.queue.back().unwrap().1.contains_key(&id1));
        // And the second most recent coin was mutated:
        assert_eq!(
            a.queue.iter().rev().nth(1).unwrap().1[&id1],
            Reputation(15 - 10)
        );
    }

    #[test]
    fn penalize_all_coins() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(Alpha(10), vec![(id1.clone(), Reputation(50))])
            .unwrap();
        a.gain(Alpha(11), vec![(id1.clone(), Reputation(30))])
            .unwrap();
        a.gain(Alpha(12), vec![(id1.clone(), Reputation(15))])
            .unwrap();
        a.gain(Alpha(13), vec![(id1.clone(), Reputation(70))])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(165));
        assert_eq!(a.penalize(&id1, cnst(164)), Ok(Reputation(164)));
        assert_eq!(a.get(&id1), Reputation(1));
        // We only have 1 reputation from the first coin
        assert_eq!(a.queue.front().unwrap().1[&id1], Reputation(1));
    }

    #[test]
    fn penalize_and_expire() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(Alpha(10), vec![(id1.clone(), Reputation(50))])
            .unwrap();
        a.gain(Alpha(11), vec![(id1.clone(), Reputation(30))])
            .unwrap();
        a.gain(Alpha(12), vec![(id1.clone(), Reputation(15))])
            .unwrap();
        a.gain(Alpha(13), vec![(id1.clone(), Reputation(70))])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(165));
        assert_eq!(a.penalize(&id1, cnst(10)), Ok(Reputation(10)));
        assert_eq!(a.expire(&Alpha(9)), Reputation(0));
        assert_eq!(a.get(&id1), Reputation(155));
        assert_eq!(a.penalize(&id1, cnst(10)), Ok(Reputation(10)));
        assert_eq!(a.expire(&Alpha(10)), Reputation(50));
        assert_eq!(a.get(&id1), Reputation(95));
        assert_eq!(a.penalize(&id1, cnst(10)), Ok(Reputation(10)));
        assert_eq!(a.expire(&Alpha(11)), Reputation(30));
        assert_eq!(a.get(&id1), Reputation(55));
        assert_eq!(a.penalize(&id1, cnst(10)), Ok(Reputation(10)));
        assert_eq!(a.expire(&Alpha(12)), Reputation(15));
        assert_eq!(a.get(&id1), Reputation(30));
    }

    #[test]
    fn penalize_overflow() {
        // This tests for negative penalizations: an identity has 50 reputation
        // and after penalization it will have 1000.
        // This is impossible, so the penalize function returns an error.
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(Alpha(10), vec![(id1.clone(), Reputation(50))])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(50));
        assert_eq!(
            a.penalize(&id1, |_| Reputation(1000)),
            Err(RepError {
                old_rep: Reputation(50),
                new_rep: Reputation(1000),
            })
        );
    }

    #[test]
    // FIXME(#676): Remove clippy skip error
    #[allow(clippy::cognitive_complexity)]
    fn multiple_identities() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        let id2 = "Bob".to_string();
        let id3 = "Eve".to_string();
        // Helper function to be used with assert_eq
        let reps = |a: &TotalReputationSet<String, Reputation, Alpha, _>| {
            (a.get(&id1).0, a.get(&id2).0, a.get(&id3).0)
        };
        let v4 = vec![
            (id1.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
            (id3.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
        ];
        let v5 = vec![
            (id1.clone(), Reputation(1024)),
            (id3.clone(), Reputation(1024)),
        ];
        let v6 = vec![
            (id1.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
        ];
        let v7 = vec![
            (id1.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
            (id3.clone(), Reputation(1024)),
        ];
        let v8 = vec![
            (id1.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
            (id3.clone(), Reputation(1024)),
        ];
        let v9 = vec![
            (id1.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
            (id3.clone(), Reputation(1024)),
        ];
        let v10 = vec![];
        let v11 = vec![];
        let v12 = vec![];
        let v13 = vec![
            (id1.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
            (id3.clone(), Reputation(1024)),
        ];
        assert_eq!(reps(&a), (0, 0, 0));
        a.expire(&Alpha(0));
        a.gain(Alpha(4), v4).unwrap();
        assert_eq!(reps(&a), (1024, 2048, 1024));
        let p = a.penalize_many(vec![(&id1, fctr(1)), (&id2, fctr(1)), (&id3, fctr(1))]);
        assert_eq!(p, Ok(Reputation(512 + 1024 + 512)));
        assert_eq!(reps(&a), (512, 1024, 512));
        a.expire(&Alpha(1));
        assert_eq!(reps(&a), (512, 1024, 512));
        a.gain(Alpha(5), v5).unwrap();
        assert_eq!(reps(&a), (1536, 1024, 1536));
        let p = a.penalize_many(vec![(&id1, fctr(1)), (&id2, fctr(1)), (&id3, fctr(1))]);
        assert_eq!(p, Ok(Reputation(768 + 512 + 768)));
        assert_eq!(reps(&a), (768, 512, 768));
        a.expire(&Alpha(2));
        assert_eq!(reps(&a), (768, 512, 768));
        a.gain(Alpha(6), v6).unwrap();
        assert_eq!(reps(&a), (1792, 1536, 768));
        let p = a.penalize_many(vec![(&id1, fctr(1)), (&id2, fctr(1)), (&id3, fctr(1))]);
        assert_eq!(p, Ok(Reputation(896 + 768 + 384)));
        assert_eq!(reps(&a), (896, 768, 384));
        a.expire(&Alpha(3));
        assert_eq!(reps(&a), (896, 768, 384));
        a.gain(Alpha(7), v7).unwrap();
        assert_eq!(reps(&a), (1920, 1792, 1408));
        let p = a.penalize_many(vec![(&id1, fctr(1)), (&id2, fctr(1)), (&id3, fctr(1))]);
        assert_eq!(p, Ok(Reputation(960 + 896 + 704)));
        assert_eq!(reps(&a), (960, 896, 704));
        a.expire(&Alpha(4));
        assert_eq!(reps(&a), (448, 384, 320));
        a.gain(Alpha(8), v8).unwrap();
        assert_eq!(reps(&a), (1472, 1408, 1344));
        let p = a.penalize_many(vec![(&id1, fctr(1)), (&id2, fctr(1)), (&id3, fctr(1))]);
        assert_eq!(p, Ok(Reputation(736 + 704 + 672)));
        assert_eq!(reps(&a), (736, 704, 672));
        a.expire(&Alpha(5));
        assert_eq!(reps(&a), (480, 704, 672));
        a.gain(Alpha(9), v9).unwrap();
        assert_eq!(reps(&a), (1504, 1728, 1696));
        let p = a.penalize_many(vec![(&id1, fctr(1)), (&id2, fctr(1)), (&id3, fctr(1))]);
        assert_eq!(p, Ok(Reputation(752 + 864 + 848)));
        assert_eq!(reps(&a), (752, 864, 848));
        a.expire(&Alpha(6));
        assert_eq!(reps(&a), (624, 608, 848));
        a.gain(Alpha(10), v10).unwrap();
        assert_eq!(reps(&a), (624, 608, 848));
        let p = a.penalize_many(vec![(&id1, fctr(1)), (&id2, fctr(1)), (&id3, fctr(1))]);
        assert_eq!(p, Ok(Reputation(312 + 304 + 424)));
        assert_eq!(reps(&a), (312, 304, 424));
        a.expire(&Alpha(7));
        assert_eq!(reps(&a), (248, 176, 104));
        a.gain(Alpha(11), v11).unwrap();
        assert_eq!(reps(&a), (248, 176, 104));
        let p = a.penalize_many(vec![(&id1, fctr(1)), (&id2, fctr(1)), (&id3, fctr(1))]);
        assert_eq!(p, Ok(Reputation(124 + 88 + 52)));
        assert_eq!(reps(&a), (124, 88, 52));
        a.expire(&Alpha(8));
        assert_eq!(reps(&a), (0, 0, 0));
        a.gain(Alpha(12), v12).unwrap();
        assert_eq!(reps(&a), (0, 0, 0));
        assert_eq!(reps(&a), (0, 0, 0));
        let p = a.penalize_many(vec![(&id1, fctr(1)), (&id2, fctr(1)), (&id3, fctr(1))]);
        assert_eq!(p, Ok(Reputation(0)));
        assert_eq!(reps(&a), (0, 0, 0));
        a.expire(&Alpha(9));
        assert_eq!(reps(&a), (0, 0, 0));
        a.gain(Alpha(13), v13).unwrap();
        assert_eq!(reps(&a), (1024, 1024, 1024));
        a.expire(&Alpha(100));
        assert_eq!(reps(&a), (0, 0, 0));
    }

    #[test]
    fn queue_from_queue() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        assert_eq!(a.get(&id1), Reputation::default());
        a.gain(Alpha(10), vec![(id1.clone(), Reputation(50))])
            .unwrap();
        a.gain(Alpha(11), vec![(id1.clone(), Reputation(30))])
            .unwrap();
        a.gain(Alpha(12), vec![(id1.clone(), Reputation(15))])
            .unwrap();
        a.gain(Alpha(13), vec![(id1.clone(), Reputation(70))])
            .unwrap();
        assert_eq!(a.get(&id1), Reputation(165));

        let b = TotalReputationSet::from_queue(
            a.queue()
                .map(|(a, i2)| (*a, i2.map(|(k, v)| (k.clone(), *v)))),
        )
        .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.map, b.map);
    }

    #[test]
    fn rep_sum() {
        let mut a = TotalReputationSet::new();
        let id1 = "Alice".to_string();
        let id2 = "Bob".to_string();
        let id3 = "Eve".to_string();
        let v4 = vec![
            (id1.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
            (id3.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
        ];
        assert_eq!(a.get_total_sum(), Reputation(0));
        assert_eq!(a.num_identities(), 0);
        a.gain(Alpha(4), v4).unwrap();
        assert_eq!(a.get_total_sum(), Reputation(4096));
        assert_eq!(a.get_sum(vec![&id1, &id2, &id3]), Reputation(4096));
        assert_eq!(a.get_sum(vec![&id1]), Reputation(1024));
        assert_eq!(a.num_identities(), 3);
    }

    #[test]
    // FIXME(#676): Remove clippy skip error
    #[allow(clippy::cognitive_complexity)]
    fn active_rep_sum() {
        let mut trs = TotalReputationSet::new();
        let mut ars = ActiveReputationSet::new(2);

        let id1 = "Alice".to_string();
        let id2 = "Bob".to_string();
        let id3 = "Charlie".to_string();
        let v4 = vec![
            (id1.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
            (id3.clone(), Reputation(1024)),
            (id2.clone(), Reputation(1024)),
        ];
        assert_eq!(trs.get_total_sum(), Reputation(0));
        assert_eq!(trs.num_identities(), 0);
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));

        trs.gain(Alpha(4), v4).unwrap();
        assert_eq!(trs.get_total_sum(), Reputation(4096));
        assert_eq!(trs.get_sum(vec![&id1, &id2, &id3]), Reputation(4096));
        assert_eq!(trs.get_sum(vec![&id1]), Reputation(1024));
        assert_eq!(trs.num_identities(), 3);

        ars.push_activity(vec![id1.clone(), id2.clone(), id3.clone()]);
        assert!(ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(ars.contains(&id3));
        assert_eq!(ars.active_identities_number(), 3);
        assert_eq!(trs.get_sum(ars.active_identities()), Reputation(4096));

        ars.push_activity(vec![id2.clone(), id3.clone()]);
        ars.push_activity(vec![id2.clone(), id3.clone()]);
        assert!(!ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(ars.contains(&id3));
        assert_eq!(ars.active_identities_number(), 2);
        assert_eq!(trs.get_sum(ars.active_identities()), Reputation(3072));

        ars.push_activity(vec![id3.clone()]);
        ars.push_activity(vec![id3.clone()]);
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(ars.contains(&id3));
        assert_eq!(ars.active_identities_number(), 1);
        assert_eq!(trs.get_sum(ars.active_identities()), Reputation(1024));
    }
}
