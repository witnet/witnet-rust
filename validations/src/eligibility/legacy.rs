use witnet_data_structures::chain::{Hash, PublicKeyHash, ReputationEngine, tapi::ActiveWips};

/// Calculate the target hash needed to create a valid VRF proof of eligibility used for block
/// mining.
pub fn calculate_randpoe_threshold(
    total_identities: u32,
    replication_factor: u32,
    block_epoch: u32,
    minimum_difficulty: u32,
    epochs_with_minimum_difficulty: u32,
    active_wips: &ActiveWips,
) -> (Hash, f64) {
    let max = u64::MAX;
    let minimum_difficulty = std::cmp::max(1, minimum_difficulty);
    let target = if block_epoch <= epochs_with_minimum_difficulty {
        max / u64::from(minimum_difficulty)
    } else if active_wips.wips_0009_0011_0012() {
        let difficulty = std::cmp::max(total_identities, minimum_difficulty);
        (max / u64::from(difficulty)).saturating_mul(u64::from(replication_factor))
    } else {
        let difficulty = std::cmp::max(1, total_identities);
        (max / u64::from(difficulty)).saturating_mul(u64::from(replication_factor))
    };
    let target = u32::try_from(target >> 32).unwrap();

    let probability = f64::from(target) / f64::from(u32::try_from(max >> 32).unwrap());
    (Hash::with_first_u32(target), probability)
}

/// Calculate the target hash needed to create a valid VRF proof of eligibility used for data
/// request witnessing.
pub fn calculate_reppoe_threshold(
    rep_eng: &ReputationEngine,
    pkh: &PublicKeyHash,
    num_witnesses: u16,
    minimum_difficulty: u32,
    active_wips: &ActiveWips,
) -> (Hash, f64) {
    // Set minimum total_active_reputation to 1 to avoid division by zero
    let total_active_rep = std::cmp::max(rep_eng.total_active_reputation(), 1);
    // Add 1 to reputation because otherwise a node with 0 reputation would
    // never be eligible for a data request
    let my_eligibility = u64::from(rep_eng.get_eligibility(pkh)) + 1;

    let max = u64::MAX;
    // Compute target eligibility and hard-cap it if required
    let target = if active_wips.wip0016() {
        let factor = u64::from(num_witnesses);
        (max / std::cmp::max(total_active_rep, u64::from(minimum_difficulty)))
            .saturating_mul(my_eligibility)
            .saturating_mul(factor)
    } else if active_wips.third_hard_fork() {
        let factor = u64::from(rep_eng.threshold_factor(num_witnesses));
        // Eligibility must never be greater than (max/minimum_difficulty)
        std::cmp::min(
            max / u64::from(minimum_difficulty),
            (max / total_active_rep).saturating_mul(my_eligibility),
        )
        .saturating_mul(factor)
    } else {
        let factor = u64::from(rep_eng.threshold_factor(num_witnesses));
        // Check for overflow: when the probability is more than 100%, cap it to 100%
        (max / total_active_rep)
            .saturating_mul(my_eligibility)
            .saturating_mul(factor)
    };
    let target = u32::try_from(target >> 32).unwrap();

    let probability = f64::from(target) / f64::from(u32::try_from(max >> 32).unwrap());
    (Hash::with_first_u32(target), probability)
}

/// Used to classify VRF hashes into slots.
///
/// When trying to mine a block, the node considers itself eligible if the hash of the VRF is lower
/// than `calculate_randpoe_threshold(total_identities, rf, 1001,0,0)` with `rf = mining_backup_factor`.
///
/// However, in order to consolidate a block, the nodes choose the best block that is valid under
/// `rf = mining_replication_factor`. If there is no valid block within that range, it retries with
/// increasing values of `rf`. For example, with `mining_backup_factor = 4` and
/// `mining_replication_factor = 8`, there are 5 different slots:
/// `rf = 4, rf = 5, rf = 6, rf = 7, rf = 8`. Blocks in later slots can only be better candidates
/// if the previous slots have zero valid blocks.
#[derive(Clone, Debug, Default)]
pub struct VrfSlots {
    target_hashes: Vec<Hash>,
}

impl VrfSlots {
    /// Create new list of slots with the given target hashes.
    ///
    /// `target_hashes` must be sorted
    pub fn new(target_hashes: Vec<Hash>) -> Self {
        Self { target_hashes }
    }

    /// Create new list of slots with the given parameters
    pub fn from_rf(
        total_identities: u32,
        replication_factor: u32,
        backup_factor: u32,
        block_epoch: u32,
        minimum_difficulty: u32,
        epochs_with_minimum_difficulty: u32,
        active_wips: &ActiveWips,
    ) -> Self {
        Self::new(
            (replication_factor..=backup_factor)
                .map(|rf| {
                    calculate_randpoe_threshold(
                        total_identities,
                        rf,
                        block_epoch,
                        minimum_difficulty,
                        epochs_with_minimum_difficulty,
                        active_wips,
                    )
                    .0
                })
                .collect(),
        )
    }

    /// Return the slot number that contains the given hash
    pub fn slot(&self, hash: &Hash) -> u32 {
        let num_sections = self.target_hashes.len();
        u32::try_from(
            self.target_hashes
                .iter()
                // The section is the index of the first section hash that is less
                // than or equal to the provided hash
                .position(|th| hash <= th)
                // If the provided hash is greater than all of the section hashes,
                // return the number of sections
                .unwrap_or(num_sections),
        )
        .unwrap()
    }

    /// Return the target hash for each slot
    pub fn target_hashes(&self) -> &[Hash] {
        &self.target_hashes
    }
}

#[allow(clippy::many_single_char_names)]
fn internal_calculate_mining_probability(
    rf: u32,
    n: f64,
    k: u32, // k: iterative rf until reach bf
    m: i32, // M: nodes with reputation greater than me
    l: i32, // L: nodes with reputation equal than me
    r: i32, // R: nodes with reputation less than me
) -> f64 {
    if k == rf {
        let rf = f64::from(rf);
        // Prob to mine is the probability that a node with the same reputation than me mine,
        // divided by all the nodes with the same reputation:
        // 1/L * (1 - ((N-RF)/N)^L)
        let prob_to_mine = (1.0 / f64::from(l)) * (1.0 - ((n - rf) / n).powi(l));
        // Prob that a node with more reputation than me mine is:
        // ((N-RF)/N)^M
        let prob_greater_neg = ((n - rf) / n).powi(m);

        prob_to_mine * prob_greater_neg
    } else {
        let k = f64::from(k);
        // Here we take into account that rf = 1 because is only a new slot
        let prob_to_mine = (1.0 / f64::from(l)) * (1.0 - ((n - 1.0) / n).powi(l));
        // The same equation than before
        let prob_bigger_neg = ((n - k) / n).powi(m);
        // Prob that a node with less or equal reputation than me mine with a lower slot is:
        // ((N+1-RF)/N)^(L+R-1)
        let prob_lower_slot_neg = ((n + 1.0 - k) / n).powi(l + r - 1);

        prob_to_mine * prob_bigger_neg * prob_lower_slot_neg
    }
}

/// Calculate the probability that the block candidate proposed by this identity will be the
/// consolidated block selected by the network.
pub fn calculate_mining_probability(
    rep_engine: &ReputationEngine,
    own_pkh: PublicKeyHash,
    rf: u32,
    bf: u32,
) -> f64 {
    let n = u32::try_from(rep_engine.ars().active_identities_number()).unwrap();

    // In case of any active node, the probability is maximum
    if n == 0 {
        return 1.0;
    }

    // First we need to know how many nodes have more or equal reputation than us
    let own_rep = rep_engine.trs().get(&own_pkh);
    let is_active_node = rep_engine.ars().contains(&own_pkh);
    let mut greater = 0;
    let mut equal = 0;
    let mut less = 0;
    for &active_id in rep_engine.ars().active_identities() {
        let rep = rep_engine.trs().get(&active_id);
        match (rep.0 > 0, own_rep.0 > 0) {
            (true, false) => greater += 1,
            (false, true) => less += 1,
            _ => equal += 1,
        }
    }
    // In case of not being active, the equal value is plus 1.
    if !is_active_node {
        equal += 1;
    }

    if rf > n && greater == 0 {
        // In case of replication factor exceed the active node number and being the most reputed
        // we obtain the maximum probability divided in the nodes we share the same reputation
        1.0 / f64::from(equal)
    } else if rf > n && greater > 0 {
        // In case of replication factor exceed the active node number and not being the most reputed
        // we obtain the minimum probability
        0.0
    } else {
        let mut aux =
            internal_calculate_mining_probability(rf, f64::from(n), rf, greater, equal, less);
        let mut k = rf + 1;
        while k <= bf && k <= n {
            aux += internal_calculate_mining_probability(rf, f64::from(n), k, greater, equal, less);
            k += 1;
        }
        aux
    }
}
