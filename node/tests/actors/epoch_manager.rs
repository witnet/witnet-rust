use std::convert::TryFrom;
use witnet_node::actors::epoch_manager::{EpochManager, EpochManagerError};

#[test]
fn epoch_zero_range() {
    let zero = 1000;
    let period = 90;
    let zero_v2 = i64::MAX;
    let period_v2 = 1;
    let mut em = EpochManager::default();
    em.set_checkpoint_zero_and_period(
        zero,
        u16::try_from(period).unwrap(),
        zero_v2,
        u16::try_from(period_v2).unwrap(),
    );

    // [1000, 1089] are in epoch 0
    for now in zero..zero + period {
        assert_eq!(em.epoch_at(now), Ok(0), "Error at {}", now);
    }

    // 1090 is the start of epoch 1
    let now = zero + period;
    assert_eq!(em.epoch_at(now), Ok(1), "Error at {}", now);

    // Epoch 0: t = 1000
    assert_eq!(em.epoch_timestamp(0), Ok(zero));
    // Epoch 1: t = 1090
    assert_eq!(em.epoch_timestamp(1), Ok(now));
}

#[test]
fn epoch_zero_in_the_future() {
    let zero = 1000;
    let now = 999;
    let period = 90u16;
    let zero_v2 = i64::MAX;
    let period_v2 = 1u16;
    let mut em = EpochManager::default();
    em.set_checkpoint_zero_and_period(zero, period, zero_v2, period_v2);

    assert_eq!(
        em.epoch_at(now),
        Err(EpochManagerError::CheckpointZeroInTheFuture(zero))
    );
}

#[test]
fn epoch_unknown() {
    let em = EpochManager::default();
    // By default, the epoch manager doesn't know when the epoch zero started
    assert_eq!(
        em.epoch_at(1234),
        Err(EpochManagerError::UnknownEpochConstants)
    );
}

#[test]
fn epoch_v2() {
    let zero = 1000;
    let period = 50;
    let zero_v2 = 2000;
    let period_v2 = 25;
    let mut em = EpochManager::default();
    em.set_checkpoint_zero_and_period(
        zero,
        u16::try_from(period).unwrap(),
        zero_v2,
        u16::try_from(period_v2).unwrap(),
    );

    // [1000, 1049] are in epoch 0
    for now in zero..zero + period {
        assert_eq!(em.epoch_at(now), Ok(0), "Error at {}", now);
    }

    // 1050 is the start of epoch 1
    assert_eq!(
        em.epoch_at(zero + period),
        Ok(1),
        "Error at {}",
        zero + period
    );

    // [1100, 1149] is part of period 2
    for now in zero + 2 * period..zero + 3 * period {
        assert_eq!(em.epoch_at(now), Ok(2), "Error at {}", now);
    }

    // [1950, 1050] are part of epoch 19 and more
    for now in zero + 19 * period..zero + 21 * period {
        if now < 2000 {
            assert_eq!(em.epoch_at(now), Ok(19), "Error at {}", now);
        } else if now < 2025 {
            assert_eq!(em.epoch_at(now), Ok(20), "Error at {}", now);
        } else {
            assert_eq!(em.epoch_at(now), Ok(21), "Error at {}", now);
        }
    }

    // Epoch 1 to 20, block time of 50 seconds
    assert_eq!(em.epoch_timestamp(0), Ok(1000), "Error at {}", 1000);
    assert_eq!(em.epoch_timestamp(10), Ok(1500), "Error at {}", 1500);
    assert_eq!(em.epoch_timestamp(20), Ok(2000), "Error at {}", 2000);
    // Epoch 21 to 40, block time of 25 seconds
    assert_eq!(em.epoch_timestamp(21), Ok(2025), "Error at {}", 2025);
    assert_eq!(em.epoch_timestamp(30), Ok(2250), "Error at {}", 2250);
    assert_eq!(em.epoch_timestamp(40), Ok(2500), "Error at {}", 2500);
}
