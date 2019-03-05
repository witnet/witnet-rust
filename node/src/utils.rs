use std::collections::HashMap;
use std::hash::Hash;
use witnet_data_structures::chain::{Input, Output};

/// Check if an output is a consensus output
pub fn is_tally_output(output: &Output) -> bool {
    match output {
        Output::Tally(_) => true,
        _ => false,
    }
}

/// Check if an output is a reveal output
pub fn is_reveal_output(output: &Output) -> bool {
    match output {
        Output::Reveal(_) => true,
        _ => false,
    }
}

/// Check if input is data request input
pub fn is_data_request_input(input: &Input) -> bool {
    match input {
        Input::DataRequest(_) => true,
        _ => false,
    }
}

/// Check if output is commit output
pub fn is_commit_output(output: &Output) -> bool {
    match output {
        Output::Commit(_) => true,
        _ => false,
    }
}

/// Check if input is commit input
pub fn is_commit_input(input: &Input) -> bool {
    match input {
        Input::Commit(_) => true,
        _ => false,
    }
}

/// Check if output is value transfer output
pub fn is_value_transfer_output(output: &Output) -> bool {
    match output {
        Output::ValueTransfer(_) => true,
        _ => false,
    }
}

/// Validate value transfer output
pub fn validate_value_transfer_output_position(outputs: &[Output]) -> bool {
    let is_value_transfer_output = |output: &&Output| match output {
        Output::ValueTransfer(_) => true,
        _ => false,
    };

    outputs
        .iter()
        .rev()
        .take_while(is_value_transfer_output)
        .eq(outputs.iter().rev().filter(is_value_transfer_output))
}

/// Count consensus outputs
pub fn count_tally_outputs(outputs: &[Output]) -> usize {
    outputs.iter().filter(|x| is_tally_output(x)).count()
}

/// Validate tally output uniqueness
pub fn validate_tally_output_uniqueness(outputs: &[Output]) -> bool {
    count_tally_outputs(outputs) == 1
}

/// Given a list of elements, return the most common one. In case of tie, return `None`.
pub fn mode_consensus<'a, I, V>(pb: I) -> Option<&'a V>
where
    I: Iterator<Item = &'a V>,
    V: Eq + Hash,
{
    let mut bp = HashMap::new();
    for k in pb {
        *bp.entry(k).or_insert(0) += 1;
    }

    let mut bpv: Vec<_> = bp.into_iter().collect();
    // Sort (beacon, peers) by number of peers
    bpv.sort_unstable_by(|a, b| b.1.cmp(&a.1));

    if bpv.len() >= 2 && bpv[0].1 == bpv[1].1 {
        // In case of tie, no consensus
        None
    } else {
        // Otherwise, the first element is the most common
        bpv.into_iter().map(|(k, _count)| k).next()
    }
}
