use witnet_data_structures::chain::{tapi::current_active_wips, Hash, Reputation};

use std::cmp::Ordering;

use crate::validations::*;

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
                                    bh_j,
                                    rep_2,
                                    vrf_j,
                                    act_j,
                                    &vrf_sections,
                                ),
                                Ordering::Less
                            );
                            assert_eq!(
                                compare_block_candidates(
                                    bh_i,
                                    rep_2,
                                    vrf_i,
                                    act_i,
                                    bh_j,
                                    rep_1,
                                    vrf_j,
                                    act_j,
                                    &vrf_sections,
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
                            bh_j,
                            rep_1,
                            vrf_j,
                            false,
                            &vrf_sections,
                        ),
                        Ordering::Greater
                    );
                    assert_eq!(
                        compare_block_candidates(
                            bh_i,
                            rep_2,
                            vrf_i,
                            false,
                            bh_j,
                            rep_2,
                            vrf_j,
                            true,
                            &vrf_sections,
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
                    bh_j,
                    rep_1,
                    vrf_2,
                    true,
                    &vrf_sections,
                ),
                Ordering::Greater
            );
            assert_eq!(
                compare_block_candidates(
                    bh_i,
                    rep_1,
                    vrf_2,
                    true,
                    bh_j,
                    rep_1,
                    vrf_1,
                    true,
                    &vrf_sections,
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
            bh_2,
            rep_1,
            vrf_1,
            true,
            &vrf_sections,
        ),
        Ordering::Greater
    );
    assert_eq!(
        compare_block_candidates(
            bh_2,
            rep_1,
            vrf_1,
            true,
            bh_1,
            rep_1,
            vrf_1,
            true,
            &vrf_sections,
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
            bh_1,
            rep_1,
            vrf_1,
            true,
            &vrf_sections,
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
                                    bh_j,
                                    rep_j,
                                    vrf_2,
                                    act_j,
                                    &vrf_sections,
                                ),
                                Ordering::Greater
                            );
                            assert_eq!(
                                compare_block_candidates(
                                    bh_i,
                                    rep_i,
                                    vrf_2,
                                    act_i,
                                    bh_j,
                                    rep_j,
                                    vrf_1,
                                    act_j,
                                    &vrf_sections,
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

    // In case of active nodes with reputation, the difference will be the vrf not the reputation
    assert_eq!(
        compare_block_candidates(
            bh_1,
            rep_1,
            vrf_1,
            true,
            bh_2,
            rep_2,
            vrf_2,
            true,
            &vrf_sections,
        ),
        Ordering::Greater
    );

    assert_eq!(
        compare_block_candidates(
            bh_1,
            rep_1,
            vrf_2,
            true,
            bh_2,
            rep_2,
            vrf_1,
            true,
            &vrf_sections,
        ),
        Ordering::Less
    );
}
