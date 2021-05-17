use crate::chain::{Environment, Epoch, PublicKeyHash};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Committee for superblock indices 750-1344
const FIRST_EMERGENCY_COMMITTEE: [&str; 7] = [
    "wit1asdpcspwysf0hg5kgwvgsp2h6g65y5kg9gj5dz",
    "wit13l337znc5yuualnxfg9s2hu9txylntq5pyazty",
    "wit17nnjuxmfuu92l6rxhque2qc3u2kvmx2fske4l9",
    "wit1drcpu0xc2akfcqn8r69vw70pj8fzjhjypdcfsq",
    "wit1cyrlc64hyu0rux7hclmg9rxwxpa0v9pevyaj2c",
    "wit1g0rkajsgwqux9rnmkfca5tz6djg0f87x7ms5qx",
    "wit1etherz02v4fvqty6jhdawefd0pl33qtevy7s4z",
];

/// Committee for superblock indices 750-1344
const SECOND_EMERGENCY_COMMITTEE: [&str; 6] = [
    "wit1drcpu0xc2akfcqn8r69vw70pj8fzjhjypdcfsq",
    "wit1drcpu3x42y5vp7w3pe203xrwpnth2pnt6c0dm9",
    "wit1asdpcspwysf0hg5kgwvgsp2h6g65y5kg9gj5dz",
    "wit1cyrlc64hyu0rux7hclmg9rxwxpa0v9pevyaj2c",
    "wit1etherz02v4fvqty6jhdawefd0pl33qtevy7s4z",
    "wit13l337znc5yuualnxfg9s2hu9txylntq5pyazty",
];

/// 22 January 2021 @ 09:00:00 UTC
pub const FIRST_HARD_FORK: Epoch = 192000;
/// 28 April 2021 @ 9:00:00 UTC
pub const SECOND_HARD_FORK: Epoch = 376320;
/// 3 June 2021  @ 9:00:00 UTC
pub const THIRD_HARD_FORK: Epoch = 445440;

/// TAPI Engine
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct TapiEngine {
    /// bit votes counter by bits
    pub bit_tapi_counter: BitTapiCounter,
    /// wip activation
    pub wip_activation: HashMap<String, Epoch>,
}

impl TapiEngine {
    pub fn update_bit_counter(&mut self, v: u32, epoch: Epoch, avoid_wip_list: &HashSet<String>) {
        // In case of empty epochs, they would be considered as blocks with tapi version to 0
        // In order to not update bit counter from old blocks where the block version was not used,
        // the first time (bit_tapi_counter.last_epoch == 0) would be skipped in this conditional branch
        if self.bit_tapi_counter.last_epoch != 0 && epoch > self.bit_tapi_counter.last_epoch + 1 {
            let init = self.bit_tapi_counter.last_epoch + 1;
            let end = epoch;
            for i in init..end {
                self.update_bit_counter(0, i, avoid_wip_list);
            }
        }
        for n in 0..self.bit_tapi_counter.len() {
            if let Some(mut bit_counter) = self.bit_tapi_counter.get_mut(n, &epoch) {
                if !self.wip_activation.contains_key(&bit_counter.wip)
                    && !avoid_wip_list.contains(&bit_counter.wip)
                {
                    if is_bit_n_activated(v, n) {
                        bit_counter.votes += 1;
                    }
                    if (epoch - bit_counter.init) % bit_counter.period == 0 {
                        if (bit_counter.votes * 100) / bit_counter.period >= 80 {
                            // An offset of 21 is added to ensure that the activation of the WIP is
                            // achieved with consolidated blocks
                            self.wip_activation
                                .insert(bit_counter.wip.clone(), epoch + 21);
                        }
                        bit_counter.votes = 0;
                    }
                }
            }
        }
        self.bit_tapi_counter.last_epoch = epoch;
    }

    pub fn initialize_wip_information(&mut self) -> (Epoch, HashSet<String>) {
        // Hardcoded information about WIPs
        let mut bits = vec![vec![]; 32];
        let wip_0014 = BitVotesCounter {
            votes: 0,
            period: 26880,
            wip: "WIP0014".to_string(),
            init: 500000,
            end: 850000,
        };
        bits[0].push(wip_0014);

        // Assessment of new WIPs
        let mut min_epoch = u32::MAX;
        let mut old_wips = HashSet::default();
        for (i, wips) in bits.into_iter().enumerate() {
            for wip in wips {
                if self.bit_tapi_counter.contains(i, &wip.wip) {
                    old_wips.insert(wip.wip);
                } else {
                    if wip.init < min_epoch {
                        min_epoch = wip.init;
                    }
                    self.bit_tapi_counter.insert(i, wip);
                }
            }
        }

        (min_epoch, old_wips)
    }
}

/// Struct that count the positives votes of a WIP
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct BitVotesCounter {
    pub votes: u32,
    pub period: Epoch,
    pub wip: String,
    pub init: Epoch,
    pub end: Epoch,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct BitTapiCounter {
    info: [Vec<BitVotesCounter>; 32],
    last_epoch: Epoch,
    current_length: usize,
}

impl BitTapiCounter {
    pub fn get_mut(&mut self, bit: usize, epoch: &u32) -> Option<&mut BitVotesCounter> {
        match self.info.get_mut(bit) {
            Some(bit_info) => {
                for i in bit_info {
                    if *epoch >= i.init && *epoch < i.end {
                        return Some(i);
                    }
                }

                None
            }
            None => None,
        }
    }

    pub fn insert(&mut self, k: usize, v: BitVotesCounter) {
        match self.info.get_mut(k) {
            Some(bit_info) => {
                bit_info.push(v);
            }
            None => {
                self.info[k] = vec![v];
            }
        }

        if k >= self.current_length {
            self.current_length = k + 1;
        }
    }

    pub fn contains(&self, bit: usize, wip: &str) -> bool {
        match self.info.get(bit) {
            Some(bit_info) => {
                for i in bit_info {
                    if i.wip.eq(wip) {
                        return true;
                    }
                }

                false
            }
            None => false,
        }
    }

    pub fn len(&self) -> usize {
        self.current_length
    }

    pub fn is_empty(&self) -> bool {
        self.current_length == 0
    }
}

fn is_bit_n_activated(v: u32, n: usize) -> bool {
    v & (1 << n) != 0
}

/// Return a hard-coded signing committee if the provided epoch belongs to an emergency period.
/// 750 and 1344: Between those indices, a special committee of 7 nodes was set.
pub fn in_emergency_period(
    superblock_index: Epoch,
    environment: Environment,
) -> Option<Vec<PublicKeyHash>> {
    if Environment::Mainnet == environment && superblock_index > 750 && superblock_index < 1344 {
        Some(
            FIRST_EMERGENCY_COMMITTEE
                .iter()
                .map(|address| address.parse().expect("Malformed signing committee"))
                .collect(),
        )
    } else if Environment::Mainnet == environment
        && superblock_index > 44165
        && superblock_index < 45509
    {
        Some(
            SECOND_EMERGENCY_COMMITTEE
                .iter()
                .map(|address| address.parse().expect("Malformed signing committee"))
                .collect(),
        )
    } else {
        None
    }
}

/// Returns a boolean indicating whether the epoch provided is after the first hard fork date
pub fn after_first_hard_fork(epoch: Epoch, environment: Environment) -> bool {
    epoch >= FIRST_HARD_FORK && Environment::Mainnet == environment
}

/// Returns a boolean indicating whether the epoch provided is after the second hard fork date
pub fn after_second_hard_fork(epoch: Epoch, environment: Environment) -> bool {
    epoch >= SECOND_HARD_FORK && Environment::Mainnet == environment
}

/// Returns a boolean indicating whether the epoch provided is after the third hard fork date
pub fn after_third_hard_fork(epoch: Epoch, environment: Environment) -> bool {
    epoch >= THIRD_HARD_FORK && Environment::Mainnet == environment
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_emergency_period_not_mainnet() {
        assert_eq!(in_emergency_period(1300, Environment::Testnet), None)
    }
    #[test]
    fn test_in_emergency_period_not_inside_period() {
        assert_eq!(in_emergency_period(50, Environment::Mainnet), None)
    }
    #[test]
    fn test_in_emergency_period_inside_first_emergency_period() {
        assert_eq!(
            in_emergency_period(800, Environment::Mainnet),
            Some(
                FIRST_EMERGENCY_COMMITTEE
                    .iter()
                    .map(|address| address.parse().expect("Malformed signing committee"))
                    .collect(),
            )
        )
    }

    #[test]
    fn test_is_bit_n_activated() {
        let aux = 1;
        assert!(is_bit_n_activated(aux, 0));
        assert!(!is_bit_n_activated(aux, 1));

        let aux = 2;
        assert!(!is_bit_n_activated(aux, 0));
        assert!(is_bit_n_activated(aux, 1));

        let aux = 3;
        assert!(is_bit_n_activated(aux, 0));
        assert!(is_bit_n_activated(aux, 1));
    }

    #[test]
    fn test_bit_tapicounter() {
        let mut a = BitTapiCounter::default();
        assert!(a.get_mut(0, &100).is_none());

        let mut aux = BitVotesCounter::default();
        aux.init = 0;
        aux.end = 50;
        aux.wip = "Wip1".to_string();
        a.insert(0, aux);
        assert!(a.get_mut(0, &100).is_none());
        assert!(a.contains(0, &"Wip1".to_string()));
        assert!(!a.contains(1, &"Wip1".to_string()));

        let mut aux2 = BitVotesCounter::default();
        aux2.init = 75;
        aux2.end = 125;
        aux2.wip = "Wip2".to_string();
        a.insert(0, aux2);
        assert_eq!(a.get_mut(0, &100).unwrap().wip, "Wip2".to_string());
        assert!(a.get_mut(1, &100).is_none());
        assert!(a.contains(0, &"Wip2".to_string()));

        assert_eq!(a.get_mut(0, &100).unwrap().votes, 0);
        let mut votes_counter = a.get_mut(0, &100).unwrap();
        votes_counter.votes += 1;
        assert_eq!(a.get_mut(0, &100).unwrap().votes, 1);
    }
}
