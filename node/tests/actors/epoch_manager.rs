use std::convert::TryFrom;
use witnet_node::actors::epoch_manager::{EpochManager, EpochManagerError};

#[test]
fn epoch_zero_range() {
    let zero = 1000;
    let period = 90;
    let mut em = EpochManager::default();
    em.set_checkpoint_zero_and_period(zero, u16::try_from(period).unwrap());

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
    let mut em = EpochManager::default();
    em.set_checkpoint_zero_and_period(zero, period);

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
