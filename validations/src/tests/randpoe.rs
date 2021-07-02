use approx::assert_abs_diff_eq;
use std::convert::TryFrom;
use witnet_data_structures::{
    chain::{Alpha, Hash, PublicKeyHash, Reputation, ReputationEngine},
    mainnet_validations::{ActiveWips, SECOND_HARD_FORK},
};

use crate::{tests::all_wips_active, validations::*};

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
    assert_abs_diff_eq!(p00, 1.0, epsilon = 1e-9);
    assert_abs_diff_eq!(p00, p01);
    let (t02, p02) = calculate_randpoe_threshold(2, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t02, Hash::with_first_u32(0x7FFF_FFFF));
    assert_abs_diff_eq!(p02, 0.5, epsilon = 1e-9);
    let (t03, p03) = calculate_randpoe_threshold(3, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t03, Hash::with_first_u32(0x5555_5555));
    assert_abs_diff_eq!(p03, 1.0 / 3.0, epsilon = 1e-9);
    let (t04, p04) = calculate_randpoe_threshold(4, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t04, Hash::with_first_u32(0x3FFF_FFFF));
    assert_abs_diff_eq!(p04, 0.25, epsilon = 1e-9);
    let (t05, p05) = calculate_randpoe_threshold(1024, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t05, Hash::with_first_u32(0x003F_FFFF));
    assert_abs_diff_eq!(p05, 1.0 / 1024.0, epsilon = 1e-9);
    let (t06, p06) = calculate_randpoe_threshold(1024 * 1024, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t06, Hash::with_first_u32(0x0000_0FFF));
    assert_abs_diff_eq!(p06, 1.0 / (1024.0 * 1024.0), epsilon = 1e-9);
}

#[test]
fn target_randpoe_initial_difficulty() {
    // This test is only valid before the first hard fork
    let a = ActiveWips {
        active_wips: Default::default(),
        block_epoch: 1,
    };
    let (t, p) = calculate_randpoe_threshold(2, 1, 1, 4, 10, &a);
    assert_eq!(t, Hash::with_first_u32(0x3FFF_FFFF));
    assert_abs_diff_eq!(p, 0.25, epsilon = 1e-9);

    let (t, p) = calculate_randpoe_threshold(2, 1, 11, 4, 10, &a);
    assert_eq!(t, Hash::with_first_u32(0x7FFF_FFFF));
    assert_abs_diff_eq!(p, 0.5, epsilon = 1e-9);
}

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
    assert_abs_diff_eq!(expected_prob, 0.002, epsilon = 1e-9);
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        1,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_abs_diff_eq!(p, expected_prob, epsilon = 1e-9);

    // After second hard fork, minimum probability is used
    let active_wips = all_wips_active();
    let block_epoch = SECOND_HARD_FORK + 1;
    let minimum_expected_prob =
        (1_f64 / f64::from(minimum_difficulty)) * f64::from(replication_factor);
    assert_abs_diff_eq!(minimum_expected_prob, 0.001, epsilon = 1e-9);
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        block_epoch,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_abs_diff_eq!(p, minimum_expected_prob, epsilon = 1e-9);

    let total_identities = 1500;
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        block_epoch,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_abs_diff_eq!(p, minimum_expected_prob, epsilon = 1e-9);

    let total_identities = minimum_difficulty - 1;
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        block_epoch,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_abs_diff_eq!(p, minimum_expected_prob, epsilon = 1e-9);

    // When achieves a number of identities equals to minimum difficulty,
    // the calculated probability is equals to the minimum
    let total_identities = minimum_difficulty;
    let expected_prob = (1_f64 / f64::from(total_identities)) * f64::from(replication_factor);
    assert_abs_diff_eq!(expected_prob, minimum_expected_prob, epsilon = 1e-9);
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        block_epoch,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_abs_diff_eq!(p, expected_prob, epsilon = 1e-9);

    // After that, the probability starts to decrease
    let total_identities = minimum_difficulty + 1;
    let expected_prob = (1_f64 / f64::from(total_identities)) * f64::from(replication_factor);
    let expected_prob = (expected_prob * 1_000_000_000_f64).round() / 1_000_000_000_f64;
    assert_abs_diff_eq!(expected_prob, 0.0009995_f64, epsilon = 1e-9);
    let (_, p) = calculate_randpoe_threshold(
        total_identities,
        replication_factor,
        block_epoch,
        minimum_difficulty,
        0,
        &active_wips,
    );
    assert_abs_diff_eq!(p, expected_prob, epsilon = 1e-9);
}

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
    assert_abs_diff_eq!(p00, 1.0, epsilon = 1e-9);
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    let (t02, p02) = calculate_randpoe_threshold(2, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t02, max_hash);
    assert_abs_diff_eq!(p02, 1.0, epsilon = 1e-9);
    let (t03, p03) = calculate_randpoe_threshold(3, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t03, max_hash);
    assert_abs_diff_eq!(p03, 1.0, epsilon = 1e-9);
    let (t04, p04) = calculate_randpoe_threshold(4, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t04, max_hash);
    assert_abs_diff_eq!(p04, 1.0, epsilon = 1e-9);
    let (t05, p05) = calculate_randpoe_threshold(1024, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t05, Hash::with_first_u32(0x00FF_FFFF));
    assert_abs_diff_eq!(p05, 4.0 / 1024.0, epsilon = 1e-9);
    let (t06, p06) = calculate_randpoe_threshold(1024 * 1024, rf, 1001, minimum_difficulty, 0, &a);
    assert_eq!(t06, Hash::with_first_u32(0x0000_3FFF));
    assert_abs_diff_eq!(p06, 4.0 / (1024.0 * 1024.0), epsilon = 1e-9);
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

fn init_rep_engine(v_rep: Vec<u32>) -> (ReputationEngine, Vec<PublicKeyHash>) {
    let mut rep_engine = ReputationEngine::new(1000);

    let mut ids = vec![];
    for (i, &rep) in v_rep.iter().enumerate() {
        let pkh = PublicKeyHash::from_bytes(&[u8::try_from(i).unwrap(); 20]).unwrap();
        rep_engine
            .trs_mut()
            .gain(Alpha(10), vec![(pkh, Reputation(rep))])
            .unwrap();
        ids.push(pkh);
    }
    rep_engine.ars_mut().push_activity(ids.clone());

    (rep_engine, ids)
}

fn calculate_mining_probs(v_rep: Vec<u32>, rf: u32, bf: u32) -> (Vec<f64>, f64) {
    let v_rep_len = v_rep.len();
    let (rep_engine, ids) = init_rep_engine(v_rep);
    let n = rep_engine.ars().active_identities_number();
    assert_eq!(n, v_rep_len);
    assert_eq!(ids.len(), v_rep_len);

    let mut probs = vec![];
    for id in ids {
        probs.push(calculate_mining_probability(&rep_engine, id, rf, bf))
    }

    let new_pkh = PublicKeyHash::from_bytes(&[0xFF; 20]).unwrap();
    let new_prob = calculate_mining_probability(&rep_engine, new_pkh, rf, bf);

    (probs, new_prob)
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[test]
fn calculate_mining_probabilities_rf1_bf1() {
    let v_rep = vec![10, 8, 8, 8, 5, 5, 5, 5, 0, 0];
    let (probs, new_prob) = calculate_mining_probs(v_rep, 1, 1);

    for &prob in &probs[0..8] {
        assert_eq!(
            (prob * 10_000.0).round() as u32,
            (7.12_f64 * 100.0).round() as u32
        );
    }

    for &prob in &probs[8..10] {
        assert_eq!(
            (prob * 10_000.0).round() as u32,
            (4.09_f64 * 100.0).round() as u32
        );
    }

    assert_eq!(
        (new_prob * 10_000.0).round() as u32,
        (3.89_f64 * 100.0).round() as u32
    );
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[test]
fn calculate_mining_probabilities_rf1_bf2() {
    let v_rep = vec![10, 8, 8, 8, 5, 5, 5, 5, 2, 2];
    let (probs, new_prob) = calculate_mining_probs(v_rep, 1, 2);

    for prob in probs {
        assert_eq!(
            (prob * 10_000.0).round() as u32,
            (9.04_f64 * 100.0).round() as u32
        );
    }

    assert_eq!(
        (new_prob * 10_000.0).round() as u32,
        (4.56_f64 * 100.0).round() as u32
    );
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[test]
fn calculate_mining_probabilities_rf2_bf2() {
    let v_rep = vec![10, 8, 8, 8, 5, 5, 5, 5, 2, 2];
    let (probs, new_prob) = calculate_mining_probs(v_rep, 2, 2);

    for prob in probs {
        assert_eq!(
            (prob * 10_000.0).round() as u32,
            (8.93_f64 * 100.0).round() as u32
        );
    }

    assert_eq!(
        (new_prob * 10_000.0).round() as u32,
        (2.15_f64 * 100.0).round() as u32
    );
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[test]
fn calculate_mining_probabilities_rf4_bf8() {
    let v_rep = vec![10, 8, 8, 8, 5, 5, 5, 5, 2, 2];
    let (probs, new_prob) = calculate_mining_probs(v_rep, 4, 8);

    for prob in probs {
        assert_eq!(
            (prob * 10_000.0).round() as u32,
            (10.02_f64 * 100.0).round() as u32
        );
    }

    assert_eq!(
        (new_prob * 10_000.0).round() as u32,
        (0.25_f64 * 100.0).round() as u32
    );
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[test]
fn calculate_mining_probabilities_rf1_bf1_10() {
    let v_rep = vec![10; 10];
    let (probs, new_prob) = calculate_mining_probs(v_rep, 1, 1);

    assert_eq!(
        (probs[0] * 10_000.0).round() as u32,
        (6.51_f64 * 100.0).round() as u32
    );

    assert_eq!(
        (new_prob * 10_000.0).round() as u32,
        (3.49_f64 * 100.0).round() as u32
    );
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[test]
fn calculate_mining_probabilities_rf1_bf1_100() {
    let v_rep = vec![10; 100];
    let (probs, new_prob) = calculate_mining_probs(v_rep, 1, 1);

    assert_eq!(
        (probs[0] * 10_000.0).round() as u32,
        (0.63_f64 * 100.0).round() as u32
    );

    assert_eq!(
        (new_prob * 10_000.0).round() as u32,
        (0.37_f64 * 100.0).round() as u32
    );
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[test]
fn calculate_mining_probabilities_rf4_bf8_100() {
    let v_rep = vec![10; 100];
    let (probs, new_prob) = calculate_mining_probs(v_rep, 4, 8);

    assert_eq!(
        (probs[0] * 10_000.0).round() as u32,
        (1.0_f64 * 100.0).round() as u32
    );

    assert_eq!(
        (new_prob * 10_000.0).round() as u32,
        (0.08_f64 * 100.0).round() as u32
    );
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[test]
fn calculate_mining_probabilities_rf4_bf8_100_diff() {
    let mut v_rep = vec![10; 25];
    v_rep.extend(vec![8; 25]);
    v_rep.extend(vec![6; 25]);
    v_rep.extend(vec![4; 25]);
    let (probs, new_prob) = calculate_mining_probs(v_rep, 4, 8);

    assert_eq!(
        (probs[0] * 10_000.0).round() as u32,
        (1_f64 * 100.0).round() as u32
    );
    assert_eq!(
        (probs[25] * 10_000.0).round() as u32,
        (1_f64 * 100.0).round() as u32
    );
    assert_eq!(
        (probs[50] * 10_000.0).round() as u32,
        (1_f64 * 100.0).round() as u32
    );
    assert_eq!(
        (probs[75] * 10_000.0).round() as u32,
        (1_f64 * 100.0).round() as u32
    );

    assert_eq!(
        (new_prob * 10_000.0).round() as u32,
        (0.08_f64 * 100.0).round() as u32
    );
}

// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[test]
fn calculate_mining_probabilities_rf_high() {
    let v_rep = vec![10, 8, 8, 2];
    let (probs, new_prob) = calculate_mining_probs(v_rep, 4, 8);

    for prob in probs {
        assert_eq!(
            (prob * 10_000.0).round() as u32,
            (25_f64 * 100.0).round() as u32
        );
    }

    assert_eq!(
        (new_prob * 10_000.0).round() as u32,
        (0_f64 * 100.0).round() as u32
    );
}
