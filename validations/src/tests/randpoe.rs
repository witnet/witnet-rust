use witnet_data_structures::{
    chain::Hash,
    mainnet_validations::{ActiveWips, SECOND_HARD_FORK},
};

use crate::{tests::all_wips_active, validations::*};

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation)]
#[test]
fn target_randpoe() {
    // This test is only valid before the first hard fork
    let a = ActiveWips {
        active_wips: Default::default(),
        block_epoch: 1001,
    };
    let rf = 1;
    let minimum_difficulty = 2000;
    let max_hash = Hash::with_first_u32(0xFFFF_FFFF);
    let (t00, p00) = calculate_randpoe_threshold(0, rf, 1001, minimum_difficulty, 0, &a);
    let (t01, p01) = calculate_randpoe_threshold(1, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t00, max_hash);
    assert_eq!(t00, t01);
    assert_eq!((p00 * 100_f64).round() as i128, 100);
    assert_eq!(
        (p00 * 100_f64).round() as i128,
        (p01 * 100_f64).round() as i128
    );
    let (t02, p02) = calculate_randpoe_threshold(2, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t02, Hash::with_first_u32(0x7FFF_FFFF));
    assert_eq!((p02 * 100_f64).round() as i128, 50);
    let (t03, p03) = calculate_randpoe_threshold(3, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t03, Hash::with_first_u32(0x5555_5555));
    assert_eq!((p03 * 100_f64).round() as i128, 33);
    let (t04, p04) = calculate_randpoe_threshold(4, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t04, Hash::with_first_u32(0x3FFF_FFFF));
    assert_eq!((p04 * 100_f64).round() as i128, 25);
    let (t05, p05) = calculate_randpoe_threshold(1024, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t05, Hash::with_first_u32(0x003F_FFFF));
    assert_eq!((p05 * 100_f64).round() as i128, 0);
    let (t06, p06) = calculate_randpoe_threshold(1024 * 1024, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t06, Hash::with_first_u32(0x0000_0FFF));
    assert_eq!((p06 * 100_f64).round() as i128, 0);
}

#[allow(clippy::cast_possible_truncation)]
#[test]
fn target_randpoe_initial_difficulty() {
    // This test is only valid before the first hard fork
    let a = ActiveWips {
        active_wips: Default::default(),
        block_epoch: 1,
    };
    let (t, p) = calculate_randpoe_threshold(2, 1, 1, 4, 10, &a);
    assert_eq!(t, Hash::with_first_u32(0x3FFF_FFFF));
    assert_eq!((p * 100_f64).round() as i128, 25);

    let (t, p) = calculate_randpoe_threshold(2, 1, 11, 4, 10, &a);
    assert_eq!(t, Hash::with_first_u32(0x7FFF_FFFF));
    assert_eq!((p * 100_f64).round() as i128, 50);
}

#[allow(clippy::cast_possible_truncation, clippy::float_cmp)]
#[test]
fn target_randpoe_minimum_difficulty() {
    let replication_factor = 2;
    let minimum_difficulty = 2000;
    // Before first hard fork
    let active_wips = ActiveWips {
        active_wips: Default::default(),
        block_epoch: 1,
    };

    let total_identities = 1000;
    let expected_prob = (1_f64 / f64::from(total_identities)) * f64::from(replication_factor);
    assert_eq!(expected_prob, 0.002_f64);
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        1,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_eq!((p * 10000_f64).round() / 10000_f64, expected_prob);

    // After second hard fork, minimum probability is used
    let active_wips = all_wips_active();
    let block_epoch = SECOND_HARD_FORK + 1;
    let minimum_expected_prob =
        (1_f64 / f64::from(minimum_difficulty)) * f64::from(replication_factor);
    assert_eq!(minimum_expected_prob, 0.001_f64);
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        block_epoch,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_eq!((p * 10000_f64).round() / 10000_f64, minimum_expected_prob);

    let total_identities = 1500;
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        block_epoch,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_eq!((p * 10000_f64).round() / 10000_f64, minimum_expected_prob);

    let total_identities = minimum_difficulty - 1;
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        block_epoch,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_eq!((p * 10000_f64).round() / 10000_f64, minimum_expected_prob);

    // When achieves a number of identities equals to minimum difficulty,
    // the calculated probability is equals to the minimum
    let total_identities = minimum_difficulty;
    let expected_prob = (1_f64 / f64::from(total_identities)) * f64::from(replication_factor);
    assert_eq!(expected_prob, minimum_expected_prob);
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        block_epoch,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_eq!((p * 10000_f64).round() / 10000_f64, expected_prob);

    // After that, the probability starts to decrease
    let total_identities = minimum_difficulty + 1;
    let expected_prob = (1_f64 / f64::from(total_identities)) * f64::from(replication_factor);
    let expected_prob = (expected_prob * 1_000_000_000_f64).round() / 1_000_000_000_f64;
    assert_eq!(expected_prob, 0.0009995_f64);
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        block_epoch,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_eq!(
        (p * 1_000_000_000_f64).round() / 1_000_000_000_f64,
        expected_prob
    );
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation)]
#[test]
fn target_randpoe_rf_4() {
    let rf = 4;
    let minimum_difficulty = 2000;
    let max_hash = Hash::with_first_u32(0xFFFF_FFFF);
    // This test is only valid before the first hard fork
    let a = ActiveWips {
        active_wips: Default::default(),
        block_epoch: 1001,
    };
    let (t00, p00) = calculate_randpoe_threshold(0, rf, 1001, minimum_difficulty, 0, &a);
    let (t01, p01) = calculate_randpoe_threshold(1, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t00, max_hash);
    assert_eq!(t01, max_hash);
    assert_eq!((p00 * 100_f64).round() as i128, 100);
    assert_eq!((p01 * 100_f64).round() as i128, 100);
    let (t02, p02) = calculate_randpoe_threshold(2, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t02, max_hash);
    assert_eq!((p02 * 100_f64).round() as i128, 100);
    let (t03, p03) = calculate_randpoe_threshold(3, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t03, max_hash);
    assert_eq!((p03 * 100_f64).round() as i128, 100);
    let (t04, p04) = calculate_randpoe_threshold(4, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t04, max_hash);
    assert_eq!((p04 * 100_f64).round() as i128, 100);
    let (t05, p05) = calculate_randpoe_threshold(1024, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t05, Hash::with_first_u32(0x00FF_FFFF));
    assert_eq!((p05 * 100_f64).round() as i128, 0);
    let (t06, p06) = calculate_randpoe_threshold(1024 * 1024, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t06, Hash::with_first_u32(0x0000_3FFF));
    assert_eq!((p06 * 100_f64).round() as i128, 0);
}

#[test]
fn vrf_sections() {
    let h0 = Hash::default();
    let h1 = Hash::with_first_u32(1);
    let h2 = Hash::with_first_u32(2);
    let h3 = Hash::with_first_u32(3);
    let a = VrfSlots::new(vec![]);
    assert_eq!(a.slot(&h0), 0);

    let a = VrfSlots::new(vec![h0]);
    assert_eq!(a.slot(&h0), 0);
    assert_eq!(a.slot(&h1), 1);

    let a = VrfSlots::new(vec![h0, h2]);
    assert_eq!(a.slot(&h0), 0);
    assert_eq!(a.slot(&h1), 1);
    assert_eq!(a.slot(&h2), 1);
    assert_eq!(a.slot(&h3), 2);
}
