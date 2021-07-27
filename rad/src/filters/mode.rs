use crate::{
    error::RadError,
    reducers::mode::mode,
    types::{array::RadonArray, RadonType, RadonTypes},
};
use witnet_data_structures::radon_report::{ReportContext, Stage};

pub fn mode_filter(
    input: &RadonArray,
    context: &mut ReportContext<RadonTypes>,
) -> Result<RadonTypes, RadError> {
    let mode = mode(input)?;
    let mut liars = vec![];

    let filtered_vec: Vec<RadonTypes> = input
        .value()
        .into_iter()
        .filter(|rad_types| {
            let cond = rad_types == &mode;
            liars.push(!cond);
            cond
        })
        .collect();

    if let Stage::Tally(ref mut metadata) = context.stage {
        metadata.update_liars(liars);
    }

    Ok(RadonArray::from(filtered_vec).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{integer::RadonInteger, string::RadonString};
    use witnet_data_structures::radon_report::TallyMetaData;

    // Helper function which works with Rust integers, to remove RadonTypes from tests
    fn imode(
        input_i128: &[i128],
        ctx: &mut ReportContext<RadonTypes>,
    ) -> Result<Vec<i128>, RadError> {
        let input_vec: Vec<RadonTypes> = input_i128
            .iter()
            .map(|f| RadonTypes::Integer(RadonInteger::from(*f)))
            .collect();
        let input = RadonArray::from(input_vec);

        let output = mode_filter(&input, ctx)?;

        let output_vec = match output {
            RadonTypes::Array(x) => x.value(),
            _ => panic!("Filter method should return a RadonArray"),
        };
        let output_i128 = output_vec
            .into_iter()
            .map(|r| match r {
                RadonTypes::Integer(x) => x.value(),
                _ => panic!("Filter method should return an array of integers"),
            })
            .collect();

        Ok(output_i128)
    }

    #[test]
    fn test_filter_mode_empty() {
        let input = vec![];
        let expected = RadError::ModeEmpty;

        let mut ctx = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };
        let output = imode(&input, &mut ctx).unwrap_err();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_filter_mode_integer_one() {
        let input = vec![1];
        let expected = input.clone();

        let mut ctx = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };
        let output = imode(&input, &mut ctx).unwrap();
        assert_eq!(output, expected);

        if let Stage::Tally(metadata) = ctx.stage {
            assert_eq!(metadata.liars, vec![false]);
        } else {
            panic!("Not tally stage");
        }
    }

    #[test]
    fn test_filter_mode_integer_tie() {
        let input = vec![1, 2];
        let expected = RadError::ModeTie {
            values: RadonArray::from(vec![
                RadonInteger::from(1).into(),
                RadonInteger::from(2).into(),
            ]),
            max_count: 1,
        };

        let mut ctx = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };
        let output = imode(&input, &mut ctx).unwrap_err();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_filter_mode_integer() {
        let input = vec![1, 2, 2, 2, 3, 1];
        let expected = vec![2, 2, 2];

        let mut ctx = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };
        let output = imode(&input, &mut ctx).unwrap();
        assert_eq!(output, expected);

        if let Stage::Tally(metadata) = ctx.stage {
            assert_eq!(metadata.liars, vec![true, false, false, false, true, true]);
        } else {
            panic!("Not tally stage");
        }
    }

    // Helper function which works with Rust Strings, to remove RadonTypes from tests
    fn strmode(
        input_string: &[String],
        ctx: &mut ReportContext<RadonTypes>,
    ) -> Result<Vec<String>, RadError> {
        let input_vec: Vec<RadonTypes> = input_string
            .iter()
            .map(|f| RadonTypes::String(RadonString::from(f.clone())))
            .collect();
        let input = RadonArray::from(input_vec);

        let output = mode_filter(&input, ctx)?;

        let output_vec = match output {
            RadonTypes::Array(x) => x.value(),
            _ => panic!("Filter method should return a RadonArray"),
        };
        let output_string = output_vec
            .into_iter()
            .map(|r| match r {
                RadonTypes::String(x) => x.value(),
                _ => panic!("Filter method should return an array of integers"),
            })
            .collect();

        Ok(output_string)
    }

    #[test]
    fn test_filter_mode_string_one() {
        let input = vec!["Hello".to_string()];
        let expected = input.clone();

        let mut ctx = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };
        let output = strmode(&input, &mut ctx).unwrap();
        assert_eq!(output, expected);

        if let Stage::Tally(metadata) = ctx.stage {
            assert_eq!(metadata.liars, vec![false]);
        } else {
            panic!("Not tally stage");
        }
    }

    #[test]
    fn test_filter_mode_string_tie() {
        let input = vec!["Hello".to_string(), "World".to_string()];
        let expected = RadError::ModeTie {
            values: RadonArray::from(vec![
                RadonString::from("Hello").into(),
                RadonString::from("World").into(),
            ]),
            max_count: 1,
        };

        let mut ctx = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };
        let output = strmode(&input, &mut ctx).unwrap_err();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_filter_mode_string() {
        let str1 = "Hello".to_string();
        let str2 = "World".to_string();
        let str3 = "Rust".to_string();
        let input = vec![
            str1.clone(),
            str2.clone(),
            str2.clone(),
            str3,
            str2.clone(),
            str1,
        ];
        let expected = vec![str2.clone(), str2.clone(), str2];

        let mut ctx = ReportContext {
            stage: Stage::Tally(TallyMetaData::default()),
            ..ReportContext::default()
        };
        let output = strmode(&input, &mut ctx).unwrap();
        assert_eq!(output, expected);

        if let Stage::Tally(metadata) = ctx.stage {
            assert_eq!(metadata.liars, vec![true, false, false, true, false, true]);
        } else {
            panic!("Not tally stage");
        }
    }
}
