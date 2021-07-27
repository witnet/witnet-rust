#[macro_use]
extern crate bencher;
use bencher::Bencher;
use std::{convert::TryFrom, iter};
use witnet_data_structures::chain::{Alpha, PublicKeyHash, Reputation, ReputationEngine};

mod old {
    use super::*;
    use itertools::Itertools;
    use std::cmp::Ordering;
    use witnet_crypto::hash::calculate_sha256;
    use witnet_data_structures::chain::Hash;

    /// Get ARS keys ordered by reputation. If tie, order by pkh.
    pub fn get_rep_ordered_ars_list(rep_eng: &ReputationEngine) -> Vec<PublicKeyHash> {
        rep_eng
            .ars()
            .active_identities()
            .cloned()
            .sorted_by(|a, b| compare_reputed_pkh(a, b, rep_eng).reverse())
            .collect()
    }

    /// Compare 2 PublicKeyHashes comparing:
    /// First: reputation
    /// Second: Hashes related to PublicKeyHash and alpha clock || PublicKeyHashes in case of 0 rep
    fn compare_reputed_pkh(
        a: &PublicKeyHash,
        b: &PublicKeyHash,
        rep_eng: &ReputationEngine,
    ) -> Ordering {
        let rep_a = rep_eng.trs().get(a).0;
        let rep_b = rep_eng.trs().get(b).0;

        rep_a.cmp(&rep_b).then_with(|| {
            if rep_a > 0 {
                let alpha_bytes: &[u8] = &rep_eng.current_alpha().0.to_be_bytes();
                let mut a_bytes = a.as_ref().to_vec();
                let mut b_bytes = b.as_ref().to_vec();

                a_bytes.extend(alpha_bytes);
                b_bytes.extend(alpha_bytes);

                let new_hash_a: Hash = calculate_sha256(&a_bytes).into();
                let new_hash_b: Hash = calculate_sha256(&b_bytes).into();

                new_hash_a.cmp(&new_hash_b)
            } else {
                // If both identities have 0 reputation their ordering is not important because
                // they will have the same eligibility, so compare them by PublicKeyHash
                a.cmp(b)
            }
        })
    }
}

fn pkh_i(id: u32) -> PublicKeyHash {
    let mut bytes = [0xFF; 20];
    let [b0, b1, b2, b3] = id.to_le_bytes();
    bytes[0] = b0;
    bytes[1] = b1;
    bytes[2] = b2;
    bytes[3] = b3;
    PublicKeyHash::from_bytes(&bytes).unwrap()
}

fn be<I>(b: &mut Bencher, reps: I)
where
    I: IntoIterator<Item = u32>,
{
    let mut rep_eng = ReputationEngine::new(1000);
    for (i, rep) in reps.into_iter().enumerate() {
        let pkh = pkh_i(u32::try_from(i).unwrap());
        rep_eng.ars_mut().push_activity(vec![pkh]);
        rep_eng
            .trs_mut()
            .gain(Alpha(10), vec![(pkh, Reputation(rep))])
            .unwrap();
    }

    b.iter(|| {
        rep_eng.invalidate_reputation_threshold_cache();
        rep_eng.get_rep_ordered_ars_list()
    })
}

fn old_be<I>(b: &mut Bencher, reps: I)
where
    I: IntoIterator<Item = u32>,
{
    let mut rep_eng = ReputationEngine::new(1000);
    for (i, rep) in reps.into_iter().enumerate() {
        let pkh = pkh_i(u32::try_from(i).unwrap());
        rep_eng.ars_mut().push_activity(vec![pkh]);
        rep_eng
            .trs_mut()
            .gain(Alpha(10), vec![(pkh, Reputation(rep))])
            .unwrap();
    }

    b.iter(|| {
        rep_eng.invalidate_reputation_threshold_cache();
        old::get_rep_ordered_ars_list(&rep_eng)
    })
}

fn staggered(take: usize) -> impl Iterator<Item = u32> {
    iter::repeat(10000)
        .take(take)
        .chain(iter::repeat(1000).take(take))
        .chain(iter::repeat(100).take(take))
        .chain(iter::repeat(10).take(take))
}

fn all_unique(n: u32) -> impl Iterator<Item = u32> {
    1..=n
}

fn all_equal(n: usize) -> impl Iterator<Item = u32> {
    iter::repeat(1).take(n)
}

fn b_staggered_10000(b: &mut Bencher) {
    be(b, staggered(2500))
}

fn b_unique_10000(b: &mut Bencher) {
    be(b, all_unique(10_000))
}

fn b_equal_10000(b: &mut Bencher) {
    be(b, all_equal(10_000))
}

fn old_b_staggered_10000(b: &mut Bencher) {
    old_be(b, staggered(2500))
}

fn old_b_unique_10000(b: &mut Bencher) {
    old_be(b, all_unique(10_000))
}

fn old_b_equal_10000(b: &mut Bencher) {
    old_be(b, all_equal(10_000))
}

benchmark_main!(benches);
benchmark_group!(
    benches,
    b_staggered_10000,
    b_unique_10000,
    b_equal_10000,
    old_b_staggered_10000,
    old_b_unique_10000,
    old_b_equal_10000,
);
