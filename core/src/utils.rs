use std::collections::HashMap;
use witnet_data_structures::chain::{Input, Output, OutputPointer};

/// find unspent outputs in unspent outputs
pub fn find_unspent_outputs<S: ::std::hash::BuildHasher>(
    unspent_outputs: &HashMap<OutputPointer, Output, S>,
    inputs: &[Input],
) -> bool {
    inputs.iter().all(|tx_input| {
        let output_pointer = match tx_input {
            Input::Commit(tx) => OutputPointer {
                transaction_id: tx.transaction_id,
                output_index: tx.output_index,
            },
            Input::Reveal(tx) => OutputPointer {
                transaction_id: tx.transaction_id,
                output_index: tx.output_index,
            },
            Input::DataRequest(tx) => OutputPointer {
                transaction_id: tx.transaction_id,
                output_index: tx.output_index,
            },
            Input::ValueTransfer(tx) => OutputPointer {
                transaction_id: tx.transaction_id,
                output_index: tx.output_index,
            },
        };

        unspent_outputs.contains_key(&output_pointer)
    })
}

/// get output pointed for input
pub fn get_output_from_input<S: ::std::hash::BuildHasher>(
    unspent_outputs: &HashMap<OutputPointer, Output, S>,
    input: &Input,
) -> Output {
    let output_pointer = match input {
        Input::Commit(tx) => OutputPointer {
            transaction_id: tx.transaction_id,
            output_index: tx.output_index,
        },
        Input::DataRequest(tx) => OutputPointer {
            transaction_id: tx.transaction_id,
            output_index: tx.output_index,
        },
        Input::Reveal(tx) => OutputPointer {
            transaction_id: tx.transaction_id,
            output_index: tx.output_index,
        },
        Input::ValueTransfer(tx) => OutputPointer {
            transaction_id: tx.transaction_id,
            output_index: tx.output_index,
        },
    };

    unspent_outputs[&output_pointer].clone()
}

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
