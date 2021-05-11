#[macro_use]
extern crate bencher;
use bencher::Bencher;
use std::{collections::HashMap, convert::TryFrom, iter};
use witnet_data_structures::chain::{
    Alpha, Environment, PublicKeyHash, Reputation, ReputationEngine,
};
use witnet_data_structures::mainnet_validations::ActiveWips;

// To benchmark the old algorithm, comment out this import:
use witnet_validations::validations;
// To benchmark the old algorithm, comment out the line that says cfg any:
#[cfg(any())]
mod validations {
    use super::*;
    use witnet_data_structures::chain::Hash;

    pub fn calculate_reppoe_threshold(
        rep_eng: &ReputationEngine,
        pkh: &PublicKeyHash,
        num_witnesses: u16,
    ) -> (Hash, f64) {
        // Add 1 to reputation because otherwise a node with 0 reputation would
        // never be eligible for a data request
        let my_reputation = u64::from(rep_eng.get_eligibility()) + 1;

        // Add N to the total active reputation to account for the +1 to my_reputation
        // This is equivalent to adding 1 reputation to every active identity
        let total_active_reputation = rep_eng.total_active_reputation();

        // The probability of being eligible is `factor / total_active_reputation`
        let factor = u64::from(num_witnesses) * my_reputation;

        let max = u64::max_value();
        // Check for overflow: when the probability is more than 100%, cap it to 100%
        let target = if factor >= total_active_reputation {
            max
        } else {
            (max / total_active_reputation) * factor
        };
        let target = (target >> 32) as u32;

        let probability = (target as f64 / (max >> 32) as f64) * 100_f64;
        (Hash::with_first_u32(target), probability)
    }
}

// This should only be used in tests
fn all_wips_active() -> ActiveWips {
    let mut active_wips = HashMap::new();
    active_wips.insert("WIP0014", 500_000);

    ActiveWips {
        active_wips,
        block_epoch: u32::MAX,
        environment: Environment::Mainnet,
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

fn be<I>(
    b: &mut Bencher,
    num_witnesses: u16,
    reps: I,
    invalidate_sorted_cache: bool,
    invalidate_threshold_cache: bool,
) where
    I: IntoIterator<Item = u32>,
{
    let mut rep_eng = ReputationEngine::new(1000);
    let my_pkh = PublicKeyHash::from_bytes(&[0x01; 20]).unwrap();
    for (i, rep) in reps.into_iter().enumerate() {
        let pkh = pkh_i(u32::try_from(i).unwrap());
        rep_eng.ars_mut().push_activity(vec![pkh]);
        rep_eng
            .trs_mut()
            .gain(Alpha(10), vec![(pkh, Reputation(rep))])
            .unwrap();
    }
    // Initialize cache
    rep_eng.total_active_reputation();
    validations::calculate_reppoe_threshold(
        &rep_eng,
        &my_pkh,
        num_witnesses,
        2000,
        &all_wips_active(),
    );
    b.iter(|| {
        if invalidate_sorted_cache {
            rep_eng.invalidate_reputation_threshold_cache()
        }
        if invalidate_threshold_cache {
            rep_eng.clear_threshold_cache();
        }
        validations::calculate_reppoe_threshold(
            &rep_eng,
            &my_pkh,
            num_witnesses,
            2000,
            &all_wips_active(),
        )
    })
}

fn staggered() -> impl Iterator<Item = u32> {
    iter::repeat(10000)
        .take(50)
        .chain(iter::repeat(1000).take(50))
        .chain(iter::repeat(100).take(50))
        .chain(iter::repeat(10).take(50))
}

fn no_cache_empty_rep_eng_1w(b: &mut Bencher) {
    be(b, 1, vec![], true, true)
}

fn no_cache_empty_rep_eng_100w(b: &mut Bencher) {
    be(b, 100, vec![], true, true)
}

fn no_cache_empty_rep_eng_10000w(b: &mut Bencher) {
    be(b, 10000, vec![], true, true)
}

fn no_cache_1_active_identity_1w(b: &mut Bencher) {
    be(b, 1, vec![100], true, true)
}

fn no_cache_1_active_identity_100w(b: &mut Bencher) {
    be(b, 100, vec![100], true, true)
}

fn no_cache_1_active_identity_10000w(b: &mut Bencher) {
    be(b, 10000, vec![100], true, true)
}

fn no_cache_10_active_identities_1w(b: &mut Bencher) {
    be(b, 1, vec![100; 10], true, true)
}

fn no_cache_10_active_identities_100w(b: &mut Bencher) {
    be(b, 100, vec![100; 10], true, true)
}

fn no_cache_10_active_identities_10000w(b: &mut Bencher) {
    be(b, 10000, vec![100; 10], true, true)
}

fn no_cache_100_active_identities_1w(b: &mut Bencher) {
    be(b, 1, vec![100; 100], true, true)
}

fn no_cache_100_active_identities_100w(b: &mut Bencher) {
    be(b, 100, vec![100; 100], true, true)
}

fn no_cache_100_active_identities_10000w(b: &mut Bencher) {
    be(b, 10000, vec![100; 100], true, true)
}

fn no_cache_staggered_1w(b: &mut Bencher) {
    be(b, 1, staggered(), true, true)
}

fn no_cache_staggered_100w(b: &mut Bencher) {
    be(b, 100, staggered(), true, true)
}

fn no_cache_staggered_10000w(b: &mut Bencher) {
    be(b, 10000, staggered(), true, true)
}

fn sorted_cache_empty_rep_eng_1w(b: &mut Bencher) {
    be(b, 1, vec![], false, true)
}

fn sorted_cache_empty_rep_eng_100w(b: &mut Bencher) {
    be(b, 100, vec![], false, true)
}

fn sorted_cache_empty_rep_eng_10000w(b: &mut Bencher) {
    be(b, 10000, vec![], false, true)
}

fn sorted_cache_1_active_identity_1w(b: &mut Bencher) {
    be(b, 1, vec![100], false, true)
}

fn sorted_cache_1_active_identity_100w(b: &mut Bencher) {
    be(b, 100, vec![100], false, true)
}

fn sorted_cache_1_active_identity_10000w(b: &mut Bencher) {
    be(b, 10000, vec![100], false, true)
}

fn sorted_cache_10_active_identities_1w(b: &mut Bencher) {
    be(b, 1, vec![100; 10], false, true)
}

fn sorted_cache_10_active_identities_100w(b: &mut Bencher) {
    be(b, 100, vec![100; 10], false, true)
}

fn sorted_cache_10_active_identities_10000w(b: &mut Bencher) {
    be(b, 10000, vec![100; 10], false, true)
}

fn sorted_cache_100_active_identities_1w(b: &mut Bencher) {
    be(b, 1, vec![100; 100], false, true)
}

fn sorted_cache_100_active_identities_100w(b: &mut Bencher) {
    be(b, 100, vec![100; 100], false, true)
}

fn sorted_cache_100_active_identities_10000w(b: &mut Bencher) {
    be(b, 10000, vec![100; 100], false, true)
}

fn sorted_cache_staggered_1w(b: &mut Bencher) {
    be(b, 1, staggered(), false, true)
}

fn sorted_cache_staggered_100w(b: &mut Bencher) {
    be(b, 100, staggered(), false, true)
}

fn sorted_cache_staggered_10000w(b: &mut Bencher) {
    be(b, 10000, staggered(), false, true)
}

fn threshold_cache_empty_rep_eng_1w(b: &mut Bencher) {
    be(b, 1, vec![], false, false)
}

fn threshold_cache_empty_rep_eng_100w(b: &mut Bencher) {
    be(b, 100, vec![], false, false)
}

fn threshold_cache_empty_rep_eng_10000w(b: &mut Bencher) {
    be(b, 10000, vec![], false, false)
}

fn threshold_cache_1_active_identity_1w(b: &mut Bencher) {
    be(b, 1, vec![100], false, false)
}

fn threshold_cache_1_active_identity_100w(b: &mut Bencher) {
    be(b, 100, vec![100], false, false)
}

fn threshold_cache_1_active_identity_10000w(b: &mut Bencher) {
    be(b, 10000, vec![100], false, false)
}

fn threshold_cache_10_active_identities_1w(b: &mut Bencher) {
    be(b, 1, vec![100; 10], false, false)
}

fn threshold_cache_10_active_identities_100w(b: &mut Bencher) {
    be(b, 100, vec![100; 10], false, false)
}

fn threshold_cache_10_active_identities_10000w(b: &mut Bencher) {
    be(b, 10000, vec![100; 10], false, false)
}

fn threshold_cache_100_active_identities_1w(b: &mut Bencher) {
    be(b, 1, vec![100; 100], false, false)
}

fn threshold_cache_100_active_identities_100w(b: &mut Bencher) {
    be(b, 100, vec![100; 100], false, false)
}

fn threshold_cache_100_active_identities_10000w(b: &mut Bencher) {
    be(b, 10000, vec![100; 100], false, false)
}

fn threshold_cache_staggered_1w(b: &mut Bencher) {
    be(b, 1, staggered(), false, false)
}

fn threshold_cache_staggered_100w(b: &mut Bencher) {
    be(b, 100, staggered(), false, false)
}

fn threshold_cache_staggered_10000w(b: &mut Bencher) {
    be(b, 10000, staggered(), false, false)
}

benchmark_main!(benches);
benchmark_group!(
    benches,
    no_cache_empty_rep_eng_1w,
    no_cache_empty_rep_eng_100w,
    no_cache_empty_rep_eng_10000w,
    no_cache_1_active_identity_1w,
    no_cache_1_active_identity_100w,
    no_cache_1_active_identity_10000w,
    no_cache_10_active_identities_1w,
    no_cache_10_active_identities_100w,
    no_cache_10_active_identities_10000w,
    no_cache_100_active_identities_1w,
    no_cache_100_active_identities_100w,
    no_cache_100_active_identities_10000w,
    no_cache_staggered_1w,
    no_cache_staggered_100w,
    no_cache_staggered_10000w,
    sorted_cache_empty_rep_eng_1w,
    sorted_cache_empty_rep_eng_100w,
    sorted_cache_empty_rep_eng_10000w,
    sorted_cache_1_active_identity_1w,
    sorted_cache_1_active_identity_100w,
    sorted_cache_1_active_identity_10000w,
    sorted_cache_10_active_identities_1w,
    sorted_cache_10_active_identities_100w,
    sorted_cache_10_active_identities_10000w,
    sorted_cache_100_active_identities_1w,
    sorted_cache_100_active_identities_100w,
    sorted_cache_100_active_identities_10000w,
    sorted_cache_staggered_1w,
    sorted_cache_staggered_100w,
    sorted_cache_staggered_10000w,
    threshold_cache_empty_rep_eng_1w,
    threshold_cache_empty_rep_eng_100w,
    threshold_cache_empty_rep_eng_10000w,
    threshold_cache_1_active_identity_1w,
    threshold_cache_1_active_identity_100w,
    threshold_cache_1_active_identity_10000w,
    threshold_cache_10_active_identities_1w,
    threshold_cache_10_active_identities_100w,
    threshold_cache_10_active_identities_10000w,
    threshold_cache_100_active_identities_1w,
    threshold_cache_100_active_identities_100w,
    threshold_cache_100_active_identities_10000w,
    threshold_cache_staggered_1w,
    threshold_cache_staggered_100w,
    threshold_cache_staggered_10000w,
);
