//! # RAD Engine

pub use serde_cbor::to_vec as cbor_to_vec;
pub use serde_cbor::Value as CborValue;

use witnet_data_structures::{
    chain::{RADAggregate, RADRequest, RADRetrieve, RADTally, RADType},
    radon_report::{RadonReport, ReportContext, Stage, TallyMetaData},
};

use crate::{
    error::RadError,
    script::{
        create_radon_script_from_filters_and_reducer, execute_radon_script, unpack_radon_script,
        RadonScriptExecutionSettings,
    },
    types::{array::RadonArray, string::RadonString, RadonTypes},
};

pub mod error;
pub mod filters;
pub mod hash_functions;
pub mod operators;
pub mod reducers;
pub mod script;
pub mod types;

pub type Result<T> = std::result::Result<T, RadError>;

pub async fn try_data_request(
    request: &RADRequest,
    settings: RadonScriptExecutionSettings,
) -> RadonReport<RadonTypes> {
    let context = &mut ReportContext::default();

    let retrieve_responses_fut = request
        .retrieve
        .iter()
        .map(|retrieve| run_retrieval_report(retrieve, settings));

    let retrieve_responses: Vec<RadonTypes> = futures::future::join_all(retrieve_responses_fut)
        .await
        .into_iter()
        .map(|retrieve| {
            retrieve
                .unwrap_or_else(|error| RadonReport::from_result(Err(error), context))
                .into_inner()
        })
        .collect();

    let aggregation_response =
        run_aggregation_report(retrieve_responses, &request.aggregate, settings)
            .unwrap_or_else(|error| RadonReport::from_result(Err(error), context))
            .into_inner();

    run_tally_report(
        vec![aggregation_response],
        &request.tally,
        None,
        None,
        settings,
    )
    .unwrap_or_else(|error| RadonReport::from_result(Err(error), context))
}

/// Run retrieval without performing any external network requests, return `RadonReport`.
pub fn run_retrieval_with_data_report(
    retrieve: &RADRetrieve,
    response: String,
    context: &mut ReportContext,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    match retrieve.kind {
        RADType::HttpGet => {
            let input = RadonTypes::from(RadonString::from(response));
            let radon_script = unpack_radon_script(&retrieve.script)?;

            execute_radon_script(input, &radon_script, context, settings)
        }
    }
}

/// Run retrieval without performing any external network requests, return `RadonTypes`.
pub fn run_retrieval_with_data(
    retrieve: &RADRetrieve,
    response: String,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonTypes> {
    let context = &mut ReportContext::default();
    run_retrieval_with_data_report(retrieve, response, context, settings)
        .map(RadonReport::into_inner)
}

/// Run retrieval stage of a data request, return `RadonReport`.
pub async fn run_retrieval_report(
    retrieve: &RADRetrieve,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    let context = &mut ReportContext::default();
    context.stage = Stage::Retrieval;

    match retrieve.kind {
        RADType::HttpGet => {
            // Validate URL because surf::get panics on invalid URL
            // It could still panic if surf gets updated and changes their URL parsing library
            let _valid_url =
                url::Url::parse(&retrieve.url).map_err(|err| RadError::UrlParseError {
                    inner: err,
                    url: retrieve.url.clone(),
                })?;

            let mut response = surf::get(&retrieve.url)
                .await
                .map_err(|x| RadError::HttpOther {
                    message: x.to_string(),
                })?;

            if !response.status().is_success() {
                return Err(RadError::HttpStatus {
                    status_code: response.status().into(),
                });
            }

            let response_string = response
                // TODO: replace with .body_bytes() and let RADON handle the encoding?
                .body_string()
                .await
                .map_err(|x| RadError::HttpOther {
                    message: x.to_string(),
                })?;

            let result =
                run_retrieval_with_data_report(retrieve, response_string, context, settings);

            match &result {
                Ok(report) => {
                    log::debug!(
                        "Successful result for source {}: {:?}",
                        retrieve.url,
                        report.result
                    );
                }
                Err(e) => log::debug!("Failed result for source {}: {:?}", retrieve.url, e),
            }

            result
        }
    }
}

/// Run retrieval stage of a data request, return `RadonTypes`.
pub async fn run_retrieval(retrieve: &RADRetrieve) -> Result<RadonTypes> {
    // Disable all execution tracing features, as this is the best-effort version of this method
    run_retrieval_report(retrieve, RadonScriptExecutionSettings::disable_all())
        .await
        .map(RadonReport::into_inner)
}

/// Run aggregate stage of a data request, return `RadonReport`.
pub fn run_aggregation_report(
    radon_types_vec: Vec<RadonTypes>,
    aggregate: &RADAggregate,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    let context = &mut ReportContext::default();
    context.stage = Stage::Aggregation;

    let filters = aggregate.filters.as_slice();
    let reducer = aggregate.reducer;
    let radon_script = create_radon_script_from_filters_and_reducer(filters, reducer)?;

    let items_to_aggregate = RadonTypes::from(RadonArray::from(radon_types_vec));

    execute_radon_script(items_to_aggregate, &radon_script, context, settings)
}

/// Run aggregate stage of a data request, return `RadonTypes`.
pub fn run_aggregation(
    radon_types_vec: Vec<RadonTypes>,
    aggregate: &RADAggregate,
) -> Result<RadonTypes> {
    // Disable all execution tracing features, as this is the best-effort version of this method
    run_aggregation_report(
        radon_types_vec,
        aggregate,
        RadonScriptExecutionSettings::disable_all(),
    )
    .map(RadonReport::into_inner)
}

/// Run tally stage of a data request, return `RadonReport`.
pub fn run_tally_report(
    radon_types_vec: Vec<RadonTypes>,
    consensus: &RADTally,
    liars: Option<Vec<bool>>,
    errors: Option<Vec<bool>>,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    let context = &mut ReportContext::default();
    let mut metadata = TallyMetaData::default();
    if let Some(liars) = liars {
        metadata.liars = liars;
    } else {
        metadata.liars = vec![false; radon_types_vec.len()];
    }
    if let Some(errors) = errors {
        metadata.errors = errors;
    } else {
        metadata.errors = vec![false; radon_types_vec.len()];
    }
    context.stage = Stage::Tally(metadata);

    let filters = consensus.filters.as_slice();
    let reducer = consensus.reducer;
    let radon_script = create_radon_script_from_filters_and_reducer(filters, reducer)?;

    if radon_types_vec.is_empty() {
        return Ok(RadonReport::from_result(Err(RadError::NoReveals), context));
    }

    let items_to_tally = RadonTypes::from(RadonArray::from(radon_types_vec));

    execute_radon_script(items_to_tally, &radon_script, context, settings)
}

/// Run tally stage of a data request, return `RadonTypes`.
pub fn run_tally(radon_types_vec: Vec<RadonTypes>, consensus: &RADTally) -> Result<RadonTypes> {
    // Disable all execution tracing features, as this is the best-effort version of this method
    let settings = RadonScriptExecutionSettings {
        partial_results: false,
        timing: false,
        breakpoints: false,
    };
    run_tally_report(radon_types_vec, consensus, None, None, settings).map(RadonReport::into_inner)
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use serde_cbor::Value;

    use witnet_data_structures::{
        chain::RADFilter,
        radon_error::{RadonError, RadonErrors},
    };

    use crate::{
        filters::RadonFilters,
        operators::RadonOpCodes,
        reducers::RadonReducers,
        types::{float::RadonFloat, integer::RadonInteger},
    };

    use super::*;

    #[test]
    fn test_run_retrieval() {
        let script_r = Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text("main".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetFloat as i128),
                Value::Text("temp".to_string()),
            ]),
        ]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();

        let retrieve = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: packed_script_r,
        };
        let response = r#"{"coord":{"lon":13.41,"lat":52.52},"weather":[{"id":500,"main":"Rain","description":"light rain","icon":"10d"}],"base":"stations","main":{"temp":17.59,"pressure":1022,"humidity":67,"temp_min":15,"temp_max":20},"visibility":10000,"wind":{"speed":3.6,"deg":260},"rain":{"1h":0.51},"clouds":{"all":20},"dt":1567501321,"sys":{"type":1,"id":1275,"message":0.0089,"country":"DE","sunrise":1567484402,"sunset":1567533129},"timezone":7200,"id":2950159,"name":"Berlin","cod":200}"#;

        let result = run_retrieval_with_data(
            &retrieve,
            response.to_string(),
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();

        match result {
            RadonTypes::Float(_) => {}
            err => panic!("Error in run_retrieval: {:?}", err),
        }
    }

    #[test]
    fn test_run_consensus_and_aggregation() {
        let f_1 = RadonTypes::Float(RadonFloat::from(1f64));
        let f_3 = RadonTypes::Float(RadonFloat::from(3f64));

        let radon_types_vec = vec![f_1, f_3];

        let expected = RadonTypes::Float(RadonFloat::from(2f64));

        let output_aggregate = run_aggregation(
            radon_types_vec.clone(),
            &RADAggregate {
                filters: vec![],
                reducer: RadonReducers::AverageMean as u32,
            },
        )
        .unwrap();
        let output_tally = run_tally(
            radon_types_vec,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::AverageMean as u32,
            },
        )
        .unwrap();

        assert_eq!(output_aggregate, expected);
        assert_eq!(output_tally, expected);
    }

    #[test]
    fn test_run_all_risk_premium() {
        let script_r = Value::Array(vec![Value::Integer(RadonOpCodes::StringAsFloat as i128)]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();
        let retrieve = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://wrapapi.com/use/aesedepece/ffzz/prima/0.0.3?wrapAPIKey=ql4DVWylABdXCpt1NUTLNEDwPH57aHGm".to_string(),
            script: packed_script_r,
        };
        let response = "84";
        let expected = RadonTypes::Float(RadonFloat::from(84));

        let aggregate = RADAggregate {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        };

        let tally = RADTally {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        };

        let retrieved = run_retrieval_with_data(
            &retrieve,
            response.to_string(),
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();
        let aggregated = run_aggregation(vec![retrieved], &aggregate).unwrap();
        let tallied = run_tally(vec![aggregated], &tally).unwrap();

        assert_eq!(tallied, expected);
    }

    #[test]
    fn test_run_all_murders() {
        let script_r = Value::Array(vec![Value::Integer(RadonOpCodes::StringAsFloat as i128)]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();
        let retrieve = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://wrapapi.com/use/aesedepece/ffzz/murders/0.0.2?wrapAPIKey=ql4DVWylABdXCpt1NUTLNEDwPH57aHGm".to_string(),
            script: packed_script_r,
        };
        let response = "307";
        let expected = RadonTypes::Float(RadonFloat::from(307));

        let aggregate = RADAggregate {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        };
        let tally = RADTally {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        };

        let retrieved = run_retrieval_with_data(
            &retrieve,
            response.to_string(),
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();
        let aggregated = run_aggregation(vec![retrieved], &aggregate).unwrap();
        let tallied = run_tally(vec![aggregated], &tally).unwrap();

        assert_eq!(tallied, expected);
    }

    #[test]
    fn test_run_all_air_quality() {
        let script_r = Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONArray as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetMap as i128),
                Value::Integer(0 as i128),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text("hora0".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetFloat as i128),
                Value::Text("valor".to_string()),
            ]),
        ]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();

        let retrieve = RADRetrieve {
            kind: RADType::HttpGet,
            url: "http://airemadrid.herokuapp.com/api/estacion".to_string(),
            script: packed_script_r,
        };
        // This response was modified because the original was about 100KB.
        let response = r#"[{"estacion_nombre":"Pza. de España","estacion_numero":4,"fecha":"03092019","hora0":{"estado":"Pasado","valor":"00008"}}]"#;
        let expected = RadonTypes::Float(RadonFloat::from(8));

        let aggregate = RADAggregate {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        };
        let tally = RADTally {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        };

        let retrieved = run_retrieval_with_data(
            &retrieve,
            response.to_string(),
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();
        let aggregated = run_aggregation(vec![retrieved], &aggregate).unwrap();
        let tallied = run_tally(vec![aggregated], &tally).unwrap();

        assert_eq!(tallied, expected);
    }

    #[test]
    fn test_run_all_elections() {
        let script_r = Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetFloat as i128),
                Value::Text("PSOE".to_string()),
            ]),
        ]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();

        let retrieve = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://wrapapi.com/use/aesedepece/ffzz/generales/0.0.3?wrapAPIKey=ql4DVWylABdXCpt1NUTLNEDwPH57aHGm".to_string(),
            script: packed_script_r,
        };
        let response = r#"{"PSOE":123,"PP":66,"Cs":57,"UP":42,"VOX":24,"ERC-SOBIRANISTES":15,"JxCAT-JUNTS":7,"PNV":6,"EH Bildu":4,"CCa-PNC":2,"NA+":2,"COMPROMÍS 2019":1,"PRC":1,"PACMA":0,"FRONT REPUBLICÀ":0,"BNG":0,"RECORTES CERO-GV":0,"NCa":0,"PACT":0,"ARA-MES-ESQUERRA":0,"GBAI":0,"PUM+J":0,"EN MAREA":0,"PCTE":0,"EL PI":0,"AxSI":0,"PCOE":0,"PCPE":0,"AVANT ADELANTE LOS VERDES":0,"EB":0,"CpM":0,"SOMOS REGIÓN":0,"PCPA":0,"PH":0,"UIG-SOM-CUIDES":0,"ERPV":0,"IZQP":0,"PCPC":0,"AHORA CANARIAS":0,"CxG":0,"PPSO":0,"CNV":0,"PREPAL":0,"C.Ex-C.R.Ex-P.R.Ex":0,"PR+":0,"P-LIB":0,"CILU-LINARES":0,"ANDECHA ASTUR":0,"JF":0,"PYLN":0,"FIA":0,"FE de las JONS":0,"SOLIDARIA":0,"F8":0,"DPL":0,"UNIÓN REGIONALISTA":0,"centrados":0,"DP":0,"VOU":0,"PDSJE-UDEC":0,"IZAR":0,"RISA":0,"C 21":0,"+MAS+":0,"UDT":0}"#;
        let expected = RadonTypes::Float(RadonFloat::from(123));

        let aggregate = RADAggregate {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        };
        let tally = RADTally {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        };

        let retrieved = run_retrieval_with_data(
            &retrieve,
            response.to_string(),
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();
        let aggregated = run_aggregation(vec![retrieved], &aggregate).unwrap();
        let tallied = run_tally(vec![aggregated], &tally).unwrap();

        assert_eq!(tallied, expected);
    }

    #[test]
    fn test_run_football() {
        use crate::types::integer::RadonInteger;

        let script_r = Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text("event".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetMap as i128),
                Value::Text("awayScore".to_string()),
            ]),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetFloat as i128),
                Value::Text("current".to_string()),
            ]),
            Value::Integer(RadonOpCodes::FloatRound as i128),
        ]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();

        let retrieve = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://www.sofascore.com/event/8397714/json".to_string(),
            script: packed_script_r,
        };
        let response = r#"{"event":{"homeTeam":{"name":"Ryazan-VDV","slug":"ryazan-vdv","gender":"F","national":false,"id":171120,"shortName":"Ryazan-VDV","subTeams":[]},"awayTeam":{"name":"Olympique Lyonnais","slug":"olympique-lyonnais","gender":"F","national":false,"id":26245,"shortName":"Lyon","subTeams":[]},"homeScore":{"current":0,"display":0,"period1":0,"normaltime":0},"awayScore":{"current":9,"display":9,"period1":5,"normaltime":9}}}"#;
        let retrieved = run_retrieval_with_data(
            &retrieve,
            response.to_string(),
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();
        let expected = RadonTypes::Integer(RadonInteger::from(9));
        assert_eq!(retrieved, expected)
    }

    #[test]
    fn test_filter_liars() {
        use crate::types::integer::RadonInteger;

        let reveals = vec![RadonTypes::Integer(RadonInteger::from(0))];

        let consensus = run_tally_report(
            reveals,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();

        let expected_result = RadonTypes::Integer(RadonInteger::from(0));
        let expected_liars = vec![false];
        assert_eq!(consensus.result, expected_result);
        let tally_metadata = if let Stage::Tally(tm) = consensus.metadata {
            tm
        } else {
            panic!("No tally stage");
        };
        assert_eq!(tally_metadata.liars, expected_liars);
    }

    #[test]
    fn test_filter_liars2() {
        use crate::types::integer::RadonInteger;

        let reveals = vec![
            RadonTypes::Integer(RadonInteger::from(0)),
            RadonTypes::Integer(RadonInteger::from(0)),
        ];

        let consensus = run_tally_report(
            reveals,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();

        let expected_result = RadonTypes::Integer(RadonInteger::from(0));
        let expected_liars = vec![false, false];
        assert_eq!(consensus.result, expected_result);
        let tally_metadata = if let Stage::Tally(tm) = consensus.metadata {
            tm
        } else {
            panic!("No tally stage");
        };
        assert_eq!(tally_metadata.liars, expected_liars);
    }

    #[test]
    fn test_filter_liars3() {
        use crate::types::integer::RadonInteger;

        let reveals = vec![
            RadonTypes::Integer(RadonInteger::from(0)),
            RadonTypes::Integer(RadonInteger::from(0)),
            RadonTypes::Integer(RadonInteger::from(0)),
        ];

        let consensus = run_tally_report(
            reveals,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();

        let expected_result = RadonTypes::Integer(RadonInteger::from(0));
        let expected_liars = vec![false, false, false];
        assert_eq!(consensus.result, expected_result);
        let tally_metadata = if let Stage::Tally(tm) = consensus.metadata {
            tm
        } else {
            panic!("No tally stage");
        };
        assert_eq!(tally_metadata.liars, expected_liars);
    }

    #[test]
    fn test_run_consensus_with_liar() {
        let f_1 = RadonTypes::Float(RadonFloat::from(1f64));
        let f_3 = RadonTypes::Float(RadonFloat::from(3f64));
        let f_out = RadonTypes::Float(RadonFloat::from(10000f64));

        let radon_types_vec = vec![f_1, f_3, f_out];

        let report = run_tally_report(
            radon_types_vec,
            &RADTally {
                filters: vec![RADFilter {
                    op: RadonFilters::DeviationStandard as u32,
                    args: vec![249, 60, 0],
                }],
                reducer: RadonReducers::AverageMean as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();

        let expected = RadonTypes::Float(RadonFloat::from(2f64));

        let output_tally = report.clone().into_inner();
        assert_eq!(output_tally, expected);

        let expected_liars = vec![false, false, true];
        let tally_metadata = if let Stage::Tally(tm) = report.metadata {
            tm
        } else {
            panic!("No tally stage");
        };
        assert_eq!(tally_metadata.liars, expected_liars);
    }

    #[test]
    fn test_run_consensus_with_liar2() {
        let f_1 = RadonTypes::Float(RadonFloat::from(1f64));
        let f_2 = RadonTypes::Float(RadonFloat::from(3f64));
        let f_3 = RadonTypes::Float(RadonFloat::from(3f64));
        let f_out = RadonTypes::Float(RadonFloat::from(10000f64));

        let radon_types_vec = vec![f_1, f_2, f_3, f_out];

        let expected = RadonTypes::Float(RadonFloat::from(3f64));

        let report = run_tally_report(
            radon_types_vec,
            &RADTally {
                filters: vec![
                    RADFilter {
                        op: RadonFilters::DeviationStandard as u32,
                        args: vec![249, 60, 0],
                    },
                    RADFilter {
                        op: RadonFilters::DeviationStandard as u32,
                        args: vec![249, 60, 0],
                    },
                ],
                reducer: RadonReducers::AverageMean as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();

        let output_tally = report.clone().into_inner();
        assert_eq!(output_tally, expected);

        let expected_liars = vec![true, false, false, true];
        let tally_metadata = if let Stage::Tally(tm) = report.metadata {
            tm
        } else {
            panic!("No tally stage");
        };
        assert_eq!(tally_metadata.liars, expected_liars);
    }

    #[test]
    fn test_mode_reducer_not_affecting_liars() {
        let f_1 = RadonTypes::Float(RadonFloat::from(1f64));
        let f_2 = RadonTypes::Float(RadonFloat::from(3f64));
        let f_3 = RadonTypes::Float(RadonFloat::from(3f64));
        let f_out = RadonTypes::Float(RadonFloat::from(10000f64));

        let radon_types_vec = vec![f_1, f_2, f_3, f_out];

        let expected = RadonTypes::Float(RadonFloat::from(3f64));

        let report = run_tally_report(
            radon_types_vec,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap();

        let output_tally = report.clone().into_inner();
        assert_eq!(output_tally, expected);

        let expected_liars = vec![false, false, false, false];
        let tally_metadata = if let Stage::Tally(tm) = report.metadata {
            tm
        } else {
            panic!("No tally stage");
        };
        assert_eq!(tally_metadata.liars, expected_liars);
    }

    #[test]
    fn test_error_sort_in_tally_stage() {
        let f_1 = RadonTypes::Integer(RadonInteger::from(1));
        let f_3 = RadonTypes::Integer(RadonInteger::from(3));
        let f_out = RadonTypes::Integer(RadonInteger::from(10000));

        let radon_types_vec = vec![f_1, f_3, f_out];

        let error = run_tally_report(
            radon_types_vec,
            &RADTally {
                filters: vec![
                    RADFilter {
                        op: RadonOpCodes::ArraySort as u32,
                        args: vec![],
                    },
                    RADFilter {
                        op: RadonFilters::DeviationStandard as u32,
                        args: vec![249, 60, 0],
                    },
                ],
                reducer: RadonReducers::AverageMean as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            RadError::UnknownFilter {
                code: RadonOpCodes::ArraySort as i128,
            }
        );
    }

    #[test]
    fn test_error_map_in_tally_stage() {
        let f_1 = RadonTypes::from(RadonArray::from(vec![
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(2f64)),
        ]));
        let f_2 = RadonTypes::from(RadonArray::from(vec![
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(2f64)),
        ]));
        let f_3 = RadonTypes::from(RadonArray::from(vec![
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(2f64)),
        ]));
        let radon_types_vec = vec![f_1, f_2, f_3];

        let error = run_tally_report(
            radon_types_vec,
            &RADTally {
                filters: vec![RADFilter {
                    op: RadonOpCodes::ArrayMap as u32,
                    args: vec![],
                }],
                reducer: RadonReducers::AverageMean as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            RadError::UnknownFilter {
                code: RadonOpCodes::ArrayMap as i128,
            }
        );
    }

    #[test]
    fn test_error_get_in_tally_stage() {
        let f_1 = RadonTypes::from(RadonArray::from(vec![
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(2f64)),
        ]));
        let f_2 = RadonTypes::from(RadonArray::from(vec![
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(2f64)),
        ]));
        let f_3 = RadonTypes::from(RadonArray::from(vec![
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(3f64)),
            RadonTypes::Float(RadonFloat::from(2f64)),
        ]));
        let radon_types_vec = vec![f_1, f_2, f_3];

        let error = run_tally_report(
            radon_types_vec,
            &RADTally {
                filters: vec![RADFilter {
                    op: RadonOpCodes::ArrayGetArray as u32,
                    args: vec![],
                }],
                reducer: RadonReducers::AverageMean as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            RadError::UnknownFilter {
                code: RadonOpCodes::ArrayGetArray as i128,
            }
        );
    }

    #[test]
    fn test_result_no_reveals() {
        // Trying to create a tally with no reveals will return a `RadonReport` with a
        // `RadonTypes::RadonError()`.
        let reveals = vec![];
        let report = run_tally_report(
            reveals,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::AverageMean as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
        )
        .unwrap()
        .into_inner();
        let expected = RadonTypes::from(RadonError::try_from(RadError::NoReveals).unwrap());

        assert_eq!(report, expected);
    }

    #[test]
    fn compare_zero_int_and_zero_error() {
        use std::convert::TryFrom;
        use witnet_data_structures::radon_report::TypeLike;

        // RadonInteger with value 0
        let int = RadonTypes::from(RadonInteger::from(0));
        // RadonError with error code 0
        let kind = RadonErrors::try_from(0).unwrap();
        let err = RadonTypes::from(
            RadonError::try_from(RadError::try_from_kind_and_cbor_args(kind, None).unwrap())
                .unwrap(),
        );
        // Ensure they encoded differently (errors are tagged using `39` as CBOR tag)
        assert_ne!(int.encode(), err.encode());
        // And they are not equal in runtime either
        assert_ne!(int, err);
    }
}
