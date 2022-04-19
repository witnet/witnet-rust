//! # RAD Engine

use futures::{executor::block_on, future::join_all};
use serde::Serialize;
pub use serde_cbor::to_vec as cbor_to_vec;
pub use serde_cbor::Value as CborValue;
use std::str::FromStr;
use surf::{
    http::headers::{HeaderName, HeaderValues, ToHeaderValues},
    RequestBuilder,
};

#[cfg(test)]
use witnet_data_structures::mainnet_validations::all_wips_active;
use witnet_data_structures::{
    chain::{RADAggregate, RADRequest, RADRetrieve, RADTally, RADType},
    mainnet_validations::{current_active_wips, ActiveWips},
    radon_report::{RadonReport, ReportContext, RetrievalMetadata, Stage, TallyMetaData},
};

use crate::{
    conditions::{evaluate_tally_precondition_clause, TallyPreconditionClauseResult},
    error::RadError,
    script::{
        create_radon_script_from_filters_and_reducer, execute_radon_script, unpack_radon_script,
        RadonScriptExecutionSettings,
    },
    types::{array::RadonArray, bytes::RadonBytes, string::RadonString, RadonTypes},
    user_agents::UserAgent,
};

pub mod conditions;
pub mod error;
pub mod filters;
pub mod hash_functions;
pub mod operators;
pub mod reducers;
pub mod script;
pub mod types;
pub mod user_agents;

pub type Result<T> = std::result::Result<T, RadError>;

/// The return type of any method executing the entire life cycle of a data request.
#[derive(Debug, Serialize)]
pub struct RADRequestExecutionReport {
    /// Report about aggregation of data sources.
    pub aggregate: RadonReport<RadonTypes>,
    /// Vector of reports about retrieval of data sources.
    pub retrieve: Vec<RadonReport<RadonTypes>>,
    /// Report about aggregation of reports (reveals, actually).
    pub tally: RadonReport<RadonTypes>,
}

/// Executes a data request locally.
/// The `inputs_injection` allows for disabling the actual retrieval of the data sources and
/// the provided strings will be fed to the retrieval scripts instead. It is therefore expected that
/// the length of `sources_injection` matches that of `request.retrieve`.
pub fn try_data_request(
    request: &RADRequest,
    settings: RadonScriptExecutionSettings,
    inputs_injection: Option<&[&str]>,
) -> RADRequestExecutionReport {
    #[cfg(not(test))]
    let active_wips = current_active_wips();
    #[cfg(test)]
    let active_wips = all_wips_active();
    let mut retrieval_context =
        ReportContext::from_stage(Stage::Retrieval(RetrievalMetadata::default()));
    let retrieve_responses = if let Some(inputs) = inputs_injection {
        assert_eq!(inputs.len(), request.retrieve.len(), "Tried to locally run a data request with a number of injected sources different than the number of retrieval paths ({} != {})", inputs.len(), request.retrieve.len());

        request
            .retrieve
            .iter()
            .zip(inputs.iter())
            .map(|(retrieve, input)| {
                run_retrieval_with_data_report(retrieve, input, &mut retrieval_context, settings)
            })
            .collect()
    } else {
        block_on(join_all(
            request
                .retrieve
                .iter()
                .map(|retrieve| run_retrieval_report(retrieve, settings, active_wips.clone()))
                .collect::<Vec<_>>(),
        ))
    };

    let retrieval_reports: Vec<RadonReport<RadonTypes>> = retrieve_responses
        .into_iter()
        .map(|retrieve| {
            retrieve
                .unwrap_or_else(|error| RadonReport::from_result(Err(error), &retrieval_context))
        })
        .collect();

    // Evaluate aggregation pre-condition by using the same logic than for tally pre-condition,
    // to ensure that at least 20% of the data sources are not errors.
    // Aggregation stage does not need to evaluate any post-condition.
    let clause_result = evaluate_tally_precondition_clause(
        retrieval_reports.clone(),
        0.2,
        1,
        &current_active_wips(),
    );

    let aggregation_report = match clause_result {
        Ok(TallyPreconditionClauseResult::MajorityOfValues {
            values,
            liars: _liars,
            errors: _errors,
        }) => {
            // Perform aggregation on the values that made it to the output vector after applying the
            // source scripts (aka _normalization scripts_ in the original whitepaper) and filtering out
            // failures.
            let (aggregation_result, aggregation_context) =
                run_aggregation_report(values, &request.aggregate, settings, active_wips.clone());

            aggregation_result
                .unwrap_or_else(|error| RadonReport::from_result(Err(error), &aggregation_context))
        }
        Ok(TallyPreconditionClauseResult::MajorityOfErrors { errors_mode }) => {
            RadonReport::from_result(
                Ok(RadonTypes::RadonError(errors_mode)),
                &ReportContext::default(),
            )
        }
        Err(e) => RadonReport::from_result(Err(e), &ReportContext::default()),
    };
    let aggregation_value = aggregation_report.result.clone();

    let (tally_result, tally_context) = run_tally_report(
        vec![aggregation_value],
        &request.tally,
        None,
        None,
        settings,
        active_wips,
    );
    let tally_report =
        tally_result.unwrap_or_else(|error| RadonReport::from_result(Err(error), &tally_context));

    RADRequestExecutionReport {
        retrieve: retrieval_reports,
        aggregate: aggregation_report,
        tally: tally_report,
    }
}

/// Handle HTTP-GET and HTTP-POST response with data, and return a `RadonReport`.
fn string_response_with_data_report(
    retrieve: &RADRetrieve,
    response: &str,
    context: &mut ReportContext<RadonTypes>,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    let input = RadonTypes::from(RadonString::from(response));
    let radon_script = unpack_radon_script(&retrieve.script)?;

    execute_radon_script(input, &radon_script, context, settings)
}

/// Handle Rng response with data report
fn rng_response_with_data_report(
    response: &str,
    context: &mut ReportContext<RadonTypes>,
) -> Result<RadonReport<RadonTypes>> {
    let response_bytes = response.as_bytes();
    let result = RadonTypes::from(RadonBytes::from(response_bytes.to_vec()));

    Ok(RadonReport::from_result(Ok(result), context))
}

/// Run retrieval without performing any external network requests, return `Result<RadonReport>`.
pub fn run_retrieval_with_data_report(
    retrieve: &RADRetrieve,
    response: &str,
    context: &mut ReportContext<RadonTypes>,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    match retrieve.kind {
        RADType::HttpGet => string_response_with_data_report(retrieve, response, context, settings),
        RADType::Rng => rng_response_with_data_report(response, context),
        RADType::HttpPost => {
            string_response_with_data_report(retrieve, response, context, settings)
        }
        _ => Err(RadError::UnknownRetrieval),
    }
}

/// Run retrieval without performing any external network requests, return `Result<RadonTypes>`.
pub fn run_retrieval_with_data(
    retrieve: &RADRetrieve,
    response: &str,
    settings: RadonScriptExecutionSettings,
    active_wips: ActiveWips,
) -> Result<RadonTypes> {
    let context = &mut ReportContext::from_stage(Stage::Retrieval(RetrievalMetadata::default()));
    context.set_active_wips(active_wips);
    run_retrieval_with_data_report(retrieve, response, context, settings)
        .map(RadonReport::into_inner)
}

/// Handle generic HTTP (GET/POST) response
async fn http_response(
    retrieve: &RADRetrieve,
    context: &mut ReportContext<RadonTypes>,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    // Validate URL because surf::get panics on invalid URL
    // It could still panic if surf gets updated and changes their URL parsing library
    // TODO: this could be removed if we use `surf::RequestBuilder::new(Method::Get, url)` instead of `surf::get(url)`
    let _valid_url = url::Url::parse(&retrieve.url).map_err(|err| RadError::UrlParseError {
        inner: err,
        url: retrieve.url.clone(),
    })?;

    let request = match retrieve.kind {
        RADType::HttpGet => surf::get(&retrieve.url),
        RADType::HttpPost => {
            // The call to `.body` sets the content type header to `application/octet-stream`
            surf::post(&retrieve.url).body(retrieve.body.clone())
        }
        _ => panic!(
            "Called http_response with invalid retrieval kind {:?}",
            retrieve.kind
        ),
    };
    let request = add_http_headers(request, retrieve, context)?;
    let mut response = request.await.map_err(|x| RadError::HttpOther {
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

    let result = run_retrieval_with_data_report(retrieve, &response_string, context, settings);

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

/// Handle Rng response
async fn rng_response(
    context: &mut ReportContext<RadonTypes>,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    // Set the execution start timestamp, if enabled by `timing` setting
    if settings.timing {
        context.start();
    }

    let random_bytes: [u8; 32] = rand::random();
    let random_bytes = RadonTypes::from(RadonBytes::from(random_bytes.to_vec()));

    // Set the completion timestamp, if enabled by `timing` settings
    if settings.timing {
        context.complete();
    }

    Ok(RadonReport::from_result(Ok(random_bytes), context))
}

/// Add HTTP headers from `retrieve.headers` to surf request.
///
/// Some notes:
///
/// * Overwriting the User-Agent header is allowed.
/// * Multiple headers with the same key but different value are not supported, the current
///   implementation will only use the last header.
/// * All the non-standard headers will be converted to lowercase. Standard headers will use their
///   standard capitalization, for example: `Content-Type`.
/// * The HTTP client may reorder the headers: the order can change between consecutive invocations
///   of the same request
fn add_http_headers(
    mut request: RequestBuilder,
    retrieve: &RADRetrieve,
    _context: &mut ReportContext<RadonTypes>,
) -> Result<RequestBuilder> {
    // Add random user agent
    request = request.header("User-Agent", UserAgent::random());

    // Add extra_headers from retrieve.headers
    for (name, value) in &retrieve.headers {
        // Additional validation because surf does not validate some cases such as:
        // * header name that contains `:`
        // * header value that contains `\n`
        let _name = http::header::HeaderName::from_str(name.as_str()).map_err(|e| {
            RadError::InvalidHttpHeader {
                name: name.to_string(),
                value: value.to_string(),
                error: e.to_string(),
            }
        })?;
        let _value = http::header::HeaderValue::from_str(value.as_str()).map_err(|e| {
            RadError::InvalidHttpHeader {
                name: name.to_string(),
                value: value.to_string(),
                error: e.to_string(),
            }
        })?;

        // Validate headers using surf to avoid panics
        let name: HeaderName =
            HeaderName::from_str(name).map_err(|e| RadError::InvalidHttpHeader {
                name: name.to_string(),
                value: value.to_string(),
                error: e.to_string(),
            })?;
        let values: HeaderValues = value
            .to_header_values()
            .map_err(|e| RadError::InvalidHttpHeader {
                name: name.to_string(),
                value: value.to_string(),
                error: e.to_string(),
            })?
            .collect();

        request = request.header(name, &values);
    }

    Ok(request)
}

/// Run retrieval stage of a data request, return `Result<RadonReport>`.
pub async fn run_retrieval_report(
    retrieve: &RADRetrieve,
    settings: RadonScriptExecutionSettings,
    active_wips: ActiveWips,
) -> Result<RadonReport<RadonTypes>> {
    let context = &mut ReportContext::from_stage(Stage::Retrieval(RetrievalMetadata::default()));
    context.set_active_wips(active_wips);

    match retrieve.kind {
        RADType::HttpGet => http_response(retrieve, context, settings).await,
        RADType::Rng => rng_response(context, settings).await,
        RADType::HttpPost => http_response(retrieve, context, settings).await,
        _ => Err(RadError::UnknownRetrieval),
    }
}

/// Run retrieval stage of a data request, return `Result<RadonTypes>`.
pub async fn run_retrieval(retrieve: &RADRetrieve, active_wips: ActiveWips) -> Result<RadonTypes> {
    // Disable all execution tracing features, as this is the best-effort version of this method
    run_retrieval_report(
        retrieve,
        RadonScriptExecutionSettings::disable_all(),
        active_wips,
    )
    .await
    .map(RadonReport::into_inner)
}

/// Run aggregate stage of a data request, return a tuple of `Result<RadonReport>` and `ReportContext`
pub fn run_aggregation_report(
    radon_types_vec: Vec<RadonTypes>,
    aggregate: &RADAggregate,
    settings: RadonScriptExecutionSettings,
    active_wips: ActiveWips,
) -> (Result<RadonReport<RadonTypes>>, ReportContext<RadonTypes>) {
    let mut context = ReportContext::from_stage(Stage::Aggregation);
    context.set_active_wips(active_wips);

    let aux =
        run_aggregation_with_context_report(radon_types_vec, aggregate, &mut context, settings);

    (aux, context)
}

/// Run aggregate stage of a data request on a custom context, return `Result<RadonReport>`.
pub fn run_aggregation_with_context_report(
    radon_types_vec: Vec<RadonTypes>,
    aggregate: &RADAggregate,
    context: &mut ReportContext<RadonTypes>,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    let filters = aggregate.filters.as_slice();
    let reducer = aggregate.reducer;

    let active_wips = if let Some(active_wips) = context.active_wips.as_ref() {
        active_wips.clone()
    } else {
        current_active_wips()
    };

    let radon_script =
        create_radon_script_from_filters_and_reducer(filters, reducer, &active_wips)?;

    let items_to_aggregate = RadonTypes::from(RadonArray::from(radon_types_vec));

    execute_radon_script(items_to_aggregate, &radon_script, context, settings)
}

/// Run aggregate stage of a data request, return `Result<RadonTypes>`.
pub fn run_aggregation(
    radon_types_vec: Vec<RadonTypes>,
    aggregate: &RADAggregate,
    active_wips: ActiveWips,
) -> Result<RadonTypes> {
    // Disable all execution tracing features, as this is the best-effort version of this method
    let (res, _) = run_aggregation_report(
        radon_types_vec,
        aggregate,
        RadonScriptExecutionSettings::disable_all(),
        active_wips,
    );

    res.map(RadonReport::into_inner)
}

/// Run tally stage of a data request, return a tuple of `Result<RadonReport>` and `ReportContext`
pub fn run_tally_report(
    radon_types_vec: Vec<RadonTypes>,
    consensus: &RADTally,
    liars: Option<Vec<bool>>,
    errors: Option<Vec<bool>>,
    settings: RadonScriptExecutionSettings,
    active_wips: ActiveWips,
) -> (Result<RadonReport<RadonTypes>>, ReportContext<RadonTypes>) {
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
    let mut context = ReportContext {
        stage: Stage::Tally(metadata),
        active_wips: Some(active_wips),
        ..Default::default()
    };

    let res = run_tally_with_context_report(radon_types_vec, consensus, &mut context, settings);

    (res, context)
}

/// Run tally stage of a data request on a custom context, return `Result<RadonReport>`.
pub fn run_tally_with_context_report(
    radon_types_vec: Vec<RadonTypes>,
    consensus: &RADTally,
    context: &mut ReportContext<RadonTypes>,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    let filters = consensus.filters.as_slice();
    let reducer = consensus.reducer;

    let active_wips = if let Some(active_wips) = context.active_wips.as_ref() {
        active_wips.clone()
    } else {
        current_active_wips()
    };

    let radon_script =
        create_radon_script_from_filters_and_reducer(filters, reducer, &active_wips)?;

    if radon_types_vec.is_empty() {
        return Ok(RadonReport::from_result(Err(RadError::NoReveals), context));
    }

    let items_to_tally = RadonTypes::from(RadonArray::from(radon_types_vec));

    execute_radon_script(items_to_tally, &radon_script, context, settings)
}

/// Run tally stage of a data request, return `Result<RadonTypes>`.
pub fn run_tally(
    radon_types_vec: Vec<RadonTypes>,
    consensus: &RADTally,
    active_wips: ActiveWips,
) -> Result<RadonTypes> {
    // Disable all execution tracing features, as this is the best-effort version of this method
    let settings = RadonScriptExecutionSettings::disable_all();
    let (res, _) = run_tally_report(
        radon_types_vec,
        consensus,
        None,
        None,
        settings,
        active_wips,
    );

    res.map(RadonReport::into_inner)
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
        types::{float::RadonFloat, integer::RadonInteger, RadonType},
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
            body: vec![],
            headers: vec![],
        };
        let response = r#"{"coord":{"lon":13.41,"lat":52.52},"weather":[{"id":500,"main":"Rain","description":"light rain","icon":"10d"}],"base":"stations","main":{"temp":17.59,"pressure":1022,"humidity":67,"temp_min":15,"temp_max":20},"visibility":10000,"wind":{"speed":3.6,"deg":260},"rain":{"1h":0.51},"clouds":{"all":20},"dt":1567501321,"sys":{"type":1,"id":1275,"message":0.0089,"country":"DE","sunrise":1567484402,"sunset":1567533129},"timezone":7200,"id":2950159,"name":"Berlin","cod":200}"#;

        let result = run_retrieval_with_data(
            &retrieve,
            response,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
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
            current_active_wips(),
        )
        .unwrap();
        let output_tally = run_tally(
            radon_types_vec,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::AverageMean as u32,
            },
            current_active_wips(),
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
            body: vec![],
            headers: vec![],
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
            response,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        )
        .unwrap();
        let aggregated =
            run_aggregation(vec![retrieved], &aggregate, current_active_wips()).unwrap();
        let tallied = run_tally(vec![aggregated], &tally, current_active_wips()).unwrap();

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
            body: vec![],
            headers: vec![],
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
            response,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        )
        .unwrap();
        let aggregated =
            run_aggregation(vec![retrieved], &aggregate, current_active_wips()).unwrap();
        let tallied = run_tally(vec![aggregated], &tally, current_active_wips()).unwrap();

        assert_eq!(tallied, expected);
    }

    #[test]
    fn test_run_all_air_quality() {
        let script_r = Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONArray as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::ArrayGetMap as i128),
                Value::Integer(0_i128),
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
            body: vec![],
            headers: vec![],
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
            response,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        )
        .unwrap();
        let aggregated =
            run_aggregation(vec![retrieved], &aggregate, current_active_wips()).unwrap();
        let tallied = run_tally(vec![aggregated], &tally, current_active_wips()).unwrap();

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
            body: vec![],
            headers: vec![],
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
            response,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        )
        .unwrap();
        let aggregated =
            run_aggregation(vec![retrieved], &aggregate, current_active_wips()).unwrap();
        let tallied = run_tally(vec![aggregated], &tally, current_active_wips()).unwrap();

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
            body: vec![],
            headers: vec![],
        };
        let response = r#"{"event":{"homeTeam":{"name":"Ryazan-VDV","slug":"ryazan-vdv","gender":"F","national":false,"id":171120,"shortName":"Ryazan-VDV","subTeams":[]},"awayTeam":{"name":"Olympique Lyonnais","slug":"olympique-lyonnais","gender":"F","national":false,"id":26245,"shortName":"Lyon","subTeams":[]},"homeScore":{"current":0,"display":0,"period1":0,"normaltime":0},"awayScore":{"current":9,"display":9,"period1":5,"normaltime":9}}}"#;
        let retrieved = run_retrieval_with_data(
            &retrieve,
            response,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        )
        .unwrap();
        let expected = RadonTypes::Integer(RadonInteger::from(9));
        assert_eq!(retrieved, expected)
    }

    #[test]
    fn test_filter_liars() {
        use crate::types::integer::RadonInteger;

        let reveals = vec![RadonTypes::Integer(RadonInteger::from(0))];

        let (res, _) = run_tally_report(
            reveals,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        );
        let consensus = res.unwrap();

        let expected_result = RadonTypes::Integer(RadonInteger::from(0));
        let expected_liars = vec![false];
        assert_eq!(consensus.result, expected_result);
        let tally_metadata = if let Stage::Tally(tm) = consensus.context.stage {
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

        let (res, _) = run_tally_report(
            reveals,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        );
        let consensus = res.unwrap();

        let expected_result = RadonTypes::Integer(RadonInteger::from(0));
        let expected_liars = vec![false, false];
        assert_eq!(consensus.result, expected_result);
        let tally_metadata = if let Stage::Tally(tm) = consensus.context.stage {
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

        let (res, _) = run_tally_report(
            reveals,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        );
        let consensus = res.unwrap();

        let expected_result = RadonTypes::Integer(RadonInteger::from(0));
        let expected_liars = vec![false, false, false];
        assert_eq!(consensus.result, expected_result);
        let tally_metadata = if let Stage::Tally(tm) = consensus.context.stage {
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

        let (res, _) = run_tally_report(
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
            current_active_wips(),
        );
        let report = res.unwrap();

        let expected = RadonTypes::Float(RadonFloat::from(2f64));

        let output_tally = report.clone().into_inner();
        assert_eq!(output_tally, expected);

        let expected_liars = vec![false, false, true];
        let tally_metadata = if let Stage::Tally(tm) = report.context.stage {
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

        let (res, _) = run_tally_report(
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
            current_active_wips(),
        );
        let report = res.unwrap();

        let output_tally = report.clone().into_inner();
        assert_eq!(output_tally, expected);

        let expected_liars = vec![true, false, false, true];
        let tally_metadata = if let Stage::Tally(tm) = report.context.stage {
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

        let (res, _) = run_tally_report(
            radon_types_vec,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        );
        let report = res.unwrap();

        let output_tally = report.clone().into_inner();
        assert_eq!(output_tally, expected);

        let expected_liars = vec![false, false, false, false];
        let tally_metadata = if let Stage::Tally(tm) = report.context.stage {
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

        let (res, _) = run_tally_report(
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
            current_active_wips(),
        );
        let error = res.unwrap_err();

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

        let (res, _) = run_tally_report(
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
            current_active_wips(),
        );
        let error = res.unwrap_err();

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

        let (res, _) = run_tally_report(
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
            current_active_wips(),
        );
        let error = res.unwrap_err();

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
        let (res, _) = run_tally_report(
            reveals,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::AverageMean as u32,
            },
            None,
            None,
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        );
        let report = res.unwrap().into_inner();
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
        let err = RadonTypes::from(RadError::try_from_kind_and_cbor_args(kind, None).unwrap());
        // Ensure they encoded differently (errors are tagged using `39` as CBOR tag)
        assert_ne!(int.encode(), err.encode());
        // And they are not equal in runtime either
        assert_ne!(int, err);
    }

    #[test]
    fn test_header_correctly_set() {
        let test_header = UserAgent::random();
        let req = surf::get("https://httpbin.org/get?page=2")
            .header("User-Agent", test_header)
            .build();
        assert_eq!(
            req.header("User-Agent")
                .map(|x| x.iter().map(|x| x.as_str()).collect()),
            Some(vec![test_header]),
        );
    }

    #[test]
    fn test_user_agent_can_be_overwritten() {
        let dummy_user_agent = "witnet http client".to_string();

        let retrieve = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://httpbin.org/get?page=2".to_string(),
            script: vec![128],
            body: vec![],
            headers: vec![("User-Agent".to_string(), dummy_user_agent.clone())],
        };

        let mut context = ReportContext {
            active_wips: Some(all_wips_active()),
            ..ReportContext::default()
        };

        let req = surf::get(&retrieve.url);
        let req = add_http_headers(req, &retrieve, &mut context).unwrap();
        let req = req.build();
        assert_eq!(
            req.header("User-Agent")
                .map(|x| x.iter().map(|x| x.as_str()).collect()),
            Some(vec![dummy_user_agent.as_str()]),
        );
    }

    #[test]
    fn test_repeated_header_uses_last_value() {
        let retrieve = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://httpbin.org/get?page=2".to_string(),
            script: vec![128],
            body: vec![],
            headers: vec![
                ("Test-Header".to_string(), "Value1".to_string()),
                ("Test-Header".to_string(), "Value2".to_string()),
            ],
        };

        let mut context = ReportContext {
            active_wips: Some(all_wips_active()),
            ..ReportContext::default()
        };

        let req = surf::get(&retrieve.url);
        let req = add_http_headers(req, &retrieve, &mut context).unwrap();
        let req = req.build();
        assert_eq!(
            req.header("Test-Header")
                .map(|x| x.iter().map(|x| x.as_str()).collect()),
            Some(vec!["Value2"]),
        );
    }

    /// Test try_data_request with a RNG source
    #[test]
    fn test_try_data_request_rng() {
        let request = RADRequest {
            time_lock: 0,
            retrieve: vec![RADRetrieve {
                kind: RADType::Rng,
                url: String::from(""),
                script: vec![128],
                body: vec![],
                headers: vec![],
            }],
            aggregate: RADAggregate {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            tally: RADTally {
                filters: vec![],
                reducer: RadonReducers::HashConcatenate as u32,
            },
        };
        let report = try_data_request(&request, RadonScriptExecutionSettings::enable_all(), None);
        let tally_result = report.tally.into_inner();

        if let RadonTypes::Bytes(bytes) = tally_result {
            assert_eq!(bytes.value().len(), 32);
        } else {
            panic!("No RadonBytes result in a RNG request");
        }
    }

    #[test]
    fn test_try_data_request_http_post_non_ascii_header_key() {
        let script_r = Value::Array(vec![]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();
        let body = Vec::from(String::from(""));
        let headers = vec![("ñ", "value")];
        let headers = headers
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let request = RADRequest {
            time_lock: 0,
            retrieve: vec![RADRetrieve {
                kind: RADType::HttpPost,
                url: String::from("http://127.0.0.1"),
                script: packed_script_r,
                body,
                headers,
            }],
            aggregate: RADAggregate {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            tally: RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
        };
        let report = try_data_request(&request, RadonScriptExecutionSettings::enable_all(), None);
        let tally_result = report.tally.into_inner();

        assert_eq!(
            tally_result,
            RadonTypes::RadonError(
                RadonError::try_from(RadError::UnhandledIntercept {
                    inner: Some(Box::new(RadError::InvalidHttpHeader {
                        name: "ñ".to_string(),
                        value: "value".to_string(),
                        error: "invalid HTTP header name".to_string()
                    })),
                    message: None
                })
                .unwrap()
            )
        );
    }

    #[test]
    fn test_try_data_request_http_post_non_ascii_header_value() {
        let script_r = Value::Array(vec![]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();
        let body = Vec::from(String::from(""));
        let headers = vec![("key", "ñ")];
        let headers = headers
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let request = RADRequest {
            time_lock: 0,
            retrieve: vec![RADRetrieve {
                kind: RADType::HttpPost,
                url: String::from("http://127.0.0.1"),
                script: packed_script_r,
                body,
                headers,
            }],
            aggregate: RADAggregate {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            tally: RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
        };
        let report = try_data_request(&request, RadonScriptExecutionSettings::enable_all(), None);
        let tally_result = report.tally.into_inner();

        assert_eq!(
            tally_result,
            RadonTypes::RadonError(
                RadonError::try_from(RadError::UnhandledIntercept {
                    inner: Some(Box::new(RadError::InvalidHttpHeader {
                        name: "key".to_string(),
                        value: "ñ".to_string(),
                        error: "String slice should be valid ASCII".to_string()
                    })),
                    message: None
                })
                .unwrap()
            )
        );
    }

    #[test]
    fn test_try_data_request_http_post_header_colon() {
        let script_r = Value::Array(vec![]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();
        let body = Vec::from(String::from(""));
        let headers = vec![("malformed:header", "value")];
        let headers = headers
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let request = RADRequest {
            time_lock: 0,
            retrieve: vec![RADRetrieve {
                kind: RADType::HttpPost,
                url: String::from("http://127.0.0.1"),
                script: packed_script_r,
                body,
                headers,
            }],
            aggregate: RADAggregate {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            tally: RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
        };
        let report = try_data_request(&request, RadonScriptExecutionSettings::enable_all(), None);
        let tally_result = report.tally.into_inner();

        assert_eq!(
            tally_result,
            RadonTypes::RadonError(
                RadonError::try_from(RadError::UnhandledIntercept {
                    inner: Some(Box::new(RadError::InvalidHttpHeader {
                        name: "malformed:header".to_string(),
                        value: "value".to_string(),
                        error: "invalid HTTP header name".to_string()
                    })),
                    message: None
                })
                .unwrap()
            )
        );
    }

    #[test]
    fn test_try_data_request_http_post_header_value_newline() {
        let script_r = Value::Array(vec![]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();
        let body = Vec::from(String::from(""));
        let headers = vec![("malformed-header", "value\nvalue2")];
        let headers = headers
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let request = RADRequest {
            time_lock: 0,
            retrieve: vec![RADRetrieve {
                kind: RADType::HttpPost,
                url: String::from("http://127.0.0.1"),
                script: packed_script_r,
                body,
                headers,
            }],
            aggregate: RADAggregate {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            tally: RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
        };
        let report = try_data_request(&request, RadonScriptExecutionSettings::enable_all(), None);
        let tally_result = report.tally.into_inner();

        assert_eq!(
            tally_result,
            RadonTypes::RadonError(
                RadonError::try_from(RadError::UnhandledIntercept {
                    inner: Some(Box::new(RadError::InvalidHttpHeader {
                        name: "malformed-header".to_string(),
                        value: "value\nvalue2".to_string(),
                        error: "failed to parse header value".to_string()
                    })),
                    message: None
                })
                .unwrap()
            )
        );
    }

    /// Ensure that `try_data_request` filters errors before calling `run_aggregation`.
    #[test]
    fn test_try_data_request_filters_aggregation_errors() {
        let script = cbor_to_vec(&Value::Array(vec![Value::Integer(
            RadonOpCodes::StringAsInteger as i128,
        )]))
        .unwrap();
        let request = RADRequest {
            time_lock: 0,
            retrieve: vec![
                RADRetrieve {
                    kind: RADType::HttpGet,
                    url: String::from(""),
                    script: script.clone(),
                    body: vec![],
                    headers: vec![],
                },
                RADRetrieve {
                    kind: RADType::HttpGet,
                    url: String::from(""),
                    script: script.clone(),
                    body: vec![],
                    headers: vec![],
                },
                RADRetrieve {
                    kind: RADType::HttpGet,
                    url: String::from(""),
                    script,
                    body: vec![],
                    headers: vec![],
                },
            ],
            aggregate: RADAggregate {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
            tally: RADTally {
                filters: vec![],
                reducer: RadonReducers::Mode as u32,
            },
        };
        let report = try_data_request(
            &request,
            RadonScriptExecutionSettings::enable_all(),
            Some(&["1", "1", "error"]),
        );
        let tally_result = report.tally.into_inner();

        assert_eq!(tally_result, RadonTypes::Integer(RadonInteger::from(1)));
    }
}
