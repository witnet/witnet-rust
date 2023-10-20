#[macro_use]
extern crate bencher;
use bencher::Bencher;
use rand::Rng;
use witnet_data_structures::staking::prelude::*;

fn populate(b: &mut Bencher) {
    let mut stakes = Stakes::<String, u64, u64, u64>::default();
    let mut i = 1;

    b.iter(|| {
        let address = format!("{i}");
        let coins = i;
        let epoch = i;
        stakes.add_stake(address, coins, epoch).unwrap();

        i += 1;
    });
}

fn rank(b: &mut Bencher) {
    let mut stakes = Stakes::<String, u64, u64, u64>::default();
    let mut i = 1;

    let stakers = 100_000;
    let rf = 10;

    let mut rng = rand::thread_rng();

    loop {
        let coins = i;
        let epoch = i;
        let address = format!("{}", rng.gen::<u64>());

        stakes.add_stake(address, coins, epoch).unwrap();

        i += 1;

        if i == stakers {
            break;
        }
    }

    b.iter(|| {
        let rank = stakes.rank(Capability::Mining, i);
        let mut top = rank.take(usize::try_from(stakers / rf).unwrap());
        let _first = top.next();
        let _last = top.last();

        i += 1;
    })
}

fn query_power(b: &mut Bencher) {
    let mut stakes = Stakes::<String, u64, u64, u64>::default();
    let mut i = 1;

    let stakers = 100_000;

    loop {
        let coins = i;
        let epoch = i;
        let address = format!("{i}");

        stakes.add_stake(address, coins, epoch).unwrap();

        i += 1;

        if i == stakers {
            break;
        }
    }

    i = 1;

    b.iter(|| {
        let address = format!("{i}");
        let _power = stakes.query_power(&address, Capability::Mining, i);

        i += 1;
    })
}

benchmark_main!(benches);
benchmark_group!(benches, populate, rank, query_power);
