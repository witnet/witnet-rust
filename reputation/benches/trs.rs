#[macro_use]
extern crate bencher;
use bencher::Bencher;
use std::ops::{AddAssign, SubAssign};
use witnet_reputation::TotalReputationSet;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Reputation(u32);

impl AddAssign for Reputation {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0
    }
}

impl SubAssign for Reputation {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Alpha(u32);

// Example demurrage functions used in tests:
// Factor: lose half of the reputation for each lie
fn fctr(num_lies: u32) -> impl Fn(Reputation) -> Reputation {
    const PENALIZATION_FACTOR: f64 = 0.5;
    move |r| Reputation((f64::from(r.0) * PENALIZATION_FACTOR.powf(f64::from(num_lies))) as u32)
}

fn bench_0_alpha(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    b.iter(|| {
        let mut a = TotalReputationSet::new();
        a.gain(Alpha(10), v.clone()).unwrap();
    })
}

fn bench_0_alpha_x10(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    b.iter(|| {
        let mut a = TotalReputationSet::new();
        for j in 0..10 {
            a.gain(Alpha(10 + j), v.clone()).unwrap();
        }
    })
}

fn bench_0_alpha_expire(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    b.iter(|| {
        let mut a = TotalReputationSet::new();
        a.gain(Alpha(10), v.clone()).unwrap();
        a.expire(&Alpha(10));
    })
}

fn bench_0_alpha_expire_x10(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    b.iter(|| {
        let mut a = TotalReputationSet::new();
        for j in 0..10 {
            a.expire(&Alpha(j));
            a.gain(Alpha(4 + j), v.clone()).unwrap();
        }
        a.expire(&Alpha(1000));
    })
}

fn bench_0_alpha_penalize(b: &mut Bencher) {
    let mut v = vec![];
    let ids: Vec<_> = (0..10000).collect();
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    b.iter(|| {
        let mut a = TotalReputationSet::new();
        a.gain(Alpha(10), v.clone()).unwrap();
        // Apply 50% demurrage to each identity
        let pp = (0..10000).map(|i| (&ids[i], fctr(1)));
        a.penalize_many(pp).unwrap();
    })
}

fn bench_0_alpha_penalize_x10(b: &mut Bencher) {
    let mut v = vec![];
    let ids: Vec<_> = (0..10000).collect();
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    b.iter(|| {
        let mut a = TotalReputationSet::new();
        for _ in 0..10 {
            a.gain(Alpha(10), v.clone()).unwrap();
            // Apply 50% demurrage to each identity
            let pp = (0..10000).map(|i| (&ids[i], fctr(1)));
            a.penalize_many(pp).unwrap();
        }
    })
}

fn bench_0_alpha_penalize_few(b: &mut Bencher) {
    let mut v = vec![];
    let ids: Vec<_> = (0..10000).collect();
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    b.iter(|| {
        let mut a = TotalReputationSet::new();
        a.gain(Alpha(10), v.clone()).unwrap();
        // Penalize few identities but with a big penalization
        let pp = (0..1000).map(|i| (&ids[i], fctr(10)));
        a.penalize_many(pp).unwrap();
    })
}

fn bench_0_alpha_expire_penalize(b: &mut Bencher) {
    let mut v = vec![];
    let ids: Vec<_> = (0..10000).collect();
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    b.iter(|| {
        let mut a = TotalReputationSet::new();
        a.gain(Alpha(10), v.clone()).unwrap();
        // Apply 50% demurrage to each identity
        let pp = (0..10000).map(|i| (&ids[i], fctr(1)));
        a.penalize_many(pp).unwrap();
        a.expire(&Alpha(10));
    })
}

fn bench_0_alpha_expire_penalize_x10(b: &mut Bencher) {
    let mut v = vec![];
    let ids: Vec<_> = (0..10000).collect();
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    b.iter(|| {
        let mut a = TotalReputationSet::new();
        for j in 0..10 {
            a.expire(&Alpha(j));
            a.gain(Alpha(4 + j), v.clone()).unwrap();
            // Apply 50% demurrage to each identity
            let pp = (0..10000).map(|i| (&ids[i], fctr(1)));
            a.penalize_many(pp).unwrap();
        }
    })
}

fn bench_0_alpha_rep_sum(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    b.iter(|| {
        let mut a = TotalReputationSet::new();
        a.gain(Alpha(10), v.clone()).unwrap();
        a.get_total_sum()
    })
}
// Measure the overhead of clone

fn bench_100_alpha_null(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    let mut a = TotalReputationSet::new();
    for j in 0..100 {
        a.gain(Alpha(j), v.clone()).unwrap();
    }

    b.iter(|| a.clone());
}

fn bench_100_alpha_gain(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    let mut a = TotalReputationSet::new();
    for j in 0..100 {
        a.gain(Alpha(j), v.clone()).unwrap();
    }

    b.iter(|| {
        let mut a = a.clone();
        a.gain(Alpha(100), v.clone()).unwrap();
        a
    })
}

fn bench_100_alpha_expire(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    let mut a = TotalReputationSet::new();
    for j in 0..100 {
        a.gain(Alpha(j), v.clone()).unwrap();
    }

    b.iter(|| {
        let mut a = a.clone();
        a.expire(&Alpha(100));
    })
}

fn bench_100_alpha_penalize(b: &mut Bencher) {
    let mut v = vec![];
    let ids: Vec<_> = (0..10000).collect();
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    let mut a = TotalReputationSet::new();
    for j in 0..100 {
        a.gain(Alpha(j), v.clone()).unwrap();
    }

    b.iter(|| {
        let mut a = a.clone();
        // Apply 50% demurrage to each identity
        let pp = (0..10000).map(|i| (&ids[i], fctr(1)));
        a.penalize_many(pp).unwrap();
        a
    })
}

fn bench_100_alpha_full_cycle(b: &mut Bencher) {
    let mut v = vec![];
    let ids: Vec<_> = (0..10000).collect();
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    let mut a = TotalReputationSet::new();
    for j in 0..100 {
        a.gain(Alpha(j), v.clone()).unwrap();
    }

    b.iter(|| {
        let mut a = a.clone();
        // Expire
        a.expire(&Alpha(0));
        // Gain
        a.gain(Alpha(100), v.clone()).unwrap();
        // Apply 50% demurrage to each identity
        let pp = (0..10000).map(|i| (&ids[i], fctr(1)));
        a.penalize_many(pp).unwrap();
        a
    })
}

// Measure the overhead of clone

fn bench_1000_alpha_null(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    let mut a = TotalReputationSet::new();
    for j in 0..1000 {
        a.gain(Alpha(j), v.clone()).unwrap();
    }

    b.iter(|| a.clone());
}
fn bench_1000_alpha_gain(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    let mut a = TotalReputationSet::new();
    for j in 0..1000 {
        a.gain(Alpha(j), v.clone()).unwrap();
    }

    b.iter(|| {
        let mut a = a.clone();
        a.gain(Alpha(1000), v.clone()).unwrap();
        a
    })
}

fn bench_1000_alpha_expire(b: &mut Bencher) {
    let mut v = vec![];
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    let mut a = TotalReputationSet::new();
    for j in 0..1000 {
        a.gain(Alpha(j), v.clone()).unwrap();
    }

    b.iter(|| {
        let mut a = a.clone();
        a.expire(&Alpha(1000));
    })
}

fn bench_1000_alpha_penalize(b: &mut Bencher) {
    let mut v = vec![];
    let ids: Vec<_> = (0..10000).collect();
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    let mut a = TotalReputationSet::new();
    for j in 0..1000 {
        a.gain(Alpha(j), v.clone()).unwrap();
    }

    b.iter(|| {
        let mut a = a.clone();
        // Apply 50% demurrage to each identity
        let pp = (0..10000).map(|i| (&ids[i], fctr(1)));
        a.penalize_many(pp).unwrap();
        a
    })
}

fn bench_1000_alpha_full_cycle(b: &mut Bencher) {
    let mut v = vec![];
    let ids: Vec<_> = (0..10000).collect();
    for i in 0..10000 {
        v.push((i, Reputation(10 + (i % 4))));
    }

    let mut a = TotalReputationSet::new();
    for j in 0..1000 {
        a.gain(Alpha(j), v.clone()).unwrap();
    }

    b.iter(|| {
        let mut a = a.clone();
        // Expire
        a.expire(&Alpha(0));
        // Gain
        a.gain(Alpha(1000), v.clone()).unwrap();
        // Apply 50% demurrage to each identity
        let pp = (0..10000).map(|i| (&ids[i], fctr(1)));
        a.penalize_many(pp).unwrap();
        a
    })
}

benchmark_main!(benches);
benchmark_group!(
    benches,
    bench_0_alpha,
    bench_0_alpha_x10,
    bench_0_alpha_expire,
    bench_0_alpha_expire_x10,
    bench_0_alpha_penalize,
    bench_0_alpha_penalize_x10,
    bench_0_alpha_penalize_few,
    bench_0_alpha_expire_penalize,
    bench_0_alpha_expire_penalize_x10,
    bench_0_alpha_rep_sum,
    bench_100_alpha_null,
    bench_100_alpha_gain,
    bench_100_alpha_expire,
    bench_100_alpha_penalize,
    bench_100_alpha_full_cycle,
    bench_1000_alpha_null,
    bench_1000_alpha_gain,
    bench_1000_alpha_expire,
    bench_1000_alpha_penalize,
    bench_1000_alpha_full_cycle,
);
