#[macro_use]
extern crate bencher;
use bencher::Bencher;
use rand::Rng;
use witnet_data_structures::staking::prelude::*;

const MINIMUM_VALIDATOR_STAKE: u64 = 1_000_000_000;

fn populate(b: &mut Bencher) {
    let mut stakes = StakesTester::default();
    let mut i = 1;

    b.iter(|| {
        let address = format!("{i}");
        let coins = i;
        let epoch = i;
        stakes
            .add_stake(
                address.as_str(),
                coins,
                epoch,
                true,
                MINIMUM_VALIDATOR_STAKE,
            )
            .unwrap();

        i += 1;
    });
}

fn rank(b: &mut Bencher) {
    let mut stakes = StakesTester::default();
    let mut i = 1;

    let stakers = 100_000;
    let rf = 10;

    let mut rng = rand::thread_rng();

    loop {
        let coins = i;
        let epoch = i;
        let address = format!("{}", rng.gen::<u64>());

        stakes
            .add_stake(
                address.as_str(),
                coins,
                epoch,
                true,
                MINIMUM_VALIDATOR_STAKE,
            )
            .unwrap();

        i += 1;

        if i == stakers {
            break;
        }
    }

    b.iter(|| {
        let rank = stakes.by_rank(Capability::Mining, i);
        let mut top = rank.take(usize::try_from(stakers / rf).unwrap());
        let _first = top.next();
        let _last = top.last();

        i += 1;
    })
}

fn query_power(b: &mut Bencher) {
    let mut stakes = StakesTester::default();
    let mut i = 1;

    let stakers = 100_000;

    loop {
        let coins = i;
        let epoch = i;
        let address = format!("{i}");

        stakes
            .add_stake(
                address.as_str(),
                coins,
                epoch,
                true,
                MINIMUM_VALIDATOR_STAKE,
            )
            .unwrap();

        i += 1;

        if i == stakers {
            break;
        }
    }

    i = 1;

    b.iter(|| {
        let address = format!("{i}");
        let _power = stakes.query_power(address.as_str(), Capability::Mining, i);

        i += 1;
    })
}

benchmark_main!(benches);
benchmark_group!(benches, populate, rank, query_power);
