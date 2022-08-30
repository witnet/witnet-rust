use std::{cmp, fmt};

use circular_queue::CircularQueue;
use itertools::Itertools;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// Assuming no missing epochs, this will keep track of priority used by transactions in the last 12
// hours (960 epochs).
const DEFAULT_QUEUE_CAPACITY: usize = 960;

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
        self.priorities.iter().cloned().collect_vec()
    }

    /// Provide suggestions for sensible transaction priority values, together with their expected
    /// time-to-block in epochs.
    ///
    /// This is only a first approach to an estimation algorithm. There is abundant prior art about
    /// fee estimation in other blockchains. We might revisit this once we collect more insights
    /// about our fees market and user feedback.
    pub fn estimate_priority(&self) -> PrioritiesEstimate {
        // Find out the queue capacity. We can only provide estimates up to this number of epochs.
        let capacity = self.priorities.capacity();
        // Guess how many entries we actually have in the engine. Needed for weighting entries based
        // on age.
        let len = self.priorities.len();
        let len = u64::try_from(self.priorities.len()).unwrap_or(len as u64);
        // Will keep track of the absolute minimum and maximum priorities found in the engine.
        let mut absolutes = Priorities::default();
        // Initialize accumulators for different priorities
        let mut drt_slow = 0u64;
        let mut drt_fast = 0u64;
        let mut drt_medium = 0u64;
        let mut vtt_slow = 0u64;
        let mut vtt_fast = 0u64;
        let mut vtt_medium = 0u64;

        for (
            age,
            Priorities {
                drt_highest,
                drt_lowest,
                vtt_highest,
                vtt_lowest,
            },
        ) in self.priorities.iter().cloned().enumerate()
        {
            // Digest the lowest and highest priorities in each entry to find the absolute lowest
            // (to be used for "painful" priority estimation) and absolute highest (used for
            // "ludicrous" priority estimation).
            absolutes.digest_drt_priority(drt_lowest);
            absolutes.digest_drt_priority(drt_highest);
            absolutes.digest_vtt_priority(vtt_lowest);
            absolutes.digest_vtt_priority(vtt_highest);

            // Add priority values to accumulators. This is the addition part of an age-weighted
            // arithmetic mean.
            let age = u64::try_from(age).unwrap_or(age as u64);
            drt_slow += age * drt_lowest;
            drt_fast += age * drt_highest;
            drt_medium += age * (drt_lowest + drt_highest) / 2;
            vtt_slow += age * vtt_lowest;
            vtt_fast += age * vtt_highest;
            vtt_medium += age * (vtt_lowest + vtt_highest) / 2;
        }

        // Note that the division part of the weighted arithmetic mean (`x / len`) is done in place
        PrioritiesEstimate {
            drt_painful: PriorityEstimate {
                priority: absolutes.drt_lowest,
                epochs: TimeToBlock::UpTo(capacity),
            },
            drt_slow: PriorityEstimate {
                priority: drt_slow / len,
                epochs: TimeToBlock::Unknown,
            },
            drt_medium: PriorityEstimate {
                priority: drt_medium / len,
                epochs: TimeToBlock::Unknown,
            },
            drt_fast: PriorityEstimate {
                priority: drt_fast / len,
                epochs: TimeToBlock::Unknown,
            },
            drt_ludicrous: PriorityEstimate {
                priority: absolutes.drt_highest,
                epochs: TimeToBlock::LessThan(1),
            },
            vtt_painful: PriorityEstimate {
                priority: absolutes.vtt_lowest,
                epochs: TimeToBlock::UpTo(capacity),
            },
            vtt_slow: PriorityEstimate {
                priority: vtt_slow / len,
                epochs: TimeToBlock::Unknown,
            },
            vtt_medium: PriorityEstimate {
                priority: vtt_medium / len,
                epochs: TimeToBlock::Unknown,
            },
            vtt_fast: PriorityEstimate {
                priority: vtt_fast / len,
                epochs: TimeToBlock::Unknown,
            },
            vtt_ludicrous: PriorityEstimate {
                priority: absolutes.vtt_highest,
                epochs: TimeToBlock::LessThan(1),
            },
        }
    }

    /// Creates a new engine from a vector of `Priorities`.
    ///
    /// This assumes that the vector is ordered from oldest to newest.
    /// TODO: verify the statement above is true, or whether it's just the opposite
    pub fn from_vec(priorities: Vec<Priorities>) -> Self {
        // Create a new queue with the desired capacity
        let mut fees = CircularQueue::with_capacity(DEFAULT_QUEUE_CAPACITY);
        // Push as many elements from the input as they can fit in the queue
        priorities
            .into_iter()
            .take(DEFAULT_QUEUE_CAPACITY)
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
        log::debug!("Pushing new transaction priorities entry: {:?}", priorities);
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
        Self::with_capacity(DEFAULT_QUEUE_CAPACITY)
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
    pub drt_lowest: u64,
    /// The highest priority used by value transfer transactions in a block.
    pub vtt_highest: u64,
    /// The lowest priority used by data requests transactions in a block.
    pub vtt_lowest: u64,
}

impl Priorities {
    /// Process the priority of a data request transaction, and update the highest and lowest values
    /// accordingly, if the provided value is higher or lower than the previously set values.
    pub fn digest_drt_priority(&mut self, priority: u64) {
        self.drt_highest = cmp::max(self.drt_highest, priority);
        self.drt_lowest = cmp::min(self.drt_lowest, priority);
    }

    /// Process the priority of a value transfer transaction, and update the highest and lowest values
    /// accordingly, if the provided value is higher or lower than the previously set values.
    pub fn digest_vtt_priority(&mut self, priority: u64) {
        self.vtt_highest = cmp::max(self.vtt_highest, priority);
        self.vtt_lowest = cmp::min(self.vtt_lowest, priority);
    }
}

impl fmt::Debug for Priorities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DRT: (High: {}, Low: {}) | VTT: (High: {}, Low: {})",
            self.drt_highest, self.drt_lowest, self.vtt_highest, self.vtt_lowest
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
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct PriorityEstimate {
    pub priority: u64,
    pub epochs: TimeToBlock,
}

/// Allows tagging time-to-block estimations for the sake of UX.
#[derive(Clone, Debug)]
pub enum TimeToBlock {
    /// The time-to-block is around X epochs.
    Around(usize),
    /// The time-to-block is less than X epochs.
    LessThan(usize),
    /// The time-to-block is unknown.
    Unknown,
    /// The time-to-block is up to X epochs.
    UpTo(usize),
}

#[cfg(test)]
mod tests {
    use crate::chain::priority::{Priorities, PriorityEngine};

    fn priorities_factory(count: u64) -> Vec<Priorities> {
        let mut output = vec![];
        for i in 0..count {
            output.push(Priorities {
                drt_highest: 4 * i + 3,
                drt_lowest: 4 * i + 2,
                vtt_highest: 4 * i + 1,
                vtt_lowest: 4 * i,
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
}
