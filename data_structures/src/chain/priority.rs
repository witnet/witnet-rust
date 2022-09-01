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
        let mut drt_low = 0u64;
        let mut drt_medium = 0u64;
        let mut drt_high = 0u64;
        let mut vtt_low = 0u64;
        let mut vtt_medium = 0u64;
        let mut vtt_high = 0u64;
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
            // (to be used for "stinky" priority estimation) and absolute highest (used for
            // "opulent" priority estimation).
            //
            // Priority values are also added to accumulators as the addition part of an age
            // weighted arithmetic mean.
            if let Some(drt_lowest) = drt_lowest {
                absolutes.digest_drt_priority(drt_lowest);
                drt_low += age * drt_lowest;
                drt_medium += age * (drt_lowest + drt_highest) / 2;
                drt_divisor += age;
            }
            if let Some(vtt_lowest) = vtt_lowest {
                absolutes.digest_vtt_priority(vtt_lowest);
                vtt_low += age * vtt_lowest;
                vtt_medium += age * (vtt_lowest + vtt_highest) / 2;
                vtt_divisor += age;
            }
            absolutes.digest_drt_priority(drt_highest);
            absolutes.digest_vtt_priority(vtt_highest);
            drt_high += age * drt_highest;
            vtt_high += age * vtt_highest;
        }

        // Different floors are enforced on the different tiers of priority.
        // Some are also corrected by 15% up or down to make priorities more dynamic.
        let drt_stinky_priority = absolutes.drt_lowest.unwrap_or_default();
        let drt_low_priority = cmp::max(drt_low * 85 / drt_divisor / 100, 10);
        let drt_medium_priority = cmp::max(drt_medium / drt_divisor, 20);
        let drt_high_priority = cmp::max(drt_high * 115 / drt_divisor / 100, 30);
        let drt_opulent_priority = cmp::max(absolutes.drt_highest * 110 / 100, 40);
        let vtt_stinky_priority = absolutes.vtt_lowest.unwrap_or_default();
        let vtt_low_priority = cmp::max(vtt_low * 85 / vtt_divisor / 100, 10);
        let vtt_medium_priority = cmp::max(vtt_medium / vtt_divisor, 20);
        let vtt_high_priority = cmp::max(vtt_high * 115 / vtt_divisor / 100, 30);
        let vtt_opulent_priority = cmp::max(absolutes.vtt_highest * 110 / 100, 40);

        // Collect the relative epochs inside the engine in which each tier of priority was enough
        // for making it into a block, by comparing to the lowest priority mined in that epoch.
        let mut drt_stinky_enough_epochs = vec![];
        let mut drt_low_enough_epochs = vec![];
        let mut drt_medium_enough_epochs = vec![];
        let mut drt_high_enough_epochs = vec![];
        let mut vtt_stinky_enough_epochs = vec![];
        let mut vtt_low_enough_epochs = vec![];
        let mut vtt_medium_enough_epochs = vec![];
        let mut vtt_high_enough_epochs = vec![];
        for (epoch, priorities) in self.priorities.iter().enumerate() {
            if Some(drt_stinky_priority) >= priorities.drt_lowest {
                drt_stinky_enough_epochs.push(epoch);
            }
            if Some(drt_low_priority) >= priorities.drt_lowest {
                drt_low_enough_epochs.push(epoch);
            }
            if Some(drt_medium_priority) >= priorities.drt_lowest {
                drt_medium_enough_epochs.push(epoch);
            }
            if Some(drt_high_priority) >= priorities.drt_lowest {
                drt_high_enough_epochs.push(epoch);
            }
            if Some(vtt_stinky_priority) >= priorities.drt_lowest {
                vtt_stinky_enough_epochs.push(epoch);
            }
            if Some(vtt_low_priority) >= priorities.vtt_lowest {
                vtt_low_enough_epochs.push(epoch);
            }
            if Some(vtt_medium_priority) >= priorities.vtt_lowest {
                vtt_medium_enough_epochs.push(epoch);
            }
            if Some(vtt_high_priority) >= priorities.vtt_lowest {
                vtt_high_enough_epochs.push(epoch);
            }
        }

        // Measure the average time between occurrences of a tier of priority being enough for
        // making it into a block.
        let drt_stinky_ttb = cmp::max(average_gap(drt_stinky_enough_epochs, len), capacity / 2);
        let drt_low_ttb = cmp::max(average_gap(drt_low_enough_epochs, len), 2);
        let drt_medium_ttb = cmp::max(average_gap(drt_medium_enough_epochs, len), 2);
        let drt_high_ttb = cmp::max(average_gap(drt_high_enough_epochs, len), 2);
        let vtt_stinky_ttb = cmp::max(average_gap(vtt_stinky_enough_epochs, len), capacity / 2);
        let vtt_low_ttb = cmp::max(average_gap(vtt_low_enough_epochs, len), 2);
        let vtt_medium_ttb = cmp::max(average_gap(vtt_medium_enough_epochs, len), 2);
        let vtt_high_ttb = cmp::max(average_gap(vtt_high_enough_epochs, len), 2);

        Some(PrioritiesEstimate {
            drt_stinky: PriorityEstimate {
                priority: drt_stinky_priority,
                time_to_block: TimeToBlock::UpTo(drt_stinky_ttb),
            },
            drt_low: PriorityEstimate {
                priority: drt_low_priority,
                time_to_block: TimeToBlock::Around(drt_low_ttb),
            },
            drt_medium: PriorityEstimate {
                priority: drt_medium_priority,
                time_to_block: TimeToBlock::Around(drt_medium_ttb),
            },
            drt_high: PriorityEstimate {
                priority: drt_high_priority,
                time_to_block: TimeToBlock::Around(drt_high_ttb),
            },
            drt_opulent: PriorityEstimate {
                priority: drt_opulent_priority,
                time_to_block: TimeToBlock::LessThan(2),
            },
            vtt_stinky: PriorityEstimate {
                priority: vtt_stinky_priority,
                time_to_block: TimeToBlock::UpTo(vtt_stinky_ttb),
            },
            vtt_low: PriorityEstimate {
                priority: vtt_low_priority,
                time_to_block: TimeToBlock::Around(vtt_low_ttb),
            },
            vtt_medium: PriorityEstimate {
                priority: vtt_medium_priority,
                time_to_block: TimeToBlock::Around(vtt_medium_ttb),
            },
            vtt_high: PriorityEstimate {
                priority: vtt_high_priority,
                time_to_block: TimeToBlock::Around(vtt_high_ttb),
            },
            vtt_opulent: PriorityEstimate {
                priority: vtt_opulent_priority,
                time_to_block: TimeToBlock::LessThan(2),
            },
        })
    }

    /// Creates a new engine from a vector of `Priorities`.
    ///
    /// This assumes that the vector is ordered from newest to oldest.
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
        log::debug!("Pushing new transaction priorities entry: {:?}", priorities);
        self.priorities.push(priorities);
        log::debug!(
            "The priority engine has received new data. The priority estimate is now:\n{}",
            self.estimate_priority().unwrap_or_default()
        );
    }

    /// Create a new engine of a certain queue capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            priorities: CircularQueue::with_capacity(capacity),
        }
    }
}

impl fmt::Debug for PriorityEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fees = self
            .priorities
            .iter()
            .enumerate()
            .map(|(i, fees)| format!("{:>4} → {:?}", i, fees))
            .join("\n");

        write!(
            f,
            "[PriorityEngine] There is priority information for {} epochs:\n{}",
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
            "[DRT](↑{:<10}, ↓{:<10}) | [VTT](↑{:<10}, ↓{:<10})",
            self.drt_highest,
            self.drt_lowest.unwrap_or_default(),
            self.vtt_highest,
            self.vtt_lowest.unwrap_or_default()
        )
    }
}

/// A whole set of estimates for priority of DRT and VTT transactions.
///
/// Each estimate contains values for 5 different tiers of priority:
/// - Stinky
/// - low
/// - Medium
/// - High
/// - Opulent
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrioritiesEstimate {
    pub drt_stinky: PriorityEstimate,
    pub drt_low: PriorityEstimate,
    pub drt_medium: PriorityEstimate,
    pub drt_high: PriorityEstimate,
    pub drt_opulent: PriorityEstimate,
    pub vtt_stinky: PriorityEstimate,
    pub vtt_low: PriorityEstimate,
    pub vtt_medium: PriorityEstimate,
    pub vtt_high: PriorityEstimate,
    pub vtt_opulent: PriorityEstimate,
}

impl fmt::Display for PrioritiesEstimate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            r#"╔══════════════════════════════════════════════════════════╗
║ TRANSACTION PRIORITY ESTIMATION REPORT                   ║
╠══════════════════════════════════════════════════════════╣
║ Data request transactions                                ║
╟──────────┬──────────────────┬────────────────────────────║
║     Tier │ Priority         │ Time-to-block              ║
╟──────────┼──────────────────┼────────────────────────────║
║   Stinky │ {:<45} ║
║      Low │ {:<45} ║
║   Medium │ {:<45} ║
║     High │ {:<45} ║
║  Opulent │ {:<45} ║
╠══════════════════════════════════════════════════════════╣
║ Value transfer transactions                              ║
╟──────────┬──────────────────┬────────────────────────────║
║     Tier │ Priority         │ Time-to-block              ║
╟──────────┼──────────────────┼────────────────────────────║
║   Stinky │ {:<45} ║
║      Low │ {:<45} ║
║   Medium │ {:<45} ║
║     High │ {:<45} ║
║  Opulent │ {:<45} ║
╚══════════════════════════════════════════════════════════╝"#,
            self.drt_stinky.to_string(),
            self.drt_low.to_string(),
            self.drt_medium.to_string(),
            self.drt_high.to_string(),
            self.drt_opulent.to_string(),
            self.vtt_stinky.to_string(),
            self.vtt_low.to_string(),
            self.vtt_medium.to_string(),
            self.vtt_high.to_string(),
            self.vtt_opulent.to_string(),
        )
    }
}

/// A estimate for priority and time-to-block.
///
/// Time-to-block states what is the expected time (in epochs) that it would take for a transaction
/// with this priority to be included into a block.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PriorityEstimate {
    pub priority: u64,
    pub time_to_block: TimeToBlock,
}

impl fmt::Display for PriorityEstimate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:<16} | {}", self.priority, self.time_to_block)
    }
}

/// Allows tagging time-to-block estimations for the sake of UX.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
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

impl fmt::Display for TimeToBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeToBlock::Around(x) => write!(f, "around {} epochs", x),
            TimeToBlock::LessThan(x) => write!(f, "less than {} epochs", x),
            TimeToBlock::Unknown => write!(f, "unknown"),
            TimeToBlock::UpTo(x) => write!(f, "up to {} epochs", x),
        }
    }
}

/// Calculates the average gap between the values in a `Vec<usize>`.
fn average_gap(occurrences: Vec<usize>, sample_size: usize) -> usize {
    let (_, gaps) = occurrences.iter().fold((0, 0), |(prev_i, sum), cur_i| {
        let gap = *cur_i - prev_i;

        (*cur_i, (sum + gap).saturating_sub(1))
    });

    sample_size
        .saturating_div(occurrences.len().saturating_sub(gaps).saturating_add(1))
        .saturating_add(1)
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
        use TimeToBlock::*;

        let priorities = priorities_factory(100);
        let engine = PriorityEngine::from_vec(priorities);
        let estimate = engine.estimate_priority().unwrap();

        let expected = PrioritiesEstimate {
            drt_stinky: PriorityEstimate {
                priority: 70,
                time_to_block: UpTo(480),
            },
            drt_low: PriorityEstimate {
                priority: 2788,
                time_to_block: Around(12),
            },
            drt_medium: PriorityEstimate {
                priority: 5157,
                time_to_block: Around(2),
            },
            drt_high: PriorityEstimate {
                priority: 8089,
                time_to_block: Around(2),
            },
            drt_opulent: PriorityEstimate {
                priority: 10931,
                time_to_block: LessThan(2),
            },
            vtt_stinky: PriorityEstimate {
                priority: 26,
                time_to_block: UpTo(480),
            },
            vtt_low: PriorityEstimate {
                priority: 2943,
                time_to_block: Around(101),
            },
            vtt_medium: PriorityEstimate {
                priority: 4891,
                time_to_block: Around(3),
            },
            vtt_high: PriorityEstimate {
                priority: 7266,
                time_to_block: Around(2),
            },
            vtt_opulent: PriorityEstimate {
                priority: 10917,
                time_to_block: LessThan(2),
            },
        };

        assert_eq!(estimate, expected);
    }
}
