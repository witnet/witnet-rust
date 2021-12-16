use std::{cmp::Ordering, convert::TryFrom};

use witnet_data_structures::{
    chain::RADTally,
    mainnet_validations::ActiveWips,
    radon_error::RadonError,
    radon_report::{RadonReport, ReportContext, Stage, TallyMetaData},
};

use crate::{
    error::RadError,
    filters,
    reducers::mode::mode,
    run_tally_report,
    script::RadonScriptExecutionSettings,
    types::{array::RadonArray, RadonType, RadonTypes},
};

/// An `Either`-like structure that covers the two possible return types of the
/// `evaluate_tally_precondition_clause` method.
#[derive(Debug)]
pub enum TallyPreconditionClauseResult {
    /// The result of the data request is a valid value
    MajorityOfValues {
        /// All the values reported by witnesses, including errors
        values: Vec<RadonTypes>,
        /// Bit vector marking witnesses as liars
        liars: Vec<bool>,
        /// Bit vector marking witnesses as errors
        errors: Vec<bool>,
    },
    /// The result of the data request is an error
    MajorityOfErrors {
        /// Mode of all the reported RadonErrors, or ModeError if there is no mode
        errors_mode: RadonError<RadError>,
    },
}

/// Run a precondition clause on an array of `RadonTypes` so as to check if the mode is a value or
/// an error, which has clear consequences in regards to consensus, rewards and punishments.
// FIXME: Allow for now, since there is no safe cast function from a usize to float yet
#[allow(clippy::cast_precision_loss)]
pub fn evaluate_tally_precondition_clause(
    reveals: Vec<RadonReport<RadonTypes>>,
    minimum_consensus: f64,
    num_commits: usize,
    active_wips: &ActiveWips,
) -> Result<TallyPreconditionClauseResult, RadError> {
    // Short-circuit if there were no commits
    if num_commits == 0 {
        return Err(RadError::InsufficientCommits);
    }
    // Short-circuit if there were no reveals
    if reveals.is_empty() {
        return Err(RadError::NoReveals);
    }

    let error_type_discriminant =
        RadonTypes::RadonError(RadonError::try_from(RadError::default()).unwrap()).discriminant();

    // Count how many times is each RADON type featured in `reveals`, but count `RadonError` items
    // separately as they need to be handled differently.
    let reveals_len = u32::try_from(reveals.len()).unwrap();
    let mut counter = Counter::new(RadonTypes::num_types());
    let mut errors = vec![];
    for reveal in &reveals {
        counter.increment(reveal.result.discriminant());
        if reveal.result.discriminant() == error_type_discriminant {
            errors.push(true);
        } else {
            errors.push(false);
        }
    }

    let mut num_commits_f = f64::from(u32::try_from(num_commits).unwrap());
    // Compute ratio of type consensus amongst reveals (percentage of reveals that have same type
    // as the frequent type).
    let achieved_consensus = f64::from(counter.max_val) / num_commits_f;

    if !active_wips.wips_0009_0011_0012() {
        // Before the second hard fork, the achieved_consensus below was incorrectly calculated as
        // max_count / num_reveals
        num_commits_f = f64::from(reveals_len);
    }

    // If the achieved consensus is over the user-defined threshold, continue.
    // Otherwise, return `RadError::InsufficientConsensus`.
    if achieved_consensus >= minimum_consensus {
        // Decide based on the most frequent type.
        match counter.max_pos {
            // Handle tie cases (there is the same amount of revealed values for multiple types).
            None => Err(RadError::ModeTie {
                values: RadonArray::from(
                    reveals
                        .into_iter()
                        .map(RadonReport::into_inner)
                        .collect::<Vec<RadonTypes>>(),
                ),
                max_count: u16::try_from(counter.max_val).unwrap(),
            }),
            // Majority of errors, return errors mode.
            Some(most_frequent_type) if most_frequent_type == error_type_discriminant => {
                let errors: Vec<RadonTypes> = reveals
                    .into_iter()
                    .filter_map(|reveal| match reveal.into_inner() {
                        radon_types @ RadonTypes::RadonError(_) => Some(radon_types),
                        _ => None,
                    })
                    .collect();

                let errors_array = RadonArray::from(errors);
                // Use the mode filter to get the count of the most common error.
                // That count must be greater than or equal to minimum_consensus,
                // otherwise RadError::InsufficientConsensus is returned
                let most_common_error_array =
                    filters::mode::mode_filter(&errors_array, &mut ReportContext::default());

                match most_common_error_array {
                    Ok(RadonTypes::Array(x)) => {
                        let x_value = x.value();
                        let achieved_consensus = x_value.len() as f64 / num_commits_f;
                        if achieved_consensus >= minimum_consensus {
                            match mode(&errors_array)? {
                                RadonTypes::RadonError(errors_mode) => {
                                    Ok(TallyPreconditionClauseResult::MajorityOfErrors { errors_mode })
                                }
                                _ => unreachable!("Mode of `RadonArray` containing only `RadonError`s cannot possibly be different from `RadonError`"),
                            }
                        } else {
                            Err(RadError::InsufficientConsensus {
                                achieved: achieved_consensus,
                                required: minimum_consensus,
                            })
                        }
                    }
                    Ok(_) => {
                        unreachable!("Mode filter should always return a `RadonArray`");
                    }
                    Err(RadError::ModeTie { values, max_count }) => {
                        let achieved_consensus = f64::from(max_count) / num_commits_f;
                        if achieved_consensus < minimum_consensus {
                            Err(RadError::InsufficientConsensus {
                                achieved: achieved_consensus,
                                required: minimum_consensus,
                            })
                        } else {
                            // This is only possible if minimum_consensus <= 0.50
                            Err(RadError::ModeTie { values, max_count })
                        }
                    }
                    Err(e) => panic!(
                        "Unexpected error when applying filter_mode on array of errors: {}",
                        e
                    ),
                }
            }
            // Majority of values, compute and filter liars
            Some(most_frequent_type) => {
                let mut liars = vec![];
                let results = reveals
                    .into_iter()
                    .filter_map(|reveal| {
                        let radon_types = reveal.into_inner();
                        let condition = most_frequent_type == radon_types.discriminant();
                        update_liars(&mut liars, radon_types, condition)
                    })
                    .collect();

                Ok(TallyPreconditionClauseResult::MajorityOfValues {
                    values: results,
                    liars,
                    errors,
                })
            }
        }
    } else {
        Err(RadError::InsufficientConsensus {
            achieved: achieved_consensus,
            required: minimum_consensus,
        })
    }
}

/// Check that after applying the tally filter the consensus percentage is still good enough.
// FIXME: Allow for now, since there is no safe cast function from a usize to float yet
#[allow(clippy::cast_precision_loss)]
pub fn evaluate_tally_postcondition_clause(
    report: RadonReport<RadonTypes>,
    minimum_consensus: f64,
    commits_count: usize,
) -> RadonReport<RadonTypes> {
    if let Stage::Tally(metadata) = report.context.stage.clone() {
        let error_type_discriminant =
            RadonTypes::RadonError(RadonError::try_from(RadError::default()).unwrap())
                .discriminant();
        // If the result is already a RadonError, return that error.
        // The result can be a RadonError in these scenarios:
        // * There is insufficient consensus before running the tally script
        // * There is consensus on an error value before running the tally script
        // * There is consensus on a non-error value but the tally script results in an error
        // In all of this cases we want to keep the old error.
        if report.result.discriminant() == error_type_discriminant {
            return report;
        }

        let achieved_consensus = metadata.liars.iter().fold(0., |count, liar| match liar {
            true => count,
            false => count + 1.,
        }) / commits_count as f64;
        if achieved_consensus > minimum_consensus {
            report
        } else {
            // If there is insufficient consensus, all revealers are set to error and liar.
            // This is the case of error out of consensus, which is an error that is not penalized
            // and not rewarded.
            let num_reveals = metadata.liars.len();
            radon_report_from_error(
                RadError::InsufficientConsensus {
                    achieved: achieved_consensus,
                    required: minimum_consensus,
                },
                num_reveals,
            )
        }
    } else {
        panic!("Report context must be in tally stage");
    }
}

/// Construct a `RadonReport` from a `TallyPreconditionClauseResult`
pub fn construct_report_from_clause_result(
    clause_result: Result<TallyPreconditionClauseResult, RadError>,
    script: &RADTally,
    reports_len: usize,
    active_wips: &ActiveWips,
) -> RadonReport<RadonTypes> {
    // This TallyMetadata would be included in case of Error Result, in that case,
    // no one has to be classified as a lier, but everyone as an error
    let mut metadata = TallyMetaData::default();
    metadata.update_liars(vec![false; reports_len]);
    metadata.errors = vec![true; reports_len];
    match clause_result {
        // The reveals passed the precondition clause (a parametric majority of them were successful
        // values). Run the tally, which will add more liars if any.
        Ok(TallyPreconditionClauseResult::MajorityOfValues {
            values,
            liars,
            errors,
        }) => {
            let mut metadata = TallyMetaData::default();
            metadata.update_liars(vec![false; reports_len]);

            match run_tally_report(
                values,
                script,
                Some(liars),
                Some(errors),
                RadonScriptExecutionSettings::all_but_partial_results(),
                active_wips.clone(),
            ) {
                (Ok(x), _) => x,
                (Err(e), _) => {
                    if active_wips.wips_0009_0011_0012() {
                        radon_report_from_error(
                            RadError::TallyExecution {
                                inner: Some(Box::new(e)),
                                message: None,
                            },
                            reports_len,
                        )
                    } else {
                        RadonReport::from_result(
                            Err(RadError::TallyExecution {
                                inner: Some(Box::new(e)),
                                message: None,
                            }),
                            &ReportContext::from_stage(Stage::Tally(metadata)),
                        )
                    }
                }
            }
        }
        // The reveals did not pass the precondition clause (a parametric majority of them were
        // errors). Tally will not be run, and the mode of the errors will be committed.
        Ok(TallyPreconditionClauseResult::MajorityOfErrors { errors_mode }) => {
            // Do not impose penalties on any of the revealers.
            RadonReport::from_result(
                Ok(RadonTypes::RadonError(errors_mode)),
                &ReportContext::from_stage(Stage::Tally(metadata)),
            )
        }
        // Failed to evaluate the precondition clause. `RadonReport::from_result()?` is the last
        // chance for errors to be intercepted and used for consensus.
        Err(e) => {
            if active_wips.wips_0009_0011_0012() {
                // If there is an error during the precondition, all revealers are set to error and liar.
                // This is an error that is not penalized and not rewarded.
                radon_report_from_error(e, reports_len)
            } else {
                RadonReport::from_result(Err(e), &ReportContext::from_stage(Stage::Tally(metadata)))
            }
        }
    }
}

fn update_liars(liars: &mut Vec<bool>, item: RadonTypes, condition: bool) -> Option<RadonTypes> {
    liars.push(!condition);
    if condition {
        Some(item)
    } else {
        None
    }
}

/// Create report with error result and mark all revealers as errors and liars
pub fn radon_report_from_error(rad_error: RadError, num_reveals: usize) -> RadonReport<RadonTypes> {
    let metadata = TallyMetaData {
        errors: vec![true; num_reveals],
        liars: vec![true; num_reveals],
        ..Default::default()
    };

    RadonReport::from_result(
        Err(rad_error),
        &ReportContext::from_stage(Stage::Tally(metadata)),
    )
}

/// An histogram-like counter that helps counting occurrences of different numeric categories.
struct Counter {
    /// Tracks the position inside `values` of the category that appears the most.
    /// This MUST be initialized to `None`.
    /// As long as `values` is not empty, `None` means there was a tie between multiple categories.
    max_pos: Option<usize>,
    /// Tracks how many times does the most frequent category appear.
    /// This is a cached version of `self.values[self.max_pos]`.
    max_val: i32,
    /// Tracks how many times does each different category appear.
    categories: Vec<i32>,
}

/// Implementations for `struct Counter`
impl Counter {
    /// Increment by one the counter for a particular category.
    fn increment(&mut self, category_id: usize) {
        // Increment the counter by 1.
        self.categories[category_id] += 1;

        // Tell whether `max_pos` and `max_val` need to be updated.
        match self.categories[category_id].cmp(&self.max_val) {
            // If the recently updated counter is less than `max_pos`, do nothing.
            Ordering::Less => {}
            // If the recently updated counter is equal than `max_pos`, it is a tie.
            Ordering::Equal => {
                self.max_pos = None;
            }
            // If the recently updated counter outgrows `max_pos`, update `max_val` and `max_pos`.
            Ordering::Greater => {
                self.max_val = self.categories[category_id];
                self.max_pos = Some(category_id);
            }
        }
    }

    /// Create a new `struct Counter` that is initialized to truck a provided number of categories.
    fn new(n: usize) -> Self {
        let categories = vec![0; n];

        Self {
            max_pos: None,
            max_val: 0,
            categories,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let mut counter = Counter::new(7);
        counter.increment(6);
        assert_eq!(counter.max_val, 1);
        assert_eq!(counter.max_pos, Some(6));

        counter.increment(6);
        assert_eq!(counter.max_val, 2);
        assert_eq!(counter.max_pos, Some(6));

        counter.increment(0);
        assert_eq!(counter.max_val, 2);
        assert_eq!(counter.max_pos, Some(6));

        counter.increment(0);
        counter.increment(0);
        assert_eq!(counter.max_val, 3);
        assert_eq!(counter.max_pos, Some(0));

        counter.increment(6);
        assert_eq!(counter.max_val, 3);
        assert_eq!(counter.max_pos, None);
    }
}
