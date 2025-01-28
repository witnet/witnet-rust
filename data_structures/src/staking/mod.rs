#![deny(missing_docs)]

/// Errors related to the staking functionality.
pub mod errors;
/// Auxiliary convenience types and data structures.
pub mod helpers;
/// The data structure and related logic for stake entries.
pub mod stake;
/// The data structure and related logic for keeping track of multiple stake entries.
pub mod stakes;

/// Module re-exporting virtually every submodule on a single level to ease importing of everything
/// staking-related.
pub mod prelude {
    pub use super::errors::*;
    pub use super::helpers::*;
    pub use super::stake::*;
    pub use super::stakes::*;
    pub use crate::capabilities::*;
}

#[cfg(test)]
/// Test module
pub mod test {
    use super::prelude::*;

    const MIN_STAKE_NANOWITS: u64 = 1;

    #[test]
    fn test_e2e() {
        let mut stakes = StakesTester::default();

        // Alpha stakes 2 @ epoch 0
        stakes
            .add_stake("Alpha", 2, 0, true, MIN_STAKE_NANOWITS)
            .unwrap();

        // Nobody holds any power just yet
        let rank = stakes.by_rank(Capability::Mining, 0).collect::<Vec<_>>();
        assert_eq!(rank, vec![("Alpha".into(), 0)]);

        // One epoch later, Alpha starts to hold power
        let rank = stakes.by_rank(Capability::Mining, 1).collect::<Vec<_>>();
        assert_eq!(rank, vec![("Alpha".into(), 2)]);

        // Beta stakes 5 @ epoch 10
        stakes
            .add_stake("Beta", 5, 10, true, MIN_STAKE_NANOWITS)
            .unwrap();

        // Alpha is still leading, but Beta has scheduled its takeover
        let rank = stakes.by_rank(Capability::Mining, 10).collect::<Vec<_>>();
        assert_eq!(rank, vec![("Alpha".into(), 20), ("Beta".into(), 0)]);

        // Beta eventually takes over after epoch 16
        let rank = stakes.by_rank(Capability::Mining, 16).collect::<Vec<_>>();
        assert_eq!(rank, vec![("Alpha".into(), 32), ("Beta".into(), 30)]);
        let rank = stakes.by_rank(Capability::Mining, 17).collect::<Vec<_>>();
        assert_eq!(rank, vec![("Beta".into(), 35), ("Alpha".into(), 34)]);

        // Gamma should never take over, even in a million epochs, because it has only 1 coin
        stakes
            .add_stake("Gamma", 1, 30, true, MIN_STAKE_NANOWITS)
            .unwrap();
        let rank = stakes
            .by_rank(Capability::Mining, 1_000_000)
            .collect::<Vec<_>>();
        assert_eq!(
            rank,
            vec![
                ("Beta".into(), 4_999_950),
                ("Alpha".into(), 2_000_000),
                ("Gamma".into(), 999_970)
            ]
        );

        // But Delta is here to change it all
        stakes
            .add_stake("Delta", 1_000, 50, true, MIN_STAKE_NANOWITS)
            .unwrap();
        let rank = stakes.by_rank(Capability::Mining, 50).collect::<Vec<_>>();
        assert_eq!(
            rank,
            vec![
                ("Beta".into(), 200),
                ("Alpha".into(), 100),
                ("Gamma".into(), 20),
                ("Delta".into(), 0)
            ]
        );
        let rank = stakes.by_rank(Capability::Mining, 51).collect::<Vec<_>>();
        assert_eq!(
            rank,
            vec![
                ("Delta".into(), 1_000),
                ("Beta".into(), 205),
                ("Alpha".into(), 102),
                ("Gamma".into(), 21)
            ]
        );

        // If Alpha removes all of its stake, it should immediately disappear
        stakes
            .remove_stake("Alpha", 2, 52, true, MIN_STAKE_NANOWITS)
            .unwrap();
        let rank = stakes.by_rank(Capability::Mining, 51).collect::<Vec<_>>();
        assert_eq!(
            rank,
            vec![
                ("Delta".into(), 1_000),
                ("Beta".into(), 205),
                ("Gamma".into(), 21),
            ]
        );
    }
}
