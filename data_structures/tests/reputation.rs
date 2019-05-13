use witnet_data_structures::chain::{penalize_factor, reputation_issuance, Alpha, Reputation};

#[test]
fn issued_reputation() {
    let rep_per_alpha = Reputation(1);
    let rep_issuance_last = Alpha(100);

    let old_alpha = Alpha(0);
    let new_alpha = Alpha(99);
    let x = reputation_issuance(rep_per_alpha, rep_issuance_last, old_alpha, new_alpha);
    assert_eq!(x.0, rep_per_alpha.0 * (rep_issuance_last.0 - 1));

    let old_alpha = Alpha(0);
    let new_alpha = Alpha(100);
    let x = reputation_issuance(rep_per_alpha, rep_issuance_last, old_alpha, new_alpha);
    assert_eq!(x.0, rep_per_alpha.0 * rep_issuance_last.0);

    let old_alpha = Alpha(0);
    let new_alpha = Alpha(101);
    let x = reputation_issuance(rep_per_alpha, rep_issuance_last, old_alpha, new_alpha);
    assert_eq!(x.0, rep_per_alpha.0 * rep_issuance_last.0);

    let old_alpha = Alpha(1);
    let new_alpha = Alpha(101);
    let x = reputation_issuance(rep_per_alpha, rep_issuance_last, old_alpha, new_alpha);
    assert_eq!(x.0, rep_per_alpha.0 * (rep_issuance_last.0 - 1));

    let old_alpha = Alpha(99);
    let new_alpha = Alpha(100);
    let x = reputation_issuance(rep_per_alpha, rep_issuance_last, old_alpha, new_alpha);
    assert_eq!(x.0, rep_per_alpha.0);

    let old_alpha = Alpha(100);
    let new_alpha = Alpha(101);
    let x = reputation_issuance(rep_per_alpha, rep_issuance_last, old_alpha, new_alpha);
    assert_eq!(x.0, 0);
}

#[test]
fn penalization_function() {
    let r = Reputation(100);
    // Lose half of the reputation for every lie
    let new_r = penalize_factor(0.5, 1)(r);
    assert_eq!(new_r, Reputation(50));

    // After 2 lies, the reputation is 1/4 of 100: 25
    let new_r2 = penalize_factor(0.5, 1)(new_r);
    assert_eq!(new_r2, Reputation(25));

    // After 3 lies, we cannot evenly divide 25/2 so the reputation is rounded down to 12
    assert_eq!(Reputation(12), penalize_factor(0.5, 1)(new_r2));

    // The result is the same when we apply the function once with num_lies = 3:
    // Reputation goes from 100 to 12
    assert_eq!(Reputation(12), penalize_factor(0.5, 3)(r));

    // And obviously the result is the same if we use a penalization factor of 0.5 ** 3 = 0.125
    assert_eq!(Reputation(12), penalize_factor(0.5 * 0.5 * 0.5, 1)(r));

    // After 6 lies the reputation goes to 1
    assert_eq!(Reputation(1), penalize_factor(0.5, 6)(r));

    // After 7 lies the reputation goes to 0
    assert_eq!(Reputation(0), penalize_factor(0.5, 7)(r));

    // Any penalization to an identity with 1 reputation point will result in losing that last
    // point of reputation, even with a very generous penalization factor
    assert_eq!(Reputation(0), penalize_factor(0.999, 1)(Reputation(1)));
}
