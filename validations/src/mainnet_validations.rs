use witnet_data_structures::chain::PublicKeyHash;
use witnet_data_structures::chain::{Environment, Epoch};

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
    } else {
        None
    }
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
}
