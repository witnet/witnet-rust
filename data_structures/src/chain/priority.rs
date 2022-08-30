use std::{cmp, fmt};

use circular_queue::CircularQueue;
use itertools::Itertools;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// Assuming no missing epochs, this will keep track of priority used by transactions in the last 12
// hours (960 epochs).
const QUEUE_CAPACITY: usize = 960;

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

    /// Creates a new engine from a vector of `Priorities`.
    ///
    /// This assumes that the vector is ordered from oldest to newest.
    /// TODO: verify the statement above is true, or whether it's just the opposite
    pub fn from_vec(history: Vec<Priorities>) -> Self {
        // Create a new queue with the desired capacity
        let mut fees = CircularQueue::with_capacity(QUEUE_CAPACITY);
        // Push as many elements from the input as they can fit in the queue
        history.into_iter().take(QUEUE_CAPACITY).for_each(|entry| {
            fees.push(entry);
        });

        Self { priorities: fees }
    }

    /// Push a new `Priorities` entry into the engine.
    pub fn push_priorities(&mut self, priorities: Priorities) {
        log::debug!("Pushing new transaction priorities entry: {:?}", priorities);
        self.priorities.push(priorities);
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
        Self {
            priorities: CircularQueue::with_capacity(QUEUE_CAPACITY),
        }
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
