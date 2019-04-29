//! Active Reputation Set

use queues::{CircularBuffer, IsQueue};

use crate::trs::{decrement_cache, increment_cache};
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

/// Active Reputation Set
///
/// This data structure keeps track of every identity `K` in a buffer of
/// time. In order to keep track the identities contained in the buffer,
/// these are stored in a circular queue (FIFO).
///
/// The method `push_activity` insert a vector of identities in the structure
/// and also update with the identities that are expired.

#[derive(Debug)]
pub struct ActiveReputationSet<K>
where
    K: Clone + Eq + Hash,
{
    // A cache of <identity: activity>
    // All the identities with activity are in the cache
    map: HashMap<K, u16>,
    // The list of active identities ordered by time
    queue: CircularBuffer<HashSet<K>>,
}

impl<K> ActiveReputationSet<K>
where
    K: Clone + Eq + Hash,
{
    /// Default `ActiveReputationSet<K>` initializer
    ///
    /// # Returns
    /// A new, empty `ActiveReputationSet<K>`
    ///
    /// # Examples
    ///
    /// ```
    /// # use witnet_reputation::ActiveReputationSet;
    /// let ars: ActiveReputationSet<isize> = ActiveReputationSet::new(3);
    /// assert_eq!(ars.buffer_size(), 0);
    /// assert_eq!(ars.buffer_capacity(), 3);
    /// ```
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            queue: CircularBuffer::new(capacity),
        }
    }

    /// Gets the capacity of the buffer: the number of insertions
    /// that it takes to start dropping old entries.
    pub fn buffer_capacity(&self) -> usize {
        self.queue.capacity()
    }

    /// Gets the size of the `ActiveReputationSet<K>`
    pub fn buffer_size(&self) -> usize {
        self.queue.size()
    }

    /// Contains method for `ActiveReputationSet<K>`
    pub fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// Returns an iterator of active identities
    pub fn active_identities(&self) -> impl Iterator<Item = &K> {
        self.map.keys()
    }

    /// Returns the number of active identities
    pub fn active_identities_number(&self) -> usize {
        self.map.len()
    }

    /// Method to add a new entry. If the buffer is full, the oldest entry
    /// will be dropped, and the identity cache will be accordingly updated.
    pub fn push_activity<M>(&mut self, identities: M)
    where
        M: IntoIterator<Item = K>,
    {
        // Update new identities added to the queue
        let identities: HashSet<K> = identities.into_iter().collect();
        identities.iter().for_each(|id| {
            increment_cache(&mut self.map, id.clone(), 1);
        });

        if let Some(identities_with_expired_reputation) = self
            .queue
            .add(identities)
            // This operation can not fail (add method only returns ok)
            .unwrap()
        {
            identities_with_expired_reputation
                .into_iter()
                .for_each(|id| {
                    // If the cache is consistent, this unwrap cannot fail
                    decrement_cache(&mut self.map, id, 1).unwrap();
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_id() {
        let mut ars = ActiveReputationSet::new(2);
        let id1 = "Alice".to_string();
        assert_eq!(ars.contains(&id1), false);

        ars.push_activity(vec![id1.clone()]);
        assert_eq!(ars.contains(&id1), true);
        assert_eq!(ars.map[&id1], 1);
        assert_eq!(ars.active_identities_number(), 1);
    }

    #[test]
    fn insert_id_twice_in_same_time() {
        let mut ars = ActiveReputationSet::new(2);
        let id1 = "Alice".to_string();
        assert_eq!(ars.contains(&id1), false);

        ars.push_activity(vec![id1.clone(), id1.clone()]);
        assert_eq!(ars.contains(&id1), true);
        assert_eq!(ars.map[&id1], 1);
        assert_eq!(ars.active_identities_number(), 1);
    }

    #[test]
    fn insert_id_twice() {
        let mut ars = ActiveReputationSet::new(2);
        let id1 = "Alice".to_string();
        assert_eq!(ars.contains(&id1), false);

        ars.push_activity(vec![id1.clone()]);
        assert_eq!(ars.contains(&id1), true);
        assert_eq!(ars.map[&id1], 1);
        assert_eq!(ars.active_identities_number(), 1);

        ars.push_activity(vec![id1.clone()]);
        assert_eq!(ars.contains(&id1), true);
        assert_eq!(ars.map[&id1], 2);
        assert_eq!(ars.active_identities_number(), 1);
    }

    #[test]
    fn expire_id() {
        let mut ars = ActiveReputationSet::new(2);
        let id1 = "Alice".to_string();
        let id2 = "Bob".to_string();
        let id3 = "Charlie".to_string();
        assert_eq!(ars.contains(&id1), false);
        assert_eq!(ars.contains(&id2), false);
        assert_eq!(ars.contains(&id3), false);

        ars.push_activity(vec![id1.clone()]);
        assert_eq!(ars.contains(&id1), true);
        assert_eq!(ars.contains(&id2), false);
        assert_eq!(ars.contains(&id3), false);
        assert_eq!(ars.map[&id1], 1);
        assert_eq!(ars.active_identities_number(), 1);

        ars.push_activity(vec![id2.clone()]);
        assert_eq!(ars.contains(&id1), true);
        assert_eq!(ars.contains(&id2), true);
        assert_eq!(ars.contains(&id3), false);
        assert_eq!(ars.map[&id2], 1);
        assert_eq!(ars.active_identities_number(), 2);

        ars.push_activity(vec![id3.clone()]);
        assert_eq!(ars.contains(&id1), false);
        assert_eq!(ars.contains(&id2), true);
        assert_eq!(ars.contains(&id3), true);
        assert_eq!(ars.map[&id3], 1);
        assert_eq!(ars.active_identities_number(), 2);
    }
}
