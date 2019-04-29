Run the benchmarks with:

```sh
cargo bench
```

# Reputation Engine Architecture Proposal
// TODO(#626): Review reputation info used from https://github.com/witnet/research/issues/24
## Context
For the proposed reputation system to work, any potential implementations would need to contain data structures and algorithms allowing for efficient knowledge of:
1. the reputation score associated to every known identity in the system,
2. the list and count of different identities recently involved in the witnessing protocol,
3. the summation of the reputation score associated to every identity in (2)

An identity is considered to be _involved in the witnessing protocol_ or _active_ if it has had one or more _proofs of eligibility_ included in blocks in the last _λ<sub>a</sub>_ epochs (_active window_). This _λ<sub>a</sub>_ period is a protocol constant that represents a time window that restricts how back in time we need to go for summing up the reputation of those identities considered _active_.

Reputation is subject to expiration. But this expiration is not tied to human time but to _network activity_, i.e. a separate clock _α_ that ticks (increments by one) every time a _witnessing act_<sup>1</sup> takes place. Initially<sup>2</sup>, the protocol constant setting the reputation expiration period _λ<sub>e</sub>_ can be a fixed amount of witnessing acts, i.e. reputation gained during an epoch that started when _α_ was _n_ will expire in the first epoch in which _α_ gets over _n + λ<sub>e</sub>_.

## Data structures

We propose two new data structures to exist, both being linked to and persisted atomically along `ChainState`:
- Total Reputation Set (`TRS`): keeps a pre-computed view on what is the total reputation in force (not expired) for every identity as of the last epoch in `ChainState`. This fulfills point (1) above.
- Active Reputation Set (`ARS`): keeps track of what are the active identities and how many times they have appeared in the active window. This fulfills point (2) above.

Both structures contain a companion queue that keeps track of additional metadata needed for efficiently updating them and fulfilling point (3) above.

### Total Reputation Set (`TRS`)
The `TRS` wraps a `HashMap` that has this internal shape:
```rust
type TrsMap = HashMap<PublicKeyHash, ReputationAmount>;
```
The type for `ReputationAmount` has not been decided upon yet, but an `u32` should be more than enough under the assumption that reputation points can not be fractioned and the total emission of reputation points possible is `2^20`.

As mentioned before, this structure has a companion queue that helps _versioning_ it so that it can be updated more efficiently:
```rust
type ReputationDiff = (PublicKeyHash, ReputationAmount);
type Expirator = (Alpha, Vec<ReputationDiff>);
type TrsQueue = VecDeque<Expirator>;
```

Neither of the internal data structures of the `TRS` (`TrsMap` and `TrsQueue`) can therefore be read or written directly. They can only be accessed through getter/setters that abstract away the internal structures and expose an unified interface that is easier to interact with and reason about:
```rust
// Everything here is pseudo-Rust, it's obviously not guaranteed to compile
struct TotalReputationSet {
  map: TrsMap,
  queue: TrsQueue
}

impl TotalReputationSet {
  fn gain(&mut self, diff: ReputationDiff, expiration: Alpha) {
    let (identity, amount) = diff;
    let old_reputation = self.map
      .get(identity)
      .unwrap_or_default();
    let expirator: Expirator = (expiration, diff);
    self.map
      .insert(identity, old_reputation + amount);
    self.queue
      .add(expirator);
  }

  fn expire(&mut self, until_alpha: Alpha) {
    // For all expirators in the front of the queue with `alpha < until_alpha`:
    // - consume them
    // - detract `expirator.amount` from `self.map[expirator.identity]`
    // - if `self.map[expirator.identity] <= 0`, drop the map entry
  }

  fn penalize(&mut self, identity: PublicKeyHash, factor: PenalizationFactor) -> u32 {
    let old_reputation = self.map
      .get(identity)
      .unwrap_or_default();
    let penalization_amount = old_reputation * factor;
    // Read the queue from the back, consuming as many queue items as needed to sum
    //   more than `punishment_amount`. If the last item has more amount than needed, mutate it
    //   without dropping it from the queue. 
    /*  ...  */

    penalization_amount
  }
}
```

### Active Reputation Set (`ARS`)
The active reputation set is also comprised internally by two different data structures with a unified interface.

```rust

type ArsMap = HashMap<PublicKeyHash, u16>;
type ArsBuffer = CircularBuffer<HashSet<PublicKeyHash>>;

struct ActiveReputationSet {
  map: ArsMap,
  buffer: ArsBuffer
}
```

`ArsBuffer` is a capped queue of fixed length _λ<sub>a</sub>_ whose entries are vectors containing all the `PublicKeyHash`es of identities that were active during the last _λ<sub>a</sub>_ epochs.

`ArsMap` keeps track of how many entries exist in `ArsBuffer` that contain each identity. This is, how many times each `PublicKeyHash` appears in `ArsBuffer`. This, in addition of telling whether an identity has been active during _λ<sub>a</sub>_ epochs, gives some insights on how active it was. Intuitively, a single identity will appear at most _λ<sub>a</sub>_ times in `ArsBuffer`.

The `impl`s for `ActiveReputationSet` are pretty straightforward:
```rust
impl ActiveReputationSet {
  fn push_activity(identities: HashSet<PublicKeyHash>) {
    identities.for_each(|identity| {
        let current_activity = self.map
          .get(identity)
          .unwrap_or_default();
        self.map.insert(identity, current_activity + 1);
    });
    let identities_with_expired_reputation = self.queue
      .add(identities)?;
    identities_with_expired_reputation
      .iter()
      .for_each(|identity| {
        let current_activity = self.map
          .get(identity)
          .unwrap_or_default();
        if current_activity > 1 {
          self.map.insert(identity, current_activity);
        } else {
          self.map.remove(identity);
        }
      })
  }
}
```

## Changes to block validation process
For the reputation engine to have the necessary reputation-related data that is derived from tally transactions, some changes are needed in the block validation process:

The block candidate validation function has two new fields in its return type:
- `alpha_diff: Alpha`: this field acts as a counter for tracking how many times should the activity clock _α_ tick after eventually consolidating the candidate. For every tally transaction included in the block, this counter should increase by as much as the _witness target_. See footnote<sup>1</sup>.
- `truthness_map: HashMap<PublicKeyHash, Vec<bool>>`: this field tracks how aligned with the consensus has every of the identities involved in the witnessing protocol been. For each tally, we should take the tally matrix, associate it with the revealers  and push the `bool` from the tally into `truthness_map[identity]`. Example below:

```rust
fn run_tally(reveals: Vec<RevealTransaction>, tally_script: RadonScript) {
  let (revealers, claims) = reveals.iter().map(|reveal| (reveal.signature.key, reveal.body.claim)).collect();
  let (result, tally_vector) = Rad::operate(claims, tally_script);

  (result, revealers
    .iter()
    .zip(tally_matrix)
    .collect())
}

let truthness_map: HashMap<PublicKeyHash, Vec<bool>> = HashMap::new();

for tally in block.tallies {
  let (result, truthness_vec) = run_tally(reveals, tally.script);
  for (identity, truthness) in truthness_vec {
      let old_truthness = truthness_map
        .get(identity)
        .unwrap_or_default();
      truthness_map
        .insert(identity, old_truthness.push(truthness))
  }
}
```

## Reputation updating process
Upon consolidation of a valid block, we will get our final `alpha_diff` and `truthness_map`.

At this point `chain_state.alpha` needs to be incremented by as much as `alpha_diff`.

`truthness_map` will contain at this point a vector of truths and lies (denoted by `true` or `false`) for every identity that participated in the last epoch. We need to partition this vector to tell the truthers from the liars:

```rust
fn count_lies(entry: (PublicKeyHash, Vec<bool>)) -> (PublicKeyHash, u16) {
  (identity, truthness_vector) = entry;
  let lies_count = truthness_vector
   .iter()
   .filter(|was_true| was_true)
   .count();
  (identity, lies_count)
}

fn tell_truthers_from_liars(entry: (PublicKeyHash, u16)) -> bool {
  (_identity, lies_count) = entry;
  lies_count > 0
}

let (truthers, liars) = truthness_map
  .iter()
  .map(count_lies)
  .partition(tell_truthers_from_liars)
  .collect();
```

At this point, we have everything we need to:
- Initialize a `reputation_bounty: u32` accumulator to zero
- Remove expired reputation from `TRS` by calling `trs.expire(chain_state.alpha)`
- Increase the reputation bounty by the same amount of the computed reputation issuance (roughly `reputation_bounty += D * alpha_diff` where `D` is right number of reputation points to be issued in the last epoch for every _α_ tick / witnessing act)
- Apply penalizations by calling `trs.penalize(identity, factor)` for every `identity` in `liars`, using `Π^Λ` as `factor`, that is, the penalization constant (between 0 and 1) to the power of the number of lies. E.g. with `Π = 0.8` some identity lying three times will see its reputation multiplied by a factor of `0.512`, thus losing almost 48.8% of the reputation it used to have as of the last epoch.
- Increase the reputation bounty by the same amount as the total number of reputation points that has been detracted from the liars.
- Calculate the truther reward, roughly `reward = rep_bounty / truthers.len()`.
- Insert the reputation rewards into the `TRS` by calling `total_reputation_set.gain((identity, reward), chain_state.alpha + lambda_e)` for every truther, where `lambda_e` is _λ<sub>a</sub>_ (reputation expiration period).
- Update the `ARS` by calling `ars.push_activity(revealers.iter().map(|(identity, _)| identity).collect::HashSet<PublicKeyHash>())`

## Footnotes
<sup>1</sup>: For every single tally transaction in a block, the number of witnessing acts it brings is the summation of all the `witness_target` values in the data requests they are finalizing, which will match the number of commitments in the chain for the same data request. Upon consolidation of a block, the activity clock _α_ gets incremented as much as the summation of witnessing acts brought by all the tally transactions it contains.
<sup>2</sup>: In the future we can make _λ<sub>e</sub>_ stochastic, alike to [nuclear decay]

[nuclear decay]: https://en.wikipedia.org/wiki/Radioactive_decay
