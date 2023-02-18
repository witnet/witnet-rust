use approx::assert_abs_diff_eq;
use witnet_data_structures::{
    chain::{
        calculate_backup_witnesses,
        tapi::{current_active_wips, ActiveWips},
        Alpha, Hash, PublicKeyHash, Reputation, ReputationEngine,
    },
    transaction::DRTransaction,
};

use crate::validations::*;

fn calculate_reppoe_threshold_v1(
    rep_eng: &ReputationEngine,
    pkh: &PublicKeyHash,
    num_witnesses: u16,
    minimum_difficulty: u32,
) -> (Hash, f64) {
    let active_wips = ActiveWips::default();
    assert!(!active_wips.wip0016());
    assert!(!active_wips.third_hard_fork());
    calculate_reppoe_threshold(
        rep_eng,
        pkh,
        num_witnesses,
        minimum_difficulty,
        &active_wips,
    )
}

fn calculate_reppoe_threshold_v2(
    rep_eng: &ReputationEngine,
    pkh: &PublicKeyHash,
    num_witnesses: u16,
    minimum_difficulty: u32,
) -> (Hash, f64) {
    let mut active_wips = ActiveWips::default();
    active_wips
        .active_wips
        .insert("THIRD_HARD_FORK".to_string(), 0);
    assert!(!active_wips.wip0016());
    assert!(active_wips.third_hard_fork());
    calculate_reppoe_threshold(
        rep_eng,
        pkh,
        num_witnesses,
        minimum_difficulty,
        &active_wips,
    )
}

fn calculate_reppoe_threshold_v3(
    rep_eng: &ReputationEngine,
    pkh: &PublicKeyHash,
    num_witnesses: u16,
    minimum_difficulty: u32,
) -> (Hash, f64) {
    let active_wips = current_active_wips();
    assert!(active_wips.wip0016());
    assert!(active_wips.third_hard_fork());
    calculate_reppoe_threshold(
        rep_eng,
        pkh,
        num_witnesses,
        minimum_difficulty,
        &active_wips,
    )
}

// Auxiliar function to add reputation
fn add_rep(rep_engine: &mut ReputationEngine, alpha: u32, pkh: PublicKeyHash, rep: u32) {
    rep_engine
        .trs_mut()
        .gain(Alpha(alpha), vec![(pkh, Reputation(rep))])
        .unwrap();
}

#[test]
fn target_reppoe_v1() {
    let mut rep_engine = ReputationEngine::new(1000);
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    add_rep(&mut rep_engine, 10, id1, 50);
    rep_engine.ars_mut().push_activity(vec![id1]);

    // 100% when we have all the reputation
    let (t00, p00) = calculate_reppoe_threshold_v1(&rep_engine, &id1, 1, 2000);
    assert_eq!(t00, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p00, 1.0, epsilon = 1e-9);

    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    add_rep(&mut rep_engine, 10, id2, 50);
    rep_engine.ars_mut().push_activity(vec![id2]);

    // 50% when there are 2 nodes with 50% of the reputation each
    let (t01, p01) = calculate_reppoe_threshold_v1(&rep_engine, &id1, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0x7FFF_FFFF));
    assert_abs_diff_eq!(p01, 0.5, epsilon = 1e-9);
}

#[test]
fn target_reppoe_v2() {
    let mut rep_engine = ReputationEngine::new(1000);
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    add_rep(&mut rep_engine, 10, id1, 50);
    rep_engine.ars_mut().push_activity(vec![id1]);

    // 0.05% when the total reputation is less than 2000
    let (t00, p00) = calculate_reppoe_threshold_v2(&rep_engine, &id1, 1, 2000);
    assert_eq!(t00, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p00, 1.0 / 2000.00, epsilon = 1e-9);

    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    add_rep(&mut rep_engine, 10, id2, 50);
    rep_engine.ars_mut().push_activity(vec![id2]);

    // 0.05% when the total reputation is less than 2000
    let (t01, p01) = calculate_reppoe_threshold_v2(&rep_engine, &id1, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p01, 1.0 / 2000.00, epsilon = 1e-9);
}

#[test]
fn target_reppoe_v3() {
    let mut rep_engine = ReputationEngine::new(1000);
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    add_rep(&mut rep_engine, 10, id1, 50);
    rep_engine.ars_mut().push_activity(vec![id1]);

    // 0.05% * 51 when the total reputation is less than 2000
    // 51 is the reputation of this node (50 + 1 because active)
    let (t00, p00) = calculate_reppoe_threshold_v3(&rep_engine, &id1, 1, 2000);
    // 0xFFFF_FFFF / 2000 * 51
    assert_eq!(t00, Hash::with_first_u32(0x06872b02));
    assert_abs_diff_eq!(p00, 51.0 / 2000.00, epsilon = 1e-9);

    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    add_rep(&mut rep_engine, 10, id2, 50);
    rep_engine.ars_mut().push_activity(vec![id2]);

    // 0.05% * 51 when the total reputation is less than 2000
    // 51 is the reputation of this node (50 + 1 because active)
    let (t01, p01) = calculate_reppoe_threshold_v3(&rep_engine, &id1, 1, 2000);
    // 0xFFFF_FFFF / 2000 * 51
    assert_eq!(t01, Hash::with_first_u32(0x06872b02));
    assert_abs_diff_eq!(p01, 51.0 / 2000.00, epsilon = 1e-9);
}

#[test]
// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(
    clippy::cast_possible_truncation,
    clippy::cognitive_complexity,
    clippy::cast_sign_loss
)]
fn target_reppoe_specific_example_v1() {
    let mut rep_engine = ReputationEngine::new(1000);
    let mut ids = vec![];
    for i in 0..8 {
        ids.push(PublicKeyHash::from_bytes(&[i; 20]).unwrap());
    }
    rep_engine.ars_mut().push_activity(ids.clone());

    add_rep(&mut rep_engine, 10, ids[0], 79);
    add_rep(&mut rep_engine, 10, ids[1], 9);
    add_rep(&mut rep_engine, 10, ids[2], 1);
    add_rep(&mut rep_engine, 10, ids[3], 1);
    add_rep(&mut rep_engine, 10, ids[4], 1);
    add_rep(&mut rep_engine, 10, ids[5], 1);

    let rep_thresholds = |thres| {
        let mut v = vec![];
        for id in ids.iter() {
            v.push(
                (calculate_reppoe_threshold_v1(&rep_engine, id, thres, 2000).1 * 1_000_000_f64)
                    .round() as u32,
            );
        }
        v
    };

    // Doesnt work, will need to compare items one by one
    //assert_abs_diff_eq!(vec![1.0f64, 2.0, 3.0], vec![1.0f64, 2.0, 3.1]);

    assert_eq!(
        rep_thresholds(1),
        vec![280_000, 230_000, 190_000, 50_000, 140_000, 90_000, 10_000, 10_000]
    );
    assert_eq!(
        rep_thresholds(2),
        vec![560_000, 460_000, 380_000, 100_000, 280_000, 180_000, 20_000, 20_000]
    );
    assert_eq!(
        rep_thresholds(3),
        vec![840_000, 690_000, 570_000, 150_000, 420_000, 270_000, 30_000, 30_000]
    );
    assert_eq!(
        rep_thresholds(4),
        vec![1_000_000, 1_000_000, 950_000, 250_000, 700_000, 450_000, 50_000, 50_000]
    );
    assert_eq!(
        rep_thresholds(5),
        vec![1_000_000, 1_000_000, 1_000_000, 350_000, 980_000, 630_000, 70_000, 70_000]
    );
    assert_eq!(
        rep_thresholds(6),
        vec![1_000_000, 1_000_000, 1_000_000, 750_000, 1_000_000, 1_000_000, 150_000, 150_000]
    );
    assert_eq!(
        rep_thresholds(7),
        vec![1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 500_000, 500_000]
    );
    assert_eq!(rep_thresholds(8), vec![1_000_000; 8]);
    assert_eq!(rep_thresholds(9), vec![1_000_000; 8]);
    assert_eq!(rep_thresholds(10), vec![1_000_000; 8]);
}

#[test]
// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(
    clippy::cast_possible_truncation,
    clippy::cognitive_complexity,
    clippy::cast_sign_loss
)]
fn target_reppoe_specific_example_v2() {
    let mut rep_engine = ReputationEngine::new(1000);
    let mut ids = vec![];
    for i in 0..8 {
        ids.push(PublicKeyHash::from_bytes(&[i; 20]).unwrap());
    }
    rep_engine.ars_mut().push_activity(ids.clone());

    add_rep(&mut rep_engine, 10, ids[0], 79);
    add_rep(&mut rep_engine, 10, ids[1], 9);
    add_rep(&mut rep_engine, 10, ids[2], 1);
    add_rep(&mut rep_engine, 10, ids[3], 1);
    add_rep(&mut rep_engine, 10, ids[4], 1);
    add_rep(&mut rep_engine, 10, ids[5], 1);

    let rep_thresholds = |thres| {
        let mut v = vec![];
        for id in ids.iter() {
            v.push(
                (calculate_reppoe_threshold_v2(&rep_engine, id, thres, 2000).1 * 1_000_000_f64)
                    .round() as u32,
            );
        }
        v
    };

    assert_eq!(rep_thresholds(1), vec![500; 8]);
    assert_eq!(rep_thresholds(2), vec![1000; 8]);
    assert_eq!(rep_thresholds(3), vec![1500; 8]);
    assert_eq!(rep_thresholds(4), vec![2500; 8]);
    assert_eq!(rep_thresholds(5), vec![3500; 8]);
    assert_eq!(rep_thresholds(6), vec![7500; 8]);
    assert_eq!(rep_thresholds(7), vec![25_000; 8]);
    assert_eq!(rep_thresholds(8), vec![50_000; 8]);
    assert_eq!(rep_thresholds(9), vec![1_000_000; 8]);
    assert_eq!(rep_thresholds(10), vec![1_000_000; 8]);
}

#[test]
// FIXME: Allow for now, wait for https://github.com/rust-lang/rust/issues/67058 to reach stable
#[allow(
    clippy::cast_possible_truncation,
    clippy::cognitive_complexity,
    clippy::cast_sign_loss
)]
fn target_reppoe_specific_example_v3() {
    let mut rep_engine = ReputationEngine::new(1000);
    let mut ids = vec![];
    for i in 0..8 {
        ids.push(PublicKeyHash::from_bytes(&[i; 20]).unwrap());
    }
    rep_engine.ars_mut().push_activity(ids.clone());

    add_rep(&mut rep_engine, 10, ids[0], 79);
    add_rep(&mut rep_engine, 10, ids[1], 9);
    add_rep(&mut rep_engine, 10, ids[2], 1);
    add_rep(&mut rep_engine, 10, ids[3], 1);
    add_rep(&mut rep_engine, 10, ids[4], 1);
    add_rep(&mut rep_engine, 10, ids[5], 1);

    let rep_thresholds = |thres| {
        let mut v = vec![];
        for id in ids.iter() {
            v.push(
                (calculate_reppoe_threshold_v3(&rep_engine, id, thres, 2000).1 * 1_000_000_f64)
                    .round() as u32,
            );
        }
        v
    };

    assert_eq!(
        rep_thresholds(1),
        vec![14_000, 11_500, 9500, 2500, 7000, 4500, 500, 500]
    );
    assert_eq!(
        rep_thresholds(2),
        vec![28_000, 23_000, 19_000, 5000, 14_000, 9000, 1000, 1000]
    );
    assert_eq!(
        rep_thresholds(3),
        vec![42_000, 34_500, 28_500, 7500, 21_000, 13_500, 1500, 1500]
    );
    assert_eq!(
        rep_thresholds(4),
        vec![56_000, 46_000, 38_000, 10_000, 28_000, 18_000, 2000, 2000]
    );
    assert_eq!(
        rep_thresholds(5),
        vec![70_000, 57_500, 47_500, 12_500, 35_000, 22_500, 2500, 2500]
    );
    assert_eq!(
        rep_thresholds(6),
        vec![84_000, 69_000, 57_000, 15_000, 42_000, 27_000, 3000, 3000]
    );
    assert_eq!(
        rep_thresholds(7),
        vec![98_000, 80_500, 66_500, 17_500, 49_000, 31_500, 3500, 3500]
    );
    assert_eq!(
        rep_thresholds(8),
        vec![112_000, 92000, 76_000, 20_000, 56_000, 36_000, 4000, 4000]
    );
    assert_eq!(
        rep_thresholds(9),
        vec![126_000, 103_500, 85_500, 22_500, 63_000, 40_500, 4500, 4500]
    );
    assert_eq!(
        rep_thresholds(10),
        vec![140_000, 115_000, 95_000, 25_000, 70_000, 45_000, 5000, 5000]
    );
}

#[test]
fn target_reppoe_zero_reputation_v1() {
    // Test the behavior of the algorithm when our node has 0 reputation
    let mut rep_engine = ReputationEngine::new(1000);
    let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();

    // 100% when the total reputation is 0
    let (t00, p00) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 1, 2000);
    assert_eq!(t00, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p00, 1.0, epsilon = 1e-9);
    let (t01, p01) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 100, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);

    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id1]);
    let (t02, p02) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p02, 1.0, epsilon = 1e-9);
    let (t02b, p02b) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 2, 2000);
    assert_eq!(t02b, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p02b, 1.0, epsilon = 1e-9);

    // 50% when the total reputation is 1
    add_rep(&mut rep_engine, 10, id1, 1);
    let (t03, p03) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 1, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x7FFF_FFFF));
    assert_abs_diff_eq!(p03, 0.5, epsilon = 1e-9);
    // 100% when the total reputation is 1
    // but the number of witnesses is greater than the number of active identities
    let (t03b, p03b) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 2, 2000);
    assert_eq!(t03b, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p03b, 1.0, epsilon = 1e-9);

    // 33% when the total reputation is 1 but there are 2 active identities
    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id2]);
    let (t04, p04) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 1, 2000);
    assert_eq!(t04, Hash::with_first_u32(0x5555_5555));
    assert_abs_diff_eq!(p04, 1.0 / 3.0, epsilon = 1e-9);
    // 100% when the total reputation is 1 but there are 2 active identities
    // but the number of witnesses is greater than the number of active identities
    let (t04b, p04b) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 2, 2000);
    assert_eq!(t04b, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p04b, 1.0, epsilon = 1e-9);
    // 100% when the total reputation is 1 but there are 2 active identities
    // but the number of witnesses is greater than the number of active identities
    let (t04c, p04c) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 3, 2000);
    assert_eq!(t04c, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p04c, 1.0, epsilon = 1e-9);

    // 10 identities with 100 total reputation: 1 / (100 + 10) ~= 0.9%
    for i in 3..=10 {
        rep_engine
            .ars_mut()
            .push_activity(vec![PublicKeyHash::from_bytes(&[i; 20]).unwrap()]);
    }
    add_rep(&mut rep_engine, 10, id1, 99);
    let (t05, p05) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 1, 2000);
    assert_eq!(t05, Hash::with_first_u32(0x0253_C825));
    assert_abs_diff_eq!(p05, 1.0 / (100.0 + 10.0), epsilon = 1e-9);

    // 10 identities with 10000 total reputation: 1 / (10000 + 10) ~= 0.01%
    add_rep(&mut rep_engine, 10, id1, 9900);
    let (t05b, p05b) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 1, 2000);
    assert_eq!(t05b, Hash::with_first_u32(0xFFFF_FFFF / (10_000 + 10)));
    assert_abs_diff_eq!(p05b, 1.0 / (10000.0 + 10.0), epsilon = 1e-9);
}

#[test]
fn target_reppoe_zero_reputation_v2() {
    // Test the behavior of the algorithm when our node has 0 reputation
    let mut rep_engine = ReputationEngine::new(1000);
    let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();

    // 100% when the total reputation is 0
    let (t00, p00) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 1, 2000);
    assert_eq!(t00, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p00, 1.0, epsilon = 1e-9);
    let (t01, p01) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 100, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);

    // 0.05% when the total reputation is 0 but there is 1 active identity
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id1]);
    let (t02, p02) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p02, 1.0 / 2000.0, epsilon = 1e-9);
    // 100% when the total reputation is 0 and there is 1 active identity,
    // but the number of witnesses is greater than the number of active identities
    let (t02b, p02b) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 2, 2000);
    assert_eq!(t02b, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p02b, 1.0, epsilon = 1e-9);

    // 0.05% when the total reputation is 1
    add_rep(&mut rep_engine, 10, id1, 1);
    let (t03, p03) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 1, 2000);
    assert_eq!(t03, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p03, 1.0 / 2000.0, epsilon = 1e-9);
    // 100% when the total reputation is 1
    // but the number of witnesses is greater than the number of active identities
    let (t03b, p03b) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 2, 2000);
    assert_eq!(t03b, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p03b, 1.0, epsilon = 1e-9);

    // 0.05% when the total reputation is 1 but there are 2 active identities
    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id2]);
    let (t04, p04) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 1, 2000);
    assert_eq!(t04, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p04, 1.0 / 2000.0, epsilon = 1e-9);
    // 0.15% when the total reputation is 1 but there are 2 active identities
    // but the number of witnesses is greater than 1
    let (t04b, p04b) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 2, 2000);
    // 0xFFFF_FFFF / 2000 * 3
    assert_eq!(t04b, Hash::with_first_u32(0x00624dd2));
    assert_abs_diff_eq!(p04b, 3.0 / 2000.0, epsilon = 1e-9);
    // 100% when the total reputation is 1 but there are 2 active identities
    // but the number of witnesses is greater than the number of active identities
    let (t04c, p04c) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 3, 2000);
    assert_eq!(t04c, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p04c, 1.0, epsilon = 1e-9);

    // 10 identities with 100 total reputation: 0.15% (100 is less than the minimum of 2000)
    for i in 3..=10 {
        rep_engine
            .ars_mut()
            .push_activity(vec![PublicKeyHash::from_bytes(&[i; 20]).unwrap()]);
    }
    add_rep(&mut rep_engine, 10, id1, 99);
    let (t05, p05) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 1, 2000);
    assert_eq!(t05, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p05, 1.0 / 2000.0, epsilon = 1e-9);

    // 10 identities with 10000 total reputation: 1 / (10000 + 10) ~= 0.01%
    add_rep(&mut rep_engine, 10, id1, 9900);
    let (t05b, p05b) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 1, 2000);
    assert_eq!(t05b, Hash::with_first_u32(0xFFFF_FFFF / (10_000 + 10)));
    assert_abs_diff_eq!(p05b, 1.0 / (10000.0 + 10.0), epsilon = 1e-9);
}

#[test]
fn target_reppoe_zero_reputation_v3() {
    // Test the behavior of the algorithm when our node has 0 reputation
    let mut rep_engine = ReputationEngine::new(1000);
    let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();

    // 0.05% when the total reputation is 0
    let (t00, p00) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 1, 2000);
    assert_eq!(t00, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p00, 1.0 / 2000.0, epsilon = 1e-9);
    // 5% when the total reputation is 0 but the number of witnesses is 100
    let (t01, p01) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 100, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF / (2000 / 100)));
    assert_abs_diff_eq!(p01, 100.0 / 2000.0, epsilon = 1e-9);

    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id1]);
    let (t02, p02) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p02, 1.0 / 2000.0, epsilon = 1e-9);
    // 0.10% when the total reputation is 0 and there is 1 active identity,
    // but the number of witnesses is 2
    let (t02b, p02b) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 2, 2000);
    // 0xFFFF_FFFF / 2000 * 2
    assert_eq!(t02b, Hash::with_first_u32(0x00418937));
    assert_abs_diff_eq!(p02b, 2.0 / 2000.0, epsilon = 1e-9);

    // 0.05% when the total reputation is 1
    add_rep(&mut rep_engine, 10, id1, 1);
    let (t03, p03) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 1, 2000);
    assert_eq!(t03, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p03, 1.0 / 2000.0, epsilon = 1e-9);
    // 0.10% when the total reputation is 1
    // but the number of witnesses is greater than the number of active identities
    let (t03b, p03b) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 2, 2000);
    // 0xFFFF_FFFF / 2000 * 2 + 1
    assert_eq!(t03b, Hash::with_first_u32(0x00418937));
    assert_abs_diff_eq!(p03b, 2.0 / 2000.0, epsilon = 1e-9);

    // When the total reputation is 1 but there are 2 active identities:
    // 0.05% when the number of witnesses is 1
    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id2]);
    let (t04, p04) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 1, 2000);
    assert_eq!(t04, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p04, 1.0 / 2000.0, epsilon = 1e-9);
    // 0.10% when the number of witnesses is 2
    let (t04b, p04b) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 2, 2000);
    // 0xFFFF_FFFF / 2000 * 2
    assert_eq!(t04b, Hash::with_first_u32(0x00418937));
    assert_abs_diff_eq!(p04b, 2.0 / 2000.0, epsilon = 1e-9);
    // 0.15% when the number of witnesses is 3
    let (t04c, p04c) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 3, 2000);
    // 0xFFFF_FFFF / 2000 * 3
    assert_eq!(t04c, Hash::with_first_u32(0x00624dd2));
    assert_abs_diff_eq!(p04c, 3.0 / 2000.0, epsilon = 1e-9);

    // 10 identities with 100 total reputation: 0.15% (100 is less than the minimum of 2000)
    for i in 3..=10 {
        rep_engine
            .ars_mut()
            .push_activity(vec![PublicKeyHash::from_bytes(&[i; 20]).unwrap()]);
    }
    add_rep(&mut rep_engine, 10, id1, 99);
    let (t05, p05) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 1, 2000);
    assert_eq!(t05, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p05, 1.0 / 2000.0, epsilon = 1e-9);

    // 10 identities with 10000 total reputation: 1 / (10000 + 10) ~= 0.01%
    add_rep(&mut rep_engine, 10, id1, 9900);
    let (t05b, p05b) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 1, 2000);
    assert_eq!(t05b, Hash::with_first_u32(0xFFFF_FFFF / (10_000 + 10)));
    assert_abs_diff_eq!(p05b, 1.0 / (10000.0 + 10.0), epsilon = 1e-9);
}

#[test]
fn reppoe_overflow_v1() {
    // Test the behavior of the algorithm when one node has 99% of the reputation
    let mut rep_engine = ReputationEngine::new(1000);
    let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id0]);
    rep_engine.ars_mut().push_activity(vec![id1]);
    add_rep(&mut rep_engine, 10, id0, u32::max_value() - 2);

    // Test big values that result in < 100%
    // Active identity with 100% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFE));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Active identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v1(&rep_engine, &id1, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x00000001));
    assert_abs_diff_eq!(p02, 0.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v1(&rep_engine, &id2, 1, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x00000001));
    assert_abs_diff_eq!(p03, 0.0, epsilon = 1e-9);

    // Repeat test with 2 witnesses
    // Active identity with 100% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 2, 2000);
    // 100% eligibility because the number of witnesses is greater than the number of active identities
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Active identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v1(&rep_engine, &id1, 2, 2000);
    assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p02, 1.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v1(&rep_engine, &id2, 2, 2000);
    assert_eq!(t03, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p03, 1.0, epsilon = 1e-9);
}

#[test]
fn reppoe_overflow_v2() {
    // Test the behavior of the algorithm when one node has 99% of the reputation
    let mut rep_engine = ReputationEngine::new(1000);
    let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id0]);
    rep_engine.ars_mut().push_activity(vec![id1]);
    add_rep(&mut rep_engine, 10, id0, u32::max_value() - 2);

    // Test big values that result in < 100%
    // Active identity with 100% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p01, 1.0 / 2000.0, epsilon = 1e-9);
    // Active identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v2(&rep_engine, &id1, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x00000001));
    assert_abs_diff_eq!(p02, 0.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v2(&rep_engine, &id2, 1, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x00000001));
    assert_abs_diff_eq!(p03, 0.0, epsilon = 1e-9);

    // Repeat test with 2 witnesses
    // Active identity with 100% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 2, 2000);
    // 100% eligibility because the number of witnesses is greater than the number of active identities
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Active identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v2(&rep_engine, &id1, 2, 2000);
    assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p02, 1.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v2(&rep_engine, &id2, 2, 2000);
    assert_eq!(t03, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p03, 1.0, epsilon = 1e-9);
}

#[test]
fn reppoe_overflow_v3() {
    // Test the behavior of the algorithm when one node has 99% of the reputation
    let mut rep_engine = ReputationEngine::new(1000);
    let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id0]);
    rep_engine.ars_mut().push_activity(vec![id1]);
    add_rep(&mut rep_engine, 10, id0, u32::max_value() - 2);

    // Test big values that result in < 100%
    // Active identity with 100% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFE));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Active identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v3(&rep_engine, &id1, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x00000001));
    assert_abs_diff_eq!(p02, 0.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v3(&rep_engine, &id2, 1, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x00000001));
    assert_abs_diff_eq!(p03, 0.0, epsilon = 1e-9);

    // Repeat test with 2 witnesses
    // Active identity with 100% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 2, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Active identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v3(&rep_engine, &id1, 2, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x00000002));
    assert_abs_diff_eq!(p02, 0.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v3(&rep_engine, &id2, 2, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x00000002));
    assert_abs_diff_eq!(p03, 0.0, epsilon = 1e-9);
}

#[test]
fn reppoe_much_rep_trapezoid_v1() {
    // Test the behavior of the algorithm when one node has 99% reputation and another node has 1%
    let mut rep_engine = ReputationEngine::new(1000);
    let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id0]);
    rep_engine.ars_mut().push_activity(vec![id1]);
    add_rep(&mut rep_engine, 10, id0, u32::max_value() - 4);
    add_rep(&mut rep_engine, 10, id1, 1);

    // Test big values that result in < 100%
    // Active identity with 99% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xDFFF_FFFE));
    assert_abs_diff_eq!(p01, 0.875, epsilon = 1e-9);
    // Active identity with 1 reputation
    let (t02, p02) = calculate_reppoe_threshold_v1(&rep_engine, &id1, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x20000001));
    assert_abs_diff_eq!(p02, 0.125, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v1(&rep_engine, &id2, 1, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x00000001));
    assert_abs_diff_eq!(p03, 0.0, epsilon = 1e-9);

    // Repeat test with 2 witnesses
    // Active identity with 99% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v1(&rep_engine, &id0, 2, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Active identity with 1 reputation
    let (t02, p02) = calculate_reppoe_threshold_v1(&rep_engine, &id1, 2, 2000);
    assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p02, 1.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v1(&rep_engine, &id2, 2, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x00000008));
    assert_abs_diff_eq!(p03, 1e-9, epsilon = 1e-9);
}

#[test]
fn reppoe_much_rep_trapezoid_v2() {
    // Test the behavior of the algorithm when one node has 99% reputation and another node has 1%
    let mut rep_engine = ReputationEngine::new(1000);
    let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id0]);
    rep_engine.ars_mut().push_activity(vec![id1]);
    add_rep(&mut rep_engine, 10, id0, u32::max_value() - 4);
    add_rep(&mut rep_engine, 10, id1, 1);

    // Test big values that result in < 100%
    // Active identity with 99% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    // 1 / 2000 = 500 / 1_000_000
    assert_abs_diff_eq!(p01, 1.0 / 2000.0, epsilon = 1e-9);
    // Active identity with 1 reputation
    let (t02, p02) = calculate_reppoe_threshold_v2(&rep_engine, &id1, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF / 2000));
    assert_abs_diff_eq!(p02, 1.0 / 2000.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v2(&rep_engine, &id2, 1, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x00000001));
    assert_abs_diff_eq!(p03, 0.0, epsilon = 1e-9);

    // Repeat test with 2 witnesses
    // Active identity with 99% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v2(&rep_engine, &id0, 2, 2000);
    assert_eq!(t01, Hash::with_first_u32(0x010624dd));
    // 8 / 2000 = 4_000 / 1_000_000
    assert_abs_diff_eq!(p01, 8.0 / 2000.0, epsilon = 1e-9);
    // Active identity with 1 reputation
    let (t02, p02) = calculate_reppoe_threshold_v2(&rep_engine, &id1, 2, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x010624dd));
    assert_abs_diff_eq!(p02, 8.0 / 2000.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v2(&rep_engine, &id2, 2, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x00000008));
    assert_abs_diff_eq!(p03, 1e-9, epsilon = 1e-9);
}

#[test]
fn reppoe_much_rep_trapezoid_v3() {
    // Test the behavior of the algorithm when one node has 99% reputation and another node has 1%
    let mut rep_engine = ReputationEngine::new(1000);
    let id0 = PublicKeyHash::from_bytes(&[0; 20]).unwrap();
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();
    let id2 = PublicKeyHash::from_bytes(&[2; 20]).unwrap();
    rep_engine.ars_mut().push_activity(vec![id0]);
    rep_engine.ars_mut().push_activity(vec![id1]);
    add_rep(&mut rep_engine, 10, id0, u32::max_value() - 4);
    add_rep(&mut rep_engine, 10, id1, 1);

    // Test big values that result in < 100%
    // Active identity with 99% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xDFFF_FFFE));
    assert_abs_diff_eq!(p01, 0.875, epsilon = 1e-9);
    // Active identity with 1 reputation
    let (t02, p02) = calculate_reppoe_threshold_v3(&rep_engine, &id1, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x20000001));
    assert_abs_diff_eq!(p02, 0.125, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v3(&rep_engine, &id2, 1, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x00000001));
    assert_abs_diff_eq!(p03, 0.0, epsilon = 1e-9);

    // Repeat test with 2 witnesses
    // Active identity with 99% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v3(&rep_engine, &id0, 2, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Active identity with 1 reputation
    let (t02, p02) = calculate_reppoe_threshold_v3(&rep_engine, &id1, 2, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x40000002));
    assert_abs_diff_eq!(p02, 0.25, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t03, p03) = calculate_reppoe_threshold_v3(&rep_engine, &id2, 2, 2000);
    assert_eq!(t03, Hash::with_first_u32(0x00000002));
    assert_abs_diff_eq!(p03, 0.0, epsilon = 1e-9);
}

#[test]
fn reppoe_100_reputed_nodes_v1() {
    // Test the behavior of the algorithm when 100 nodes have equally large reputation
    let mut rep_engine = ReputationEngine::new(1000);
    let gen_id = |i| PublicKeyHash::from_bytes(&[i; 20]).unwrap();
    let id_rep = gen_id(0);
    let id_no_active = gen_id(255);
    rep_engine.ars_mut().push_activity((0..100).map(gen_id));
    rep_engine
        .trs_mut()
        .gain(
            Alpha(10),
            (0..100).map(|i| {
                let id = gen_id(i);
                (id, Reputation(10_000))
            }),
        )
        .unwrap();

    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v1(&rep_engine, &id_rep, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0x028f5c28));
    assert_abs_diff_eq!(p01, 0.01, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v1(&rep_engine, &id_no_active, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x000010c6));
    assert_abs_diff_eq!(p02, 1e-6, epsilon = 1e-9);

    // Repeat test with 2 witnesses
    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v1(&rep_engine, &id_rep, 2, 2000);
    assert_eq!(t01, Hash::with_first_u32(0x051eb851));
    assert_abs_diff_eq!(p01, 0.02, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v1(&rep_engine, &id_no_active, 2, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x0000218d));
    assert_abs_diff_eq!(p02, 2e-6, epsilon = 1e-9);

    // Repeat test with 100 witnesses
    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v1(&rep_engine, &id_rep, 100, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v1(&rep_engine, &id_no_active, 100, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x00068d8d));
    assert_abs_diff_eq!(p02, 99.99e-6, epsilon = 1e-9);

    // Repeat test with 101 witnesses
    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v1(&rep_engine, &id_rep, 101, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v1(&rep_engine, &id_no_active, 101, 2000);
    assert_eq!(t02, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p02, 1.0, epsilon = 1e-9);
}

#[test]
fn reppoe_100_reputed_nodes_v2() {
    // Test the behavior of the algorithm when 100 nodes have equally large reputation
    let mut rep_engine = ReputationEngine::new(1000);
    let gen_id = |i| PublicKeyHash::from_bytes(&[i; 20]).unwrap();
    let id_rep = gen_id(0);
    let id_no_active = gen_id(255);
    rep_engine.ars_mut().push_activity((0..100).map(gen_id));
    rep_engine
        .trs_mut()
        .gain(
            Alpha(10),
            (0..100).map(|i| {
                let id = gen_id(i);
                (id, Reputation(10_000))
            }),
        )
        .unwrap();

    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v2(&rep_engine, &id_rep, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0x0020c49b));
    assert_abs_diff_eq!(p01, 1.0 / 2000.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v2(&rep_engine, &id_no_active, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x000010c6));
    assert_abs_diff_eq!(p02, 1e-6, epsilon = 1e-9);

    // Repeat test with 2 witnesses
    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v2(&rep_engine, &id_rep, 2, 2000);
    assert_eq!(t01, Hash::with_first_u32(0x00418937));
    assert_abs_diff_eq!(p01, 2.0 / 2000.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v2(&rep_engine, &id_no_active, 2, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x0000218d));
    assert_abs_diff_eq!(p02, 2e-6, epsilon = 1e-9);

    // Repeat test with 100 witnesses
    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v2(&rep_engine, &id_rep, 100, 2000);
    assert_eq!(t01, Hash::with_first_u32(0x0ccccccc));
    assert_abs_diff_eq!(p01, 100.0 / 2000.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v2(&rep_engine, &id_no_active, 100, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x00068d8d));
    assert_abs_diff_eq!(p02, 99.99e-6, epsilon = 1e-9);

    // Repeat test with 101 witnesses
    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v2(&rep_engine, &id_rep, 101, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xffffffff));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v2(&rep_engine, &id_no_active, 101, 2000);
    assert_eq!(t02, Hash::with_first_u32(0xffffffff));
    assert_abs_diff_eq!(p02, 1.0, epsilon = 1e-9);
}

#[test]
fn reppoe_100_reputed_nodes_v3() {
    // Test the behavior of the algorithm when 100 nodes have equally large reputation
    let mut rep_engine = ReputationEngine::new(1000);
    let gen_id = |i| PublicKeyHash::from_bytes(&[i; 20]).unwrap();
    let id_rep = gen_id(0);
    let id_no_active = gen_id(255);
    rep_engine.ars_mut().push_activity((0..100).map(gen_id));
    rep_engine
        .trs_mut()
        .gain(
            Alpha(10),
            (0..100).map(|i| {
                let id = gen_id(i);
                (id, Reputation(10_000))
            }),
        )
        .unwrap();

    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v3(&rep_engine, &id_rep, 1, 2000);
    assert_eq!(t01, Hash::with_first_u32(0x028f5c28));
    assert_abs_diff_eq!(p01, 0.01, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v3(&rep_engine, &id_no_active, 1, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x000010c6));
    assert_abs_diff_eq!(p02, 1e-6, epsilon = 1e-9);

    // Repeat test with 2 witnesses
    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v3(&rep_engine, &id_rep, 2, 2000);
    assert_eq!(t01, Hash::with_first_u32(0x051eb851));
    assert_abs_diff_eq!(p01, 0.02, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v3(&rep_engine, &id_no_active, 2, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x0000218d));
    assert_abs_diff_eq!(p02, 2e-6, epsilon = 1e-9);

    // Repeat test with 100 witnesses
    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v3(&rep_engine, &id_rep, 100, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v3(&rep_engine, &id_no_active, 100, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x00068d8d));
    assert_abs_diff_eq!(p02, 99.99e-6, epsilon = 1e-9);

    // Repeat test with 100 witnesses
    // Active identity with 1% of the reputation
    let (t01, p01) = calculate_reppoe_threshold_v3(&rep_engine, &id_rep, 101, 2000);
    assert_eq!(t01, Hash::with_first_u32(0xFFFF_FFFF));
    assert_abs_diff_eq!(p01, 1.0, epsilon = 1e-9);
    // Inactive identity with 0 reputation
    let (t02, p02) = calculate_reppoe_threshold_v3(&rep_engine, &id_no_active, 101, 2000);
    assert_eq!(t02, Hash::with_first_u32(0x00069e54));
    assert_abs_diff_eq!(p02, 100.99e-6, epsilon = 1e-9);
}

#[test]
fn reppoe_worst_case() {
    // Check the worst case reppoe probability in mainnet with an empty ARS, a data request with
    // the maximum number of witnesses, and taking into account the extra commit rounds.

    fn max_num_witnesses_for_dr_weigth(max_dr_weight: u32) -> u16 {
        let mut dr = DRTransaction::default();
        let mut num_witnesses = 1;

        while dr.weight() < max_dr_weight {
            dr.body.dr_output.witnesses = num_witnesses;
            num_witnesses += 1;
        }

        num_witnesses - 1
    }

    let consensus_constants = witnet_config::config::consensus_constants_from_partial(
        &witnet_data_structures::chain::PartialConsensusConstants::default(),
        &witnet_config::defaults::Mainnet,
    );
    let max_dr_weight = consensus_constants.max_dr_weight;
    // Need to use extra_rounds + 1 because this variable represents the additional rounds
    let max_commit_rounds = consensus_constants.extra_rounds + 1;
    let minimum_difficulty = consensus_constants.minimum_difficulty;

    let max_witnesses = max_num_witnesses_for_dr_weigth(max_dr_weight);
    let max_backup_witnesses = calculate_backup_witnesses(max_witnesses, max_commit_rounds);

    assert_eq!(max_witnesses, 126);
    assert_eq!(max_witnesses + max_backup_witnesses, 630);

    // Empty ARS
    let rep_engine = ReputationEngine::new(1000);
    let id1 = PublicKeyHash::from_bytes(&[1; 20]).unwrap();

    let (_t00, p00) = calculate_reppoe_threshold_v3(
        &rep_engine,
        &id1,
        max_witnesses + max_backup_witnesses,
        minimum_difficulty,
    );

    assert_abs_diff_eq!(p00, 0.315, epsilon = 1e-9);
}
