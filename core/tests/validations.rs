use witnet_core::validations::block_reward;

#[test]
fn test_block_reward() {
    // Satowits per wit
    let spw = 100_000_000;

    assert_eq!(block_reward(0), 500 * spw);
    assert_eq!(block_reward(1), 500 * spw);
    assert_eq!(block_reward(1_749_999), 500 * spw);
    assert_eq!(block_reward(1_750_000), 250 * spw);
    assert_eq!(block_reward(3_499_999), 250 * spw);
    assert_eq!(block_reward(3_500_000), 125 * spw);
    assert_eq!(block_reward(1_750_000 * 35), 1);
    assert_eq!(block_reward(1_750_000 * 36), 0);
    assert_eq!(block_reward(1_750_000 * 63), 0);
    assert_eq!(block_reward(1_750_000 * 64), 0);
    assert_eq!(block_reward(1_750_000 * 100), 0);
}
