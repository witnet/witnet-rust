use std::{cmp, fmt};

use circular_queue::CircularQueue;
use itertools::Itertools;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// Assuming no missing epochs, this will keep track of priority used by transactions in the last 12
// hours (960 epochs).
const DEFAULT_QUEUE_CAPACITY_EPOCHS: usize = 960;
// The minimum number of epochs that we need to track before estimating transaction priority
const MINIMUM_TRACKED_EPOCHS: usize = 10;

/// Keeps track of fees being paid by transactions included in recent blocks, and provides methods
/// for estimating sensible priority values for future transactions.
///
/// This supports _value transfer transactions_ (VTTs) as well as _data requests_ (DRs).
///
/// All across this module, fees are always expressed in their relative form (nanowits per weight
/// unit), aka "transaction priority".
#[derive(Clone, Eq, PartialEq)]
pub struct PriorityEngine {
    /// Queue for storing fees info for recent transactions.
    priorities: CircularQueue<Priorities>,
}

impl PriorityEngine {
    /// Retrieve the inner fee information as a vector.
    pub fn as_vec(&self) -> Vec<Priorities> {
        self.priorities.iter().rev().cloned().collect_vec()
    }

    /// Provide suggestions for sensible transaction priority values, together with their expected
    /// time-to-block in epochs.
    ///
    /// This is only a first approach to an estimation algorithm. There is abundant prior art about
    /// fee estimation in other blockchains. We might revisit this once we collect more insights
    /// about our fees market and user feedback.
    ///
    /// The default values used here assume that estimation operates with picoWit (10 ^ -12).
    /// That is, from a user perspective, all priority values shown here have 3 implicit decimal
    /// digits. They need to be divided by 1,000 for the real protocol-wide nanoWit value, and by
    /// 1,000,000,000,000 for the Wit value. This allows for more fine-grained estimations while the
    /// market for block space is idle.
    pub fn estimate_priority(&self) -> Option<PrioritiesEstimate> {
        // Short-circuit if there are too few tracked epochs for an accurate estimation.
        let len = self.priorities.len();
        if len < MINIMUM_TRACKED_EPOCHS {
            return None;
        }

        // Find out the queue capacity. We can only provide estimates up to this number of epochs.
        let capacity = self.priorities.capacity();
        // Will keep track of the absolute minimum and maximum priorities found in the engine.
        let mut absolutes = Priorities::default();
        // Initialize accumulators for different priorities.
        let mut drt_slow = 0u64;
        let mut drt_medium = 0u64;
        let mut drt_fast = 0u64;
        let mut vtt_slow = 0u64;
        let mut vtt_medium = 0u64;
        let mut vtt_fast = 0u64;
        // To be used later as the divisors in an age weighted arithmetic means.
        // These are initialized to 1 to avoid division by zero issues.
        let mut drt_divisor = 1u64;
        let mut vtt_divisor = 1u64;

        let mut age = len as u64;
        for Priorities {
            drt_highest,
            drt_lowest,
            vtt_highest,
            vtt_lowest,
        } in self.priorities.iter().cloned()
        {
            age -= 1;

            // Digest the lowest and highest priorities in each entry to find the absolute lowest
            // (to be used for "painful" priority estimation) and absolute highest (used for
            // "ludicrous" priority estimation).
            //
            // Priority values are also added to accumulators as the addition part of an age
            // weighted arithmetic mean.
            if let Some(drt_lowest) = drt_lowest {
                absolutes.digest_drt_priority(drt_lowest);
                drt_slow += age * drt_lowest;
                drt_medium += age * (drt_lowest + drt_highest) / 2;
                drt_divisor += age;
            }
            if let Some(vtt_lowest) = vtt_lowest {
                absolutes.digest_vtt_priority(vtt_lowest);
                vtt_slow += age * vtt_lowest;
                vtt_medium += age * (vtt_lowest + vtt_highest) / 2;
                vtt_divisor += age;
            }
            absolutes.digest_drt_priority(drt_highest);
            absolutes.digest_vtt_priority(vtt_highest);
            drt_fast += age * drt_highest;
            vtt_fast += age * vtt_highest;
        }

        // Note that the division part of the weighted arithmetic mean (`x / divisor`) is done in
        // place.
        // Different floors are enforced for the different types of estimate.
        Some(PrioritiesEstimate {
            drt_painful: PriorityEstimate {
                priority: absolutes.drt_lowest.unwrap_or_default(),
                epochs: TimeToBlock::UpTo(capacity),
            },
            drt_slow: PriorityEstimate {
                priority: cmp::max(drt_slow / drt_divisor, 100),
                epochs: TimeToBlock::Unknown,
            },
            drt_medium: PriorityEstimate {
                priority: cmp::max(drt_medium / drt_divisor, 200),
                epochs: TimeToBlock::Unknown,
            },
            drt_fast: PriorityEstimate {
                priority: cmp::max(drt_fast / drt_divisor, 300),
                epochs: TimeToBlock::Unknown,
            },
            drt_ludicrous: PriorityEstimate {
                priority: cmp::max(absolutes.drt_highest, 400),
                epochs: TimeToBlock::LessThan(1),
            },
            vtt_painful: PriorityEstimate {
                priority: absolutes.vtt_lowest.unwrap_or_default(),
                epochs: TimeToBlock::UpTo(capacity),
            },
            vtt_slow: PriorityEstimate {
                priority: cmp::max(vtt_slow / vtt_divisor, 100),
                epochs: TimeToBlock::Unknown,
            },
            vtt_medium: PriorityEstimate {
                priority: cmp::max(vtt_medium / vtt_divisor, 200),
                epochs: TimeToBlock::Unknown,
            },
            vtt_fast: PriorityEstimate {
                priority: cmp::max(vtt_fast / vtt_divisor, 300),
                epochs: TimeToBlock::Unknown,
            },
            vtt_ludicrous: PriorityEstimate {
                priority: cmp::max(absolutes.vtt_highest, 400),
                epochs: TimeToBlock::LessThan(2),
            },
        })
    }

    /// Creates a new engine from a vector of `Priorities`.
    ///
    /// This assumes that the vector is ordered from oldest to newest.
    /// TODO: verify the statement above is true, or whether it's just the opposite
    pub fn from_vec(priorities: Vec<Priorities>) -> Self {
        // Create a new queue with the desired capacity
        let mut fees = CircularQueue::with_capacity(DEFAULT_QUEUE_CAPACITY_EPOCHS);
        // Push as many elements from the input as they can fit in the queue
        priorities
            .into_iter()
            .rev()
            .take(DEFAULT_QUEUE_CAPACITY_EPOCHS)
            .for_each(|entry| {
                fees.push(entry);
            });

        Self { priorities: fees }
    }

    /// Get the entry at a certain position, if an item at that position exists, or None otherwise.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&Priorities> {
        if index >= self.priorities.capacity() {
            None
        } else {
            self.priorities.iter().nth(index)
        }
    }

    /// Push a new `Priorities` entry into the engine.
    #[inline]
    pub fn push_priorities(&mut self, priorities: Priorities) {
        // TODO: make this a debug line
        log::warn!("Pushing new transaction priorities entry: {:?}", priorities);
        self.priorities.push(priorities);
    }

    /// Create a new engine of a certain queue capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            priorities: CircularQueue::with_capacity(capacity),
        }
    }
}

// TODO: show fee estimations
impl fmt::Debug for PriorityEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fees = self
            .priorities
            .iter()
            .enumerate()
            .map(|(i, fees)| format!("{}\t→\t{:?}", i, fees))
            .join("\n");

        write!(
            f,
            "There is priority information for {} epochs:\n{}",
            self.priorities.len(),
            fees,
        )
    }
}

impl Default for PriorityEngine {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_QUEUE_CAPACITY_EPOCHS)
    }
}

impl<'de> Deserialize<'de> for PriorityEngine {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<Priorities>::deserialize(deserializer).map(Self::from_vec)
    }
}

impl Serialize for PriorityEngine {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        Serialize::serialize(&self.as_vec(), serializer)
    }
}

/// Type for each of the entries in `FeesEngine`.
///
/// Fees are always expressed in their relative form (nanowits per weight unit), aka "transaction
/// priority".
#[derive(Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Priorities {
    /// The highest priority used by data request transactions in a block.
    pub drt_highest: u64,
    /// The lowest priority used by data requests transactions in a block.
    pub drt_lowest: Option<u64>,
    /// The highest priority used by value transfer transactions in a block.
    pub vtt_highest: u64,
    /// The lowest priority used by data requests transactions in a block.
    pub vtt_lowest: Option<u64>,
}

impl Priorities {
    /// Process the priority of a data request transaction, and update the highest and lowest values
    /// accordingly, if the provided value is higher or lower than the previously set values.
    #[inline]
    pub fn digest_drt_priority(&mut self, priority: u64) {
        // Update highest
        if priority > self.drt_highest {
            self.drt_highest = priority;
        }
        // Update lowest
        if let Some(drt_lowest) = self.drt_lowest {
            if priority < drt_lowest {
                self.drt_lowest = Some(priority);
            }
        } else if priority > 0 {
            self.drt_lowest = Some(priority);
        }
    }

    /// Process the priority of a value transfer transaction, and update the highest and lowest
    /// values accordingly, if the provided value is higher or lower than the previously set values.
    #[inline]
    pub fn digest_vtt_priority(&mut self, priority: u64) {
        // Update highest
        if priority > self.vtt_highest {
            self.vtt_highest = priority;
        }
        // Update lowest
        if let Some(vtt_lowest) = self.vtt_lowest {
            if priority < vtt_lowest {
                self.vtt_lowest = Some(priority);
            }
        } else if priority > 0 {
            self.vtt_lowest = Some(priority);
        }
    }
}

impl fmt::Debug for Priorities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DRT: (High: {}, Low: {}) | VTT: (High: {}, Low: {})",
            self.drt_highest,
            self.drt_lowest.unwrap_or_default(),
            self.vtt_highest,
            self.vtt_lowest.unwrap_or_default()
        )
    }
}

/// A whole set of estimates for priority of DRT and VTT transactions.
///
/// There are 5 different levels of estimations:
/// - Painful
/// - Slow
/// - Medium
/// - Fast
/// - Ludicrous
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PrioritiesEstimate {
    pub drt_painful: PriorityEstimate,
    pub drt_slow: PriorityEstimate,
    pub drt_medium: PriorityEstimate,
    pub drt_fast: PriorityEstimate,
    pub drt_ludicrous: PriorityEstimate,
    pub vtt_painful: PriorityEstimate,
    pub vtt_slow: PriorityEstimate,
    pub vtt_medium: PriorityEstimate,
    pub vtt_fast: PriorityEstimate,
    pub vtt_ludicrous: PriorityEstimate,
}

/// A estimate for priority and time-to-block.
///
/// Time-to-block states what is the expected time (in epochs) that it would take for a transaction
/// with this priority to be included into a block.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PriorityEstimate {
    pub priority: u64,
    pub epochs: TimeToBlock,
}

/// Allows tagging time-to-block estimations for the sake of UX.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum TimeToBlock {
    /// The time-to-block is around X epochs.
    Around(usize),
    /// The time-to-block is less than X epochs.
    LessThan(usize),
    /// The time-to-block is unknown.
    #[default]
    Unknown,
    /// The time-to-block is up to X epochs.
    UpTo(usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;

    fn priorities_factory(count: u64) -> Vec<Priorities> {
        let mut prng = StdRng::seed_from_u64(0);

        let mut output = vec![];
        for _ in 0..count {
            let mut a = prng.gen_range(0, 10000);
            let mut b = prng.gen_range(0, 10000);
            let mut c = prng.gen_range(0, 10000);
            let mut d = prng.gen_range(0, 10000);

            if a.cmp(&b) == cmp::Ordering::Less {
                (a, b) = (b, a)
            }
            if c.cmp(&d) == cmp::Ordering::Less {
                (c, d) = (d, c)
            }

            output.push(Priorities {
                drt_highest: a,
                drt_lowest: Some(b),
                vtt_highest: c,
                vtt_lowest: Some(d),
            })
        }

        output
    }

    #[test]
    fn engine_from_vec() {
        let input = priorities_factory(2);
        let engine = PriorityEngine::from_vec(input.clone());

        assert_eq!(engine.get(0), input.get(0));
        assert_eq!(engine.get(1), input.get(1));
    }

    #[test]
    fn engine_as_vec() {
        let input = priorities_factory(2);
        let mut engine = PriorityEngine::default();
        for priorities in &input {
            engine.push_priorities(priorities.clone());
        }
        let output = engine.as_vec();

        assert_eq!(output, input);
    }

    #[test]
    fn drt_priorities_digestion() {
        let mut priorities = Priorities::default();
        assert_eq!(priorities.drt_highest, 0);
        assert_eq!(priorities.drt_lowest, None);

        priorities.digest_drt_priority(0);
        assert_eq!(priorities.drt_highest, 0);
        assert_eq!(priorities.drt_lowest, None);

        priorities.digest_drt_priority(5);
        assert_eq!(priorities.drt_highest, 5);
        assert_eq!(priorities.drt_lowest, Some(5));

        priorities.digest_drt_priority(7);
        assert_eq!(priorities.drt_highest, 7);
        assert_eq!(priorities.drt_lowest, Some(5));

        priorities.digest_drt_priority(3);
        assert_eq!(priorities.drt_highest, 7);
        assert_eq!(priorities.drt_lowest, Some(3));
    }

    #[test]
    fn vtt_priorities_digestion() {
        let mut priorities = Priorities::default();
        assert_eq!(priorities.vtt_highest, 0);
        assert_eq!(priorities.vtt_lowest, None);

        priorities.digest_vtt_priority(0);
        assert_eq!(priorities.vtt_highest, 0);
        assert_eq!(priorities.vtt_lowest, None);

        priorities.digest_vtt_priority(5);
        assert_eq!(priorities.vtt_highest, 5);
        assert_eq!(priorities.vtt_lowest, Some(5));

        priorities.digest_vtt_priority(7);
        assert_eq!(priorities.vtt_highest, 7);
        assert_eq!(priorities.vtt_lowest, Some(5));

        priorities.digest_vtt_priority(3);
        assert_eq!(priorities.vtt_highest, 7);
        assert_eq!(priorities.vtt_lowest, Some(3));
    }

    #[test]
    fn cannot_estimate_with_few_epochs_in_queue() {
        let priorities = priorities_factory(MINIMUM_TRACKED_EPOCHS as u64 - 1);
        let engine = PriorityEngine::from_vec(priorities);
        let estimate = engine.estimate_priority();

        assert_eq!(estimate, None);
    }

    #[test]
    fn can_estimate_correctly() {
        let priorities = priorities_factory(100);
        let engine = PriorityEngine::from_vec(priorities);
        println!("{:?}", engine);
        let estimate = engine.estimate_priority().unwrap();
        println!("{:#?}", estimate);

        let expected = PrioritiesEstimate::default();

        assert_eq!(estimate, expected);
    }
}
