use witnet_data_structures::{
    chain::{tapi::current_active_wips, Hash, Reputation},
    proto::versioning::ProtocolVersion,
    staking::prelude::Power,
};

use std::cmp::Ordering;

use crate::{eligibility::legacy::*, validations::*};

#[test]
fn test_compare_candidate_same_section() {
    let bh_1 = Hash::SHA256([10; 32]);
    let bh_2 = Hash::SHA256([20; 32]);
    let rep_1 = Reputation(0);
    let rep_2 = Reputation(2);
    let vrf_1 = Hash::SHA256([1; 32]);
    let vrf_2 = Hash::SHA256([2; 32]);
    // Only one section and all VRFs are valid
    let vrf_sections = VrfSlots::default();
    // Dummy zero power variable for tests before Witnet 2.0
    let power_zero = Power::from(0 as u64);

    // The candidate with reputation always wins
    for &bh_i in &[bh_1, bh_2] {
        for &bh_j in &[bh_1, bh_2] {
            for &vrf_i in &[vrf_1, vrf_2] {
                for &vrf_j in &[vrf_1, vrf_2] {
                    for &act_i in &[true, false] {
                        for &act_j in &[true, false] {
                            assert_eq!(
                                compare_block_candidates(
                                    bh_i,
                                    rep_1,
                                    vrf_i,
                                    act_i,
                                    power_zero,
                                    bh_j,
                                    rep_2,
                                    vrf_j,
                                    act_j,
                                    power_zero,
                                    &vrf_sections,
                                    ProtocolVersion::V1_7,
                                ),
                                Ordering::Less
                            );
                            assert_eq!(
                                compare_block_candidates(
                                    bh_i,
                                    rep_2,
                                    vrf_i,
                                    act_i,
                                    power_zero,
                                    bh_j,
                                    rep_1,
                                    vrf_j,
                                    act_j,
                                    power_zero,
                                    &vrf_sections,
                                    ProtocolVersion::V1_7,
                                ),
                                Ordering::Greater
                            );
                        }
                    }
                }
            }
        }
    }

    // Equal reputation: the candidate that is active wins
    for &bh_i in &[bh_1, bh_2] {
        for &bh_j in &[bh_1, bh_2] {
            for &vrf_i in &[vrf_1, vrf_2] {
                for &vrf_j in &[vrf_1, vrf_2] {
                    assert_eq!(
                        compare_block_candidates(
                            bh_i,
                            rep_1,
                            vrf_i,
                            true,
                            power_zero,
                            bh_j,
                            rep_1,
                            vrf_j,
                            false,
                            power_zero,
                            &vrf_sections,
                            ProtocolVersion::V1_7,
                        ),
                        Ordering::Greater
                    );
                    assert_eq!(
                        compare_block_candidates(
                            bh_i,
                            rep_2,
                            vrf_i,
                            false,
                            power_zero,
                            bh_j,
                            rep_2,
                            vrf_j,
                            true,
                            power_zero,
                            &vrf_sections,
                            ProtocolVersion::V1_7,
                        ),
                        Ordering::Less
                    );
                }
            }
        }
    }

    // Equal reputation and activity: the candidate with lower VRF hash wins
    for &bh_i in &[bh_1, bh_2] {
        for &bh_j in &[bh_1, bh_2] {
            assert_eq!(
                compare_block_candidates(
                    bh_i,
                    rep_1,
                    vrf_1,
                    true,
                    power_zero,
                    bh_j,
                    rep_1,
                    vrf_2,
                    true,
                    power_zero,
                    &vrf_sections,
                    ProtocolVersion::V1_7,
                ),
                Ordering::Greater
            );
            assert_eq!(
                compare_block_candidates(
                    bh_i,
                    rep_1,
                    vrf_2,
                    true,
                    power_zero,
                    bh_j,
                    rep_1,
                    vrf_1,
                    true,
                    power_zero,
                    &vrf_sections,
                    ProtocolVersion::V1_7,
                ),
                Ordering::Less
            );
        }
    }

    // Equal reputation, equal activity and equal VRF hash: the candidate with lower block hash wins
    assert_eq!(
        compare_block_candidates(
            bh_1,
            rep_1,
            vrf_1,
            true,
            power_zero,
            bh_2,
            rep_1,
            vrf_1,
            true,
            power_zero,
            &vrf_sections,
            ProtocolVersion::V1_7,
        ),
        Ordering::Greater
    );
    assert_eq!(
        compare_block_candidates(
            bh_2,
            rep_1,
            vrf_1,
            true,
            power_zero,
            bh_1,
            rep_1,
            vrf_1,
            true,
            power_zero,
            &vrf_sections,
            ProtocolVersion::V1_7,
        ),
        Ordering::Less
    );

    // Everything equal: it is the same block
    assert_eq!(
        compare_block_candidates(
            bh_1,
            rep_1,
            vrf_1,
            true,
            power_zero,
            bh_1,
            rep_1,
            vrf_1,
            true,
            power_zero,
            &vrf_sections,
            ProtocolVersion::V1_7,
        ),
        Ordering::Equal
    );
}

#[test]
fn test_compare_candidate_different_section() {
    let bh_1 = Hash::SHA256([10; 32]);
    let bh_2 = Hash::SHA256([20; 32]);
    let rep_1 = Reputation(0);
    let rep_2 = Reputation(2);
    // Candidate 1 should always be better than candidate 2
    let vrf_sections = VrfSlots::from_rf(16, 1, 2, 1001, 0, 0, &current_active_wips());
    // Candidate 1 is in section 0
    let vrf_1 = vrf_sections.target_hashes()[0];
    // Candidate 2 is in section 1
    let vrf_2 = vrf_sections.target_hashes()[1];
    // Dummy zero power variable for tests before Witnet 2.0
    let power_zero = Power::from(0 as u64);

    // The candidate in the lower section always wins
    for &bh_i in &[bh_1, bh_2] {
        for &bh_j in &[bh_1, bh_2] {
            for &rep_i in &[rep_1, rep_2] {
                for &rep_j in &[rep_1, rep_2] {
                    for &act_i in &[true, false] {
                        for &act_j in &[true, false] {
                            assert_eq!(
                                compare_block_candidates(
                                    bh_i,
                                    rep_i,
                                    vrf_1,
                                    act_i,
                                    power_zero,
                                    bh_j,
                                    rep_j,
                                    vrf_2,
                                    act_j,
                                    power_zero,
                                    &vrf_sections,
                                    ProtocolVersion::V1_7,
                                ),
                                Ordering::Greater
                            );
                            assert_eq!(
                                compare_block_candidates(
                                    bh_i,
                                    rep_i,
                                    vrf_2,
                                    act_i,
                                    power_zero,
                                    bh_j,
                                    rep_j,
                                    vrf_1,
                                    act_j,
                                    power_zero,
                                    &vrf_sections,
                                    ProtocolVersion::V1_7,
                                ),
                                Ordering::Less
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn test_compare_candidate_different_reputation_bigger_than_zero() {
    let bh_1 = Hash::SHA256([10; 32]);
    let bh_2 = Hash::SHA256([20; 32]);
    let rep_1 = Reputation(1);
    let rep_2 = Reputation(2);
    let vrf_1 = Hash::SHA256([1; 32]);
    let vrf_2 = Hash::SHA256([2; 32]);
    // Only one section and all VRFs are valid
    let vrf_sections = VrfSlots::default();
    // Dummy zero power variable for tests before Witnet 2.0
    let power_zero = Power::from(0 as u64);

    // In case of active nodes with reputation, the difference will be the vrf not the reputation
    assert_eq!(
        compare_block_candidates(
            bh_1,
            rep_1,
            vrf_1,
            true,
            power_zero,
            bh_2,
            rep_2,
            vrf_2,
            true,
            power_zero,
            &vrf_sections,
            ProtocolVersion::V1_7,
        ),
        Ordering::Greater
    );

    assert_eq!(
        compare_block_candidates(
            bh_1,
            rep_1,
            vrf_2,
            true,
            power_zero,
            bh_2,
            rep_2,
            vrf_1,
            true,
            power_zero,
            &vrf_sections,
            ProtocolVersion::V1_7,
        ),
        Ordering::Less
    );
}

#[test]
fn test_compare_candidates_witnet_pos() {
    let bh_1 = Hash::SHA256([10; 32]);
    let bh_2 = Hash::SHA256([20; 32]);
    let rep_1 = Reputation(0);
    let rep_2 = Reputation(0);
    let vrf_1 = Hash::SHA256([1; 32]);
    let vrf_2 = Hash::SHA256([2; 32]);
    let vrf_sections = VrfSlots::default();
    let power_1 = Power::from(10 as u64);
    let power_2 = Power::from(5 as u64);

    // The first staker proposing the first block wins because his power is higher or vrf and block hash are lower
    for power in &[power_1, power_2] {
        for vrf in &[vrf_1, vrf_2] {
            for bh in &[bh_1, bh_2] {
                let ordering = if *power == power_2 && *vrf == vrf_2 && *bh == bh_2 {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                };
                assert_eq!(
                    compare_block_candidates(
                        *bh,
                        rep_1,
                        *vrf,
                        true,
                        *power,
                        bh_2,
                        rep_2,
                        vrf_2,
                        true,
                        power_2,
                        &vrf_sections,
                        ProtocolVersion::V2_0,
                    ),
                    ordering
                );
            }
        }
    }

    // The second staker proposing the second block wins because his power is higher or vrf and block hash are lower
    for power in &[power_1, power_2] {
        for vrf in &[vrf_1, vrf_2] {
            for bh in &[bh_1, bh_2] {
                let ordering = if *power == power_2 && *vrf == vrf_2 && *bh == bh_2 {
                    Ordering::Equal
                } else {
                    Ordering::Less
                };
                assert_eq!(
                    compare_block_candidates(
                        bh_2,
                        rep_2,
                        vrf_2,
                        true,
                        power_2,
                        *bh,
                        rep_1,
                        *vrf,
                        true,
                        *power,
                        &vrf_sections,
                        ProtocolVersion::V2_0,
                    ),
                    ordering
                );
            }
        }
    }
}
