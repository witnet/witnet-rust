//! Active Reputation Set

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::ReputationError;
use crate::trs::{decrement_cache, increment_cache};
use std::{
    collections::{HashMap, HashSet, VecDeque},
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
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Debug, Clone)]
pub struct ActiveReputationSet<K>
where
    K: Clone + Eq + Hash,
{
    // A cache of <identity: activity>
    // All the identities with activity are in the cache
    #[cfg_attr(
        feature = "serde",
        serde(bound(
            serialize = "HashMap<K, u16>: Serialize",
            deserialize = "HashMap<K, u16>: Deserialize<'de>"
        ))
    )]
    map: HashMap<K, u16>,
    // The list of active identities ordered by time
    #[cfg_attr(
        feature = "serde",
        serde(bound(
            serialize = "VecDeque<HashSet<K>>: Serialize",
            deserialize = "VecDeque<HashSet<K>>: Deserialize<'de>"
        ))
    )]
    queue: VecDeque<HashSet<K>>,
    // Capacity
    capacity: usize,
    // Last update (last computed block epoch)
    last_update: u32,
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
            queue: VecDeque::with_capacity(capacity),
            capacity,
            last_update: 0,
        }
    }

    /// Clear method for `ActiveReputationSet<K>`
    pub fn clear(&mut self) {
        self.map.clear();
        self.queue.clear();
    }

    /// Gets the capacity of the buffer: the number of insertions
    /// that it takes to start dropping old entries.
    pub fn buffer_capacity(&self) -> usize {
        self.capacity
    }

    /// Gets the size of the `ActiveReputationSet<K>`
    pub fn buffer_size(&self) -> usize {
        self.queue.len()
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
        let identities: HashSet<K> = identities.into_iter().collect();

        if self.queue.len() >= self.capacity {
            self.queue.pop_front().unwrap().into_iter().for_each(|id| {
                // If the cache is consistent, this unwrap cannot fail
                decrement_cache(&mut self.map, id, 1).unwrap();
            });
        }

        // Update new identities added to the queue
        identities.iter().for_each(|id| {
            increment_cache(&mut self.map, id.clone(), 1);
        });

        self.queue.push_back(identities);
    }

    /// Method to add a new entry taking into account the proposed time
    pub fn update<M>(&mut self, identities: M, block_epoch: u32) -> Result<(), failure::Error>
    where
        M: IntoIterator<Item = K>,
    {
        if block_epoch > self.last_update {
            // In order that activity period correspond to epoch instead of blocks
            // empty vectors has to be added to the ARS when there are holes between blocks
            let difference = (block_epoch - self.last_update) as usize;
            if difference >= self.capacity {
                self.clear();
            } else if difference > 1 {
                for _i in 0..(difference - 1) {
                    self.push_activity(vec![]);
                }
            }

            self.push_activity(identities);
            self.last_update = block_epoch;

            Ok(())
        } else if block_epoch == 0 {
            // In the epoch 0 (genesis block) there must be no reputation update

            Ok(())
        } else {
            Err(ReputationError::InvalidUpdateTime {
                new_time: block_epoch,
                current_time: self.last_update,
            }
            .into())
        }
    }
}

impl<K> PartialEq for ActiveReputationSet<K>
where
    K: Clone + Eq + Hash,
{
    fn eq(&self, other: &Self) -> bool {
        // Equality is fully defined by equality of queues
        self.capacity == other.capacity && self.queue == other.queue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_id() {
        let mut ars = ActiveReputationSet::new(2);
        let id1 = "Alice".to_string();
        assert!(!ars.contains(&id1));

        ars.push_activity(vec![id1.clone()]);
        assert!(ars.contains(&id1));
        assert_eq!(ars.map[&id1], 1);
        assert_eq!(ars.active_identities_number(), 1);
    }

    #[test]
    fn insert_id_twice_in_same_time() {
        let mut ars = ActiveReputationSet::new(2);
        let id1 = "Alice".to_string();
        assert!(!ars.contains(&id1));

        ars.push_activity(vec![id1.clone(), id1.clone()]);
        assert!(ars.contains(&id1));
        assert_eq!(ars.map[&id1], 1);
        assert_eq!(ars.active_identities_number(), 1);
    }

    #[test]
    fn insert_id_twice() {
        let mut ars = ActiveReputationSet::new(2);
        let id1 = "Alice".to_string();
        assert!(!ars.contains(&id1));

        ars.push_activity(vec![id1.clone()]);
        assert!(ars.contains(&id1));
        assert_eq!(ars.map[&id1], 1);
        assert_eq!(ars.active_identities_number(), 1);

        ars.push_activity(vec![id1.clone()]);
        assert!(ars.contains(&id1));
        assert_eq!(ars.map[&id1], 2);
        assert_eq!(ars.active_identities_number(), 1);
    }

    #[test]
    fn expire_id() {
        let mut ars = ActiveReputationSet::new(2);
        let id1 = "Alice".to_string();
        let id2 = "Bob".to_string();
        let id3 = "Charlie".to_string();
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));

        ars.push_activity(vec![id1.clone()]);
        assert!(ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.map[&id1], 1);
        assert_eq!(ars.active_identities_number(), 1);

        ars.push_activity(vec![id2.clone()]);
        assert!(ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.map[&id2], 1);
        assert_eq!(ars.active_identities_number(), 2);

        ars.push_activity(vec![id3.clone()]);
        assert!(!ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(ars.contains(&id3));
        assert_eq!(ars.map[&id3], 1);
        assert_eq!(ars.active_identities_number(), 2);
    }

    #[test]
    fn update_ars_test() {
        let mut ars = ActiveReputationSet::new(3);
        let id1 = "Alice".to_string();
        let id2 = "Bob".to_string();
        let id3 = "Charlie".to_string();
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 0);
        assert_eq!(ars.active_identities_number(), 0);

        let _res = ars.update(vec![id1.clone()], 1);
        assert!(ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 1);
        assert_eq!(ars.active_identities_number(), 1);

        let _res = ars.update(vec![id2.clone()], 2);
        assert!(ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 2);
        assert_eq!(ars.active_identities_number(), 2);

        let _res = ars.update(vec![id2.clone()], 3);
        assert!(ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 3);
        assert_eq!(ars.active_identities_number(), 2);

        let _res = ars.update(vec![id3.clone()], 4);
        assert!(!ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(ars.contains(&id3));
        assert_eq!(ars.last_update, 4);
        assert_eq!(ars.active_identities_number(), 2);
    }

    #[test]
    fn update_ars_test_empty_epochs() {
        let mut ars = ActiveReputationSet::new(3);
        let id1 = "Alice".to_string();
        let id2 = "Bob".to_string();
        let id3 = "Charlie".to_string();
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 0);
        assert_eq!(ars.active_identities_number(), 0);

        let _res = ars.update(vec![id1.clone()], 1);
        assert!(ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 1);
        assert_eq!(ars.active_identities_number(), 1);

        let _res = ars.update(vec![id2.clone()], 2);
        assert!(ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 2);
        assert_eq!(ars.active_identities_number(), 2);

        let _res = ars.update(vec![id3.clone()], 10);
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(ars.contains(&id3));
        assert_eq!(ars.last_update, 10);
        assert_eq!(ars.active_identities_number(), 1);

        let _res = ars.update(vec![id3.clone()], 20);
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(ars.contains(&id3));
        assert_eq!(ars.last_update, 20);
        assert_eq!(ars.active_identities_number(), 1);
    }

    #[test]
    fn update_ars_test_position() {
        let mut ars = ActiveReputationSet::new(3);
        let id1 = "Alice".to_string();
        let id2 = "Bob".to_string();
        let id3 = "Charlie".to_string();
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 0);
        assert_eq!(ars.active_identities_number(), 0);

        let _res = ars.update(vec![id1.clone()], 1);
        assert!(ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 1);
        assert_eq!(ars.active_identities_number(), 1);

        let _res = ars.update(vec![id2.clone()], 10);
        assert!(!ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 10);
        assert_eq!(ars.active_identities_number(), 1);

        let _res = ars.update(vec![id3.clone()], 11);
        let _res = ars.update(vec![id3.clone()], 12);
        assert_eq!(ars.active_identities_number(), 2);
        let _res = ars.update(vec![id3.clone()], 13);
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(ars.contains(&id3));
        assert_eq!(ars.last_update, 13);
        assert_eq!(ars.active_identities_number(), 1);
    }

    #[test]
    fn update_ars_test_error() {
        let mut ars = ActiveReputationSet::new(3);
        let id1 = "Alice".to_string();
        let id2 = "Bob".to_string();
        let id3 = "Charlie".to_string();
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 0);
        assert_eq!(ars.active_identities_number(), 0);

        let _res = ars.update(vec![id1.clone()], 10);
        assert!(ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 10);
        assert_eq!(ars.active_identities_number(), 1);

        let error = ars.update(vec![id2.clone()], 10).unwrap_err();
        assert_eq!(
            error.to_string(),
            ReputationError::InvalidUpdateTime {
                new_time: 10,
                current_time: 10,
            }
            .to_string()
        );
        assert_eq!(ars.last_update, 10);
        assert_eq!(ars.active_identities_number(), 1);

        let error2 = ars.update(vec![id2], 5).unwrap_err();
        assert_eq!(
            error2.to_string(),
            ReputationError::InvalidUpdateTime {
                new_time: 5,
                current_time: 10,
            }
            .to_string()
        );
        assert_eq!(ars.last_update, 10);
        assert_eq!(ars.active_identities_number(), 1);
    }

    #[test]
    // FIXME(#676): Remove clippy skip error
    #[allow(clippy::cognitive_complexity)]
    fn update_ars_test_never_empty() {
        let mut ars = ActiveReputationSet::new(3);
        let id1 = "Alice".to_string();
        let id2 = "Bob".to_string();
        let id3 = "Charlie".to_string();
        assert!(!ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 0);
        assert_eq!(ars.active_identities_number(), 0);

        let _res = ars.update(vec![id1.clone()], 10);
        assert!(ars.contains(&id1));
        assert!(!ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 10);
        assert_eq!(ars.active_identities_number(), 1);

        let _res = ars.update(vec![id2.clone()], 11);
        assert!(ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 11);
        assert_eq!(ars.active_identities_number(), 2);

        let _res = ars.update(vec![id2.clone()], 21);
        let _res = ars.update(vec![id2.clone()], 23);
        let _res = ars.update(vec![id2.clone()], 24);
        assert!(!ars.contains(&id1));
        assert!(ars.contains(&id2));
        assert!(!ars.contains(&id3));
        assert_eq!(ars.last_update, 24);
        assert_eq!(ars.active_identities_number(), 1);
    }
}
