//! # RAD Engine

extern crate witnet_data_structures;

use futures::{executor::block_on, future::join_all, AsyncReadExt};
use serde::Serialize;
pub use serde_cbor::{to_vec as cbor_to_vec, Value as CborValue};
#[cfg(test)]
use witnet_data_structures::chain::tapi::all_wips_active;
use witnet_data_structures::{
    chain::{
        tapi::{current_active_wips, ActiveWips},
        RADAggregate, RADRequest, RADRetrieve, RADTally, RADType,
    },
    radon_report::{RadonReport, ReportContext, RetrievalMetadata, Stage, TallyMetaData},
    witnessing::WitnessingConfig,
};
use witnet_net::client::http::WitnetHttpClient;
pub use witnet_net::Uri;

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
use core::convert::From;
use witnet_net::client::http::{WitnetHttpBody, WitnetHttpRequest};

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
    witnessing: Option<WitnessingConfig<witnet_net::Uri>>,
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
                run_retrieval_with_data_report(
                    retrieve,
                    RadonTypes::from(RadonString::from(*input)),
                    &mut retrieval_context,
                    settings,
                )
            })
            .collect()
    } else {
        block_on(join_all(
            request
                .retrieve
                .iter()
                .map(|retrieve| {
                    run_paranoid_retrieval(
                        retrieve,
                        request.aggregate.clone(),
                        settings,
                        active_wips.clone(),
                        witnessing.clone().unwrap_or_default(),
                    )
                })
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
                run_aggregation_report(values, request.aggregate.clone(), settings, &active_wips);

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
        &active_wips,
    );
    let tally_report =
        tally_result.unwrap_or_else(|error| RadonReport::from_result(Err(error), &tally_context));

    RADRequestExecutionReport {
        retrieve: retrieval_reports,
        aggregate: aggregation_report,
        tally: tally_report,
    }
}

/// Execute Radon Script using as input the RadonTypes value deserialized from a retrieval response
fn handle_response_with_data_report(
    retrieve: &RADRetrieve,
    response: RadonTypes,
    context: &mut ReportContext<RadonTypes>,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    let radon_script = unpack_radon_script(&retrieve.script)?;
    execute_radon_script(response, &radon_script, context, settings)
}

/// Run retrieval without performing any external network requests, return `Result<RadonReport>`.
pub fn run_retrieval_with_data_report(
    retrieve: &RADRetrieve,
    response: RadonTypes,
    context: &mut ReportContext<RadonTypes>,
    settings: RadonScriptExecutionSettings,
) -> Result<RadonReport<RadonTypes>> {
    match retrieve.kind {
        RADType::HttpGet | RADType::HttpPost | RADType::HttpHead | RADType::Rng => {
            handle_response_with_data_report(retrieve, response, context, settings)
        }
        _ => Err(RadError::UnknownRetrieval),
    }
}

/// Run retrieval without performing any external network requests, return `Result<RadonTypes>`.
pub fn run_retrieval_with_data(
    retrieve: &RADRetrieve,
    response: RadonTypes,
    settings: RadonScriptExecutionSettings,
    active_wips: ActiveWips,
) -> Result<RadonTypes> {
    let context = &mut ReportContext::from_stage(Stage::Retrieval(RetrievalMetadata::default()));
    context.set_active_wips(active_wips);
    run_retrieval_with_data_report(retrieve, response, context, settings)
        .map(RadonReport::into_inner)
}

/// Handle generic HTTP (GET/POST/HEAD) response
async fn http_response(
    retrieve: &RADRetrieve,
    context: &mut ReportContext<RadonTypes>,
    settings: RadonScriptExecutionSettings,
    client: Option<WitnetHttpClient>,
) -> Result<RadonReport<RadonTypes>> {
    // Validate URL to make sure that we handle malformed URLs nicely before they hit any library
    if let Err(err) = url::Url::parse(&retrieve.url) {
        Err(RadError::UrlParseError {
            inner: err,
            url: retrieve.url.clone(),
        })?
    };

    // Use the provided HTTP client, or instantiate a new one if none
    let client = match client {
        Some(client) => client,
        None => {
            let follow_redirects = context
                .active_wips
                .as_ref()
                .map(|active_wips| active_wips.wip0025())
                .unwrap_or(true);

            WitnetHttpClient::new(None, follow_redirects).map_err(|err| RadError::HttpOther {
                message: err.to_string(),
            })?
        }
    };

    let request = WitnetHttpRequest::build(|builder| {
        // Populate the builder and generate the body for different types of retrievals
        let (builder, body) = match retrieve.kind {
            RADType::HttpGet => (
                builder.method("GET").uri(&retrieve.url),
                WitnetHttpBody::empty(),
            ),
            RADType::HttpPost => {
                // Using `Vec<u8>` as the body sets the content type header to `application/octet-stream`
                (
                    builder.method("POST").uri(&retrieve.url),
                    WitnetHttpBody::from(retrieve.body.clone()),
                )
            }
            RADType::HttpHead => (
                builder.method("HEAD").uri(&retrieve.url),
                WitnetHttpBody::empty(),
            ),
            _ => panic!(
                "Called http_response with invalid retrieval kind {:?}",
                retrieve.kind
            ),
        };

        // Add random user agent
        let mut builder = builder.header("User-Agent", UserAgent::random());

        // Add extra_headers from retrieve.headers
        for (name, value) in &retrieve.headers {
            // Handle invalid header names and values with a specific and friendly error message
            validate_header(name, value)?;

            builder = builder.header(name, value);
        }

        // Finally attach the body to complete building the HTTP request
        builder.body(body).map_err(|e| RadError::HttpOther {
            message: e.to_string(),
        })
    })?;

    let response = client
        .send(request)
        .await
        .map_err(|x| RadError::HttpOther {
            message: x.to_string(),
        })?
        .inner();

    if !response.status().is_success() {
        return Err(RadError::HttpStatus {
            status_code: response.status().into(),
        });
    }

    let (parts, mut body) = response.into_parts();

    let response: RadonTypes;
    if retrieve.kind != RADType::HttpHead && parts.headers.contains_key("accept-ranges") {
        // http response is a binary stream
        let mut response_bytes = Vec::<u8>::default();

        // todo: before reading the response buffer, an error should be thrown if it was too big
        body.read_to_end(&mut response_bytes).await.map_err(|x| {
            RadError::HttpOther {
                message: x.to_string(),
            }
        })?;
        response = RadonTypes::from(RadonBytes::from(response_bytes));
    } else {
        // response is a string
        let mut response_string = String::default();
        match retrieve.kind {
            RADType::HttpHead => {
                response_string = format!("{:?}", parts.headers);
            }
            _ => {
                body.read_to_string(&mut response_string)
                    .await
                    .map_err(|x| RadError::HttpOther {
                        message: x.to_string(),
                    })?;
            }
        }
        response = RadonTypes::from(RadonString::from(response_string));
    }

    let result = handle_response_with_data_report(retrieve, response, context, settings);
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

/// Run retrieval stage of a data request, return `Result<RadonReport>`.
pub async fn run_retrieval_report(
    retrieve: &RADRetrieve,
    settings: RadonScriptExecutionSettings,
    active_wips: ActiveWips,
    client: Option<WitnetHttpClient>,
) -> Result<RadonReport<RadonTypes>> {
    let context = &mut ReportContext::from_stage(Stage::Retrieval(RetrievalMetadata::default()));
    context.set_active_wips(active_wips);

    match retrieve.kind {
        RADType::HttpGet | RADType::HttpHead | RADType::HttpPost => {
            http_response(retrieve, context, settings, client).await
        }
        RADType::Rng => rng_response(context, settings).await,
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
        None,
    )
    .await
    .map(RadonReport::into_inner)
}

/// Run retrieval using multiple transports, and only produce a positive result if the retrieved
/// values pass the filter function from the tally stage.
///
/// The idea behind this is to avoid producing commitments for data requests with sources that act
/// in an inconsistent way, i.e. they return very different values when queried through different
/// HTTP transports at once.
pub async fn run_paranoid_retrieval(
    retrieve: &RADRetrieve,
    aggregate: RADAggregate,
    settings: RadonScriptExecutionSettings,
    active_wips: ActiveWips,
    witnessing: WitnessingConfig<witnet_net::Uri>,
) -> Result<RadonReport<RadonTypes>> {
    // We can skip paranoid checks for retrieval types that don't use networking (e.g. RNG)
    if !retrieve.kind.is_http() {
        return run_retrieval_report(retrieve, settings, active_wips, None).await;
    }

    let futures: Result<Vec<_>> = witnessing
        .transports_as::<witnet_net::Uri>()
        .map_err(|(_, err)| RadError::HttpOther {
            message: err.to_string(),
        })?
        .into_iter()
        .map(|transport| {
            let follow_redirects = active_wips.wip0025();

            WitnetHttpClient::new(transport, follow_redirects)
                .map_err(|err| RadError::HttpOther {
                    message: err.to_string(),
                })
                .map(|client| {
                    run_retrieval_report(retrieve, settings, active_wips.clone(), Some(client))
                })
        })
        .collect();

    let values = join_all(futures?).await;

    evaluate_paranoid_retrieval(values, aggregate, settings, witnessing.paranoid_threshold)
}

/// Evaluate whether the values obtained when retrieving a data source through multiple transports
/// are consistent, i.e. enough of them pass the filters from the aggregation stage.
///
/// There are 4 cases in which this function will fail with `InconsistentSource`:
///
/// 1. All the transports failed or no transports are configured at all (in theory, this condition
/// should be unreachable).
/// 2. The retrieval failed on some of the used transports.
/// 3. The values that we got from different transports cannot be aggregated together.
/// 4. The result of applying the aggregation on the data coming from the different transports
/// reached a level of consensus that is lower than the configured paranoid threshold.
fn evaluate_paranoid_retrieval(
    data: Vec<Result<RadonReport<RadonTypes>>>,
    aggregate: RADAggregate,
    settings: RadonScriptExecutionSettings,
    paranoid: f32,
) -> Result<RadonReport<RadonTypes>> {
    // If there was only one retrieved value, there's no actual need to run the tally, as this means
    // that only one transport was used and therefore the node is not in paranoid mode.
    // We can simply return the first report as is.
    if data.len() < 2 {
        return data
            .into_iter()
            .next()
            // Case 1
            .ok_or(RadError::InconsistentSource)
            .and_then(|r| r);
    }

    // Case 2
    let reports = data
        .into_iter()
        .collect::<Result<Vec<_>>>()
        .or(Err(RadError::InconsistentSource))?;
    let values = reports
        .iter()
        .cloned()
        .map(RadonReport::into_inner)
        .collect();

    // This block is using a Tally context because Aggregate contexts currently do not keep
    // track of outliers.
    // Additionally, the `RADAggregate` struct is converted into `RADTally` for the same reason.
    // In the future, if we think that is an interesting feature (e.g. for
    // debugging data sources through `witnet_toolkit`), we can refactor `AggregateMetaData` and
    // avoid these tricks here.
    let mut context = ReportContext::from_stage(Stage::Tally(TallyMetaData::default()));
    let consensus = RADTally::from(aggregate);
    let tally = run_tally_with_context_report(values, &consensus, &mut context, settings)
        // Case 3
        .or(Err(RadError::InconsistentSource))?;

    // If the consensus of the data points is below the paranoid threshold of the node, we need
    // to resolve to the `InconsistentSource` error.
    if let Stage::Tally(TallyMetaData { consensus, .. }) = context.stage {
        if consensus < paranoid {
            // Case 4
            return Err(RadError::InconsistentSource);
        }
    }

    // If all the values pass the filters, return one of the reports, but swap the result for
    // that of the tally, so the potentially committed value is already averaged across the
    // multiple transports.
    // Case 1 as well
    let mut report = reports
        .into_iter()
        .next()
        .ok_or(RadError::InconsistentSource)?;
    report.result = tally.result;

    Ok(report)
}

/// Run aggregate stage of a data request, return a tuple of `Result<RadonReport>` and `ReportContext`
pub fn run_aggregation_report(
    radon_types_vec: Vec<RadonTypes>,
    aggregate: RADAggregate,
    settings: RadonScriptExecutionSettings,
    active_wips: &ActiveWips,
) -> (Result<RadonReport<RadonTypes>>, ReportContext<RadonTypes>) {
    let mut context = ReportContext::from_stage(Stage::Aggregation);
    context.set_active_wips(active_wips.clone());

    let aux =
        run_aggregation_with_context_report(radon_types_vec, aggregate, &mut context, settings);

    (aux, context)
}

/// Run aggregate stage of a data request on a custom context, return `Result<RadonReport>`.
pub fn run_aggregation_with_context_report(
    radon_types_vec: Vec<RadonTypes>,
    aggregate: RADAggregate,
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
    aggregate: RADAggregate,
    active_wips: &ActiveWips,
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
    active_wips: &ActiveWips,
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
        active_wips: Some(active_wips.clone()),
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
    active_wips: &ActiveWips,
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

/// Centralizes validation of header names and values.
///
/// ASCII checks are always run before `try_from` to prevent panics in the `http` library.
fn validate_header(name: &str, value: &str) -> Result<()> {
    let mut error_message = None;
    if name.is_ascii() {
        if let Err(err) = http::HeaderName::try_from(name) {
            error_message = Some(err.to_string())
        } else if value.is_ascii() {
            if let Err(err) = http::HeaderValue::try_from(value) {
                error_message = Some(err.to_string())
            }
        } else {
            error_message = Some("invalid HTTP header value".to_string())
        }
    } else {
        error_message = Some("invalid HTTP header name".to_string())
    };

    if let Some(error_message) = error_message {
        Err(RadError::InvalidHttpHeader {
            name: name.to_string(),
            value: value.to_string(),
            error: error_message,
        })
    } else {
        Ok(())
    }
}

/// Provides the `FromFrom` trait and implementations.
pub mod fromx {
    /// A `From<T>`-like trait that enables easy type routing, i.e. `A` → `B` →`Self`, `A` → `B` →
    /// `C` → `Self` and `A` → `B` → `C` → `D` → `Self`.
    ///
    /// As an example, this is how you build an instance of `RadonTypes` from a `&'static str`:
    /// ```rust
    /// use witnet_rad::{fromx::*, types::{RadonTypes, string::RadonString}};
    ///
    /// let a = RadonTypes::from3::<RadonString, String>("Hello, World!");
    /// let b: RadonTypes = "Hello, World!".into3::<String, RadonString>();
    /// let c = RadonTypes::from(RadonString::from(String::from("Hello, World!")));
    /// assert_eq!(a, b);
    /// assert_eq!(b, c);
    /// ```
    pub trait FromX<A> {
        fn from2<B>(_: A) -> Self
        where
            Self: From<B>,
            B: From<A>;

        fn from3<C, B>(_: A) -> Self
        where
            Self: From<C>,
            C: From<B>,
            B: From<A>;

        fn from4<D, C, B>(_: A) -> Self
        where
            Self: From<D>,
            D: From<C>,
            C: From<B>,
            B: From<A>;
    }

    impl<A, T> FromX<A> for T {
        /// Blanket implementation of `FromX<A>::from2<B> -> A` for for all `T` provided that the
        /// following requirements are met:
        /// - `T` implements `From<B>`
        /// - `B` implements `From<A>`
        fn from2<B>(item: A) -> Self
        where
            Self: From<B>,
            B: From<A>,
        {
            Self::from(B::from(item))
        }

        /// Blanket implementation of `FromX<A>::from3<C, B> -> A` for for all `T` provided that the
        /// following requirements are met:
        /// - `T` implements `From<C>`
        /// - `C` implements `From<B>`
        /// - `B` implements `From<A>`
        fn from3<C, B>(item: A) -> Self
        where
            Self: From<C>,
            C: From<B>,
            B: From<A>,
        {
            Self::from2::<C>(B::from(item))
        }

        /// Blanket implementation of `FromX<A>::from4<D, C, B> -> A` for for all `T` provided that
        /// the following requirements are met:
        /// - `T` implements `From<D>`
        /// - `D` implements `From<C>`
        /// - `C` implements `From<B>`
        /// - `B` implements `From<A>`
        fn from4<D, C, B>(item: A) -> Self
        where
            Self: From<D>,
            D: From<C>,
            C: From<B>,
            B: From<A>,
        {
            Self::from3::<D, C>(B::from(item))
        }
    }

    pub trait IntoX<A> {
        fn into2<B>(self) -> A
        where
            A: From<B>,
            B: From<Self>,
            Self: Sized;

        fn into3<C, B>(self) -> A
        where
            A: From<B>,
            B: From<C>,
            C: From<Self>,
            Self: Sized;

        fn into4<D, C, B>(self) -> A
        where
            A: From<B>,
            B: From<C>,
            C: From<D>,
            D: From<Self>,
            Self: Sized;
    }

    impl<A, T> IntoX<A> for T
    where
        T: FromX<A>,
    {
        /// Blanket implementation of `IntoX<T>::into2<B> -> A` for for all `T` provided that the
        /// following requirements are met:
        /// - `A` implements `From<B>`
        /// - `B` implements `From<T>`
        fn into2<B>(self) -> A
        where
            A: From<B>,
            B: From<Self>,
            Self: Sized,
        {
            A::from2::<B>(self)
        }

        /// Blanket implementation of `IntoX<T>::into3<C, B> -> A` for for all `T` provided that the
        /// following requirements are met:
        /// - `A` implements `From<B>`
        /// - `B` implements `From<C>`
        /// - `C` implements `From<T>`
        fn into3<C, B>(self) -> A
        where
            A: From<B>,
            B: From<C>,
            C: From<Self>,
            Self: Sized,
        {
            A::from3::<B, C>(self)
        }

        /// Blanket implementation of `IntoX<T>::into4<D, C, B> -> A` for for all `T` provided that
        /// the following requirements are met:
        /// - `A` implements `From<B>`
        /// - `B` implements `From<C>`
        /// - `C` implements `From<D>`
        /// - `D` implements `From<T>`
        fn into4<D, C, B>(self) -> A
        where
            A: From<B>,
            B: From<C>,
            C: From<D>,
            D: From<Self>,
            Self: Sized,
        {
            A::from4::<B, C, D>(self)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use serde_cbor::Value;
    use witnet_data_structures::{
        chain::RADFilter,
        radon_error::{RadonError, RadonErrors},
        radon_report::RadonReport,
    };

    use crate::{
        filters::RadonFilters,
        operators::RadonOpCodes,
        reducers::RadonReducers,
        types::{float::RadonFloat, integer::RadonInteger, RadonType},
    };

    use super::*;

    #[test]
    fn test_run_http_head_retrieval() {
        let script_r = Value::Array(vec![
            Value::Integer(RadonOpCodes::StringParseJSONMap as i128),
            Value::Array(vec![
                Value::Integer(RadonOpCodes::MapGetString as i128),
                Value::Text("etag".to_string()),
            ]),
        ]);
        let packed_script_r = serde_cbor::to_vec(&script_r).unwrap();

        let retrieve = RADRetrieve {
            kind: RADType::HttpHead,
            url: "https://en.wikipedia.org/static/images/icons/wikipedia.png".to_string(),
            script: packed_script_r,
            body: vec![],
            headers: vec![],
        };
        let response_string = r#"{"date": "Wed, 11 Oct 2023 15:18:42 GMT", "content-type": "image/png", "content-length": "498219", "x-origin-cache": "HIT", "last-modified": "Mon, 28 Aug 2023 13:30:41 GMT", "access-control-allow-origin": "*", "etag": "\"64eca181-79a2b\"", "expires": "Wed, 11 Oct 2023 15:28:41 GMT", "cache-control": "max-age=1800", "x-proxy-cache": "MISS", "x-github-request-id": "6750:35DB:BF8211:FEFD2B:652602FA", "via": "1.1 varnish", "x-served-by": "cache-hnd18736-HND", "x-cache": "MISS", "x-cache-hits": "0", "x-timer": "S1696989946.496383,VS0,VE487", "vary": "Accept-Encoding", "x-fastly-request-id": "118bdfd8a926cbdc781bc23079c3dc07a22d2223", "cf-cache-status": "REVALIDATED", "accept-ranges": "bytes", "report-to": "{\"endpoints\":[{\"url\":\"https:\/\/a.nel.cloudflare.com\/report\/v3?s=FlzxKRCYYN4SL0x%2FraG7ugKCqdC%2BeQqVrucvsfeDWf%2F7A0Nv9fv7TYRgU0WL4k1kbZyxt%2B04VjOyv0XK55sF37GEPwXHE%2FdXnoFlWutID762k2ktcX6hUml6oNk%3D\"}],\"group\":\"cf-nel\",\"max_age\":604800}", "nel": "{\"success_fraction\":0,\"report_to\":\"cf-nel\",\"max_age\":604800}", "strict-transport-security": "max-age=0", "x-content-type-options": "nosniff", "server": "cloudflare", "cf-ray": "814813bf3a73f689-NRT", "alt-svc": "h3=\":443\"; ma=86400"}"#;
        let result = run_retrieval_with_data(
            &retrieve,
            RadonTypes::from(RadonString::from(response_string)),
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        )
        .unwrap();

        match result {
            RadonTypes::String(_) => {}
            err => panic!("Error in run_retrieval: {:?}", err),
        }
    }

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
        let response_string = r#"{"coord":{"lon":13.41,"lat":52.52},"weather":[{"id":500,"main":"Rain","description":"light rain","icon":"10d"}],"base":"stations","main":{"temp":17.59,"pressure":1022,"humidity":67,"temp_min":15,"temp_max":20},"visibility":10000,"wind":{"speed":3.6,"deg":260},"rain":{"1h":0.51},"clouds":{"all":20},"dt":1567501321,"sys":{"type":1,"id":1275,"message":0.0089,"country":"DE","sunrise":1567484402,"sunset":1567533129},"timezone":7200,"id":2950159,"name":"Berlin","cod":200}"#;
        let result = run_retrieval_with_data(
            &retrieve,
            RadonTypes::from(RadonString::from(response_string)),
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
            RADAggregate {
                filters: vec![],
                reducer: RadonReducers::AverageMean as u32,
            },
            &current_active_wips(),
        )
        .unwrap();
        let output_tally = run_tally(
            radon_types_vec,
            &RADTally {
                filters: vec![],
                reducer: RadonReducers::AverageMean as u32,
            },
            &current_active_wips(),
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
        let response_string = "84";
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
            RadonTypes::from(RadonString::from(response_string)),
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        )
        .unwrap();
        let aggregated =
            run_aggregation(vec![retrieved], aggregate, &current_active_wips()).unwrap();
        let tallied = run_tally(vec![aggregated], &tally, &current_active_wips()).unwrap();

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
        let response_string = "307";
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
            RadonTypes::from(RadonString::from(response_string)),
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        )
        .unwrap();
        let aggregated =
            run_aggregation(vec![retrieved], aggregate, &current_active_wips()).unwrap();
        let tallied = run_tally(vec![aggregated], &tally, &current_active_wips()).unwrap();

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
        let response_string = r#"[{"estacion_nombre":"Pza. de España","estacion_numero":4,"fecha":"03092019","hora0":{"estado":"Pasado","valor":"00008"}}]"#;
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
            RadonTypes::from(RadonString::from(response_string)),
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        )
        .unwrap();
        let aggregated =
            run_aggregation(vec![retrieved], aggregate, &current_active_wips()).unwrap();
        let tallied = run_tally(vec![aggregated], &tally, &current_active_wips()).unwrap();

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
        let response_string = r#"{"PSOE":123,"PP":66,"Cs":57,"UP":42,"VOX":24,"ERC-SOBIRANISTES":15,"JxCAT-JUNTS":7,"PNV":6,"EH Bildu":4,"CCa-PNC":2,"NA+":2,"COMPROMÍS 2019":1,"PRC":1,"PACMA":0,"FRONT REPUBLICÀ":0,"BNG":0,"RECORTES CERO-GV":0,"NCa":0,"PACT":0,"ARA-MES-ESQUERRA":0,"GBAI":0,"PUM+J":0,"EN MAREA":0,"PCTE":0,"EL PI":0,"AxSI":0,"PCOE":0,"PCPE":0,"AVANT ADELANTE LOS VERDES":0,"EB":0,"CpM":0,"SOMOS REGIÓN":0,"PCPA":0,"PH":0,"UIG-SOM-CUIDES":0,"ERPV":0,"IZQP":0,"PCPC":0,"AHORA CANARIAS":0,"CxG":0,"PPSO":0,"CNV":0,"PREPAL":0,"C.Ex-C.R.Ex-P.R.Ex":0,"PR+":0,"P-LIB":0,"CILU-LINARES":0,"ANDECHA ASTUR":0,"JF":0,"PYLN":0,"FIA":0,"FE de las JONS":0,"SOLIDARIA":0,"F8":0,"DPL":0,"UNIÓN REGIONALISTA":0,"centrados":0,"DP":0,"VOU":0,"PDSJE-UDEC":0,"IZAR":0,"RISA":0,"C 21":0,"+MAS+":0,"UDT":0}"#;
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
            RadonTypes::from(RadonString::from(response_string)),
            RadonScriptExecutionSettings::disable_all(),
            current_active_wips(),
        )
        .unwrap();
        let aggregated =
            run_aggregation(vec![retrieved], aggregate, &current_active_wips()).unwrap();
        let tallied = run_tally(vec![aggregated], &tally, &current_active_wips()).unwrap();

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
        let response_string = r#"{"event":{"homeTeam":{"name":"Ryazan-VDV","slug":"ryazan-vdv","gender":"F","national":false,"id":171120,"shortName":"Ryazan-VDV","subTeams":[]},"awayTeam":{"name":"Olympique Lyonnais","slug":"olympique-lyonnais","gender":"F","national":false,"id":26245,"shortName":"Lyon","subTeams":[]},"homeScore":{"current":0,"display":0,"period1":0,"normaltime":0},"awayScore":{"current":9,"display":9,"period1":5,"normaltime":9}}}"#;
        let retrieved = run_retrieval_with_data(
            &retrieve,
            RadonTypes::from(RadonString::from(response_string)),
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
            &current_active_wips(),
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
            &current_active_wips(),
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
            &current_active_wips(),
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
            &current_active_wips(),
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
            &current_active_wips(),
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
            &current_active_wips(),
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
            &current_active_wips(),
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
            &current_active_wips(),
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
            &current_active_wips(),
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
            &current_active_wips(),
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
        let report = try_data_request(
            &request,
            RadonScriptExecutionSettings::enable_all(),
            None,
            None,
        );
        let tally_result = report.tally.into_inner();

        if let RadonTypes::Bytes(bytes) = tally_result {
            assert_eq!(bytes.value().len(), 32);
        } else {
            panic!("No RadonBytes result in a RNG request");
        }
    }

    #[test]
    fn test_try_data_request_http_get_non_ascii_header_key() {
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
                kind: RADType::HttpGet,
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
        let report = try_data_request(
            &request,
            RadonScriptExecutionSettings::enable_all(),
            None,
            None,
        );
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
        let report = try_data_request(
            &request,
            RadonScriptExecutionSettings::enable_all(),
            None,
            None,
        );
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
        let report = try_data_request(
            &request,
            RadonScriptExecutionSettings::enable_all(),
            None,
            None,
        );
        let tally_result = report.tally.into_inner();

        assert_eq!(
            tally_result,
            RadonTypes::RadonError(
                RadonError::try_from(RadError::UnhandledIntercept {
                    inner: Some(Box::new(RadError::InvalidHttpHeader {
                        name: "key".to_string(),
                        value: "ñ".to_string(),
                        error: "invalid HTTP header value".to_string()
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
        let report = try_data_request(
            &request,
            RadonScriptExecutionSettings::enable_all(),
            None,
            None,
        );
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
        let report = try_data_request(
            &request,
            RadonScriptExecutionSettings::enable_all(),
            None,
            None,
        );
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
            None,
        );
        let tally_result = report.tally.into_inner();

        assert_eq!(tally_result, RadonTypes::Integer(RadonInteger::from(1)));
    }

    fn reports_from_values(results: Vec<RadonTypes>) -> Vec<Result<RadonReport<RadonTypes>>> {
        let context = ReportContext::from_stage(Stage::Retrieval(RetrievalMetadata::default()));

        results
            .into_iter()
            .map(Ok)
            .map(|result| RadonReport::from_result(result, &context))
            .map(Ok)
            .collect()
    }

    fn aggregate_deviation_standard_and_average_mean(threshold: f32) -> RADAggregate {
        let args = serde_cbor::to_vec(&serde_cbor::Value::from(threshold)).unwrap();
        let filter = RADFilter {
            op: RadonFilters::DeviationStandard as u32,
            args,
        };

        RADAggregate {
            filters: vec![filter],
            reducer: RadonReducers::AverageMean as u32,
        }
    }

    #[test]
    fn test_evaluate_paranoid_retrieval_happy_path() {
        let settings = RadonScriptExecutionSettings::disable_all();
        let data = reports_from_values(vec![
            RadonTypes::from(RadonFloat::from(100)),
            RadonTypes::from(RadonFloat::from(105)),
        ]);
        let aggregate = aggregate_deviation_standard_and_average_mean(1.1);

        let actual_result = evaluate_paranoid_retrieval(data, aggregate, settings, 0.7)
            .unwrap()
            .result;
        let expected_result = RadonTypes::from(RadonFloat::from(102.5));

        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn test_evaluate_paranoid_retrieval_accept_outlier_if_not_paranoid_enough() {
        let settings = RadonScriptExecutionSettings::disable_all();
        let data = reports_from_values(vec![
            RadonTypes::from(RadonFloat::from(100)),
            RadonTypes::from(RadonFloat::from(105)),
            RadonTypes::from(RadonFloat::from(300)),
        ]);
        let aggregate = aggregate_deviation_standard_and_average_mean(1.1);

        let actual_result = evaluate_paranoid_retrieval(data, aggregate, settings, 0.66)
            .unwrap()
            .result;
        let expected_result = RadonTypes::from(RadonFloat::from(102.5));

        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn test_evaluate_paranoid_retrieval_reject_outlier_if_paranoid_enough() {
        let settings = RadonScriptExecutionSettings::disable_all();
        let data = reports_from_values(vec![
            RadonTypes::from(RadonFloat::from(100)),
            RadonTypes::from(RadonFloat::from(105)),
            RadonTypes::from(RadonFloat::from(300)),
        ]);
        let aggregate = aggregate_deviation_standard_and_average_mean(1.1);

        let actual_result =
            evaluate_paranoid_retrieval(data, aggregate, settings, 0.67).unwrap_err();
        let expected_result = RadError::InconsistentSource;

        assert_eq!(actual_result, expected_result);
    }
}
