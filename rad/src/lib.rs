//! # RAD Engine

use reqwest;

use witnet_data_structures::chain::{RADAggregate, RADRetrieve, RADTally, RADType};
use witnet_data_structures::radon_report::{RadonReport, ReportContext, Stage, TallyMetaData};

use crate::error::RadError;
use crate::script::{execute_radon_script, unpack_radon_script};
use crate::types::{array::RadonArray, string::RadonString, RadonTypes};

pub mod error;
pub mod filters;
pub mod hash_functions;
pub mod operators;
pub mod reducers;
pub mod script;
pub mod types;

pub type Result<T> = std::result::Result<T, RadError>;

/// Run retrieval without performing any external network requests, return `RadonReport`.
pub fn run_retrieval_with_data_report(
    retrieve: &RADRetrieve,
    response: String,
    context: &mut ReportContext,
) -> Result<RadonReport<RadonTypes>> {
    match retrieve.kind {
        RADType::HttpGet => {
            let input = RadonTypes::from(RadonString::from(response));
            let radon_script = unpack_radon_script(&retrieve.script)?;

            execute_radon_script(input, &radon_script, context)
        }
    }
}

/// Run retrieval without performing any external network requests, return `RadonTypes`.
pub fn run_retrieval_with_data(retrieve: &RADRetrieve, response: String) -> Result<RadonTypes> {
    let context = &mut ReportContext::default();
    run_retrieval_with_data_report(retrieve, response, context).and_then(RadonReport::into_inner)
}

/// Run retrieval stage of a data request, return `RadonReport`.
pub fn run_retrieval_report(retrieve: &RADRetrieve) -> Result<RadonReport<RadonTypes>> {
    let context = &mut ReportContext::default();
    context.stage = Stage::Retrieval;

    match retrieve.kind {
        RADType::HttpGet => {
            let response = reqwest::get(&retrieve.url)
                .map_err(RadError::from)?
                .text()
                .map_err(RadError::from)?;

            let result = run_retrieval_with_data_report(retrieve, response, context);

            log::debug!("Result for URL {}: {:?}", retrieve.url, result);

            result
        }
    }
}

/// Run retrieval stage of a data request, return `RadonTypes`.
pub fn run_retrieval(retrieve: &RADRetrieve) -> Result<RadonTypes> {
    run_retrieval_report(retrieve).and_then(RadonReport::into_inner)
}

/// Run aggregate stage of a data request, return `RadonReport`.
pub fn run_aggregation_report(
    radon_types_vec: Vec<RadonTypes>,
    aggregate: &RADAggregate,
) -> Result<RadonReport<RadonTypes>> {
    let context = &mut ReportContext::default();
    context.stage = Stage::Aggregation;

    let radon_script = unpack_radon_script(aggregate.script.as_slice())?;
    let items_to_aggregate = RadonTypes::from(RadonArray::from(radon_types_vec));

    execute_radon_script(items_to_aggregate, &radon_script, context)
}

/// Run aggregate stage of a data request, return `RadonTypes`.
pub fn run_aggregation(
    radon_types_vec: Vec<RadonTypes>,
    aggregate: &RADAggregate,
) -> Result<RadonTypes> {
    run_aggregation_report(radon_types_vec, aggregate).and_then(RadonReport::into_inner)
}

/// Run tally stage of a data request, return `RadonReport`.
pub fn run_tally_report(
    radon_types_vec: Vec<RadonTypes>,
    consensus: &RADTally,
) -> Result<RadonReport<RadonTypes>> {
    let context = &mut ReportContext::default();
    context.stage = Stage::Tally(TallyMetaData::default());

    let radon_script = unpack_radon_script(consensus.script.as_slice())?;
    let items_to_tally = RadonTypes::from(RadonArray::from(radon_types_vec));

    execute_radon_script(items_to_tally, &radon_script, context)
}

/// Run tally stage of a data request, return `RadonTypes`.
pub fn run_tally(radon_types_vec: Vec<RadonTypes>, consensus: &RADTally) -> Result<RadonTypes> {
    run_tally_report(radon_types_vec, consensus).and_then(RadonReport::into_inner)
}

#[test]
fn test_run_retrieval() {
    let script = vec![
        134, 24, 69, 24, 116, 130, 1, 100, 109, 97, 105, 110, 24, 116, 130, 1, 100, 116, 101, 109,
        112, 24, 114,
    ];

    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
        script
    };
    let response = r#"{"coord":{"lon":13.41,"lat":52.52},"weather":[{"id":500,"main":"Rain","description":"light rain","icon":"10d"}],"base":"stations","main":{"temp":17.59,"pressure":1022,"humidity":67,"temp_min":15,"temp_max":20},"visibility":10000,"wind":{"speed":3.6,"deg":260},"rain":{"1h":0.51},"clouds":{"all":20},"dt":1567501321,"sys":{"type":1,"id":1275,"message":0.0089,"country":"DE","sunrise":1567484402,"sunset":1567533129},"timezone":7200,"id":2950159,"name":"Berlin","cod":200}"#;

    let result = run_retrieval_with_data(&retrieve, response.to_string()).unwrap();

    match result {
        RadonTypes::Float(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}

#[test]
fn test_run_consensus_and_aggregation() {
    use crate::types::float::RadonFloat;

    let f_1 = RadonTypes::Float(RadonFloat::from(1f64));
    let f_3 = RadonTypes::Float(RadonFloat::from(3f64));

    let radon_types_vec = vec![f_1, f_3];

    let packed_script = vec![129, 130, 24, 87, 3];

    let expected = RadonTypes::Float(RadonFloat::from(2f64));

    let output_aggregate = run_aggregation(
        radon_types_vec.clone(),
        &RADAggregate {
            script: packed_script.clone(),
        },
    )
    .unwrap();
    let output_tally = run_tally(
        radon_types_vec,
        &RADTally {
            script: packed_script,
        },
    )
    .unwrap();

    assert_eq!(output_aggregate, expected);
    assert_eq!(output_tally, expected);
}

#[test]
#[ignore]
fn test_run_retrieval_random_api() {
    let script = vec![
        133, 24, 83, 24, 132, 130, 1, 100, 100, 97, 116, 97, 24, 128, 130, 1, 0,
    ];
    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "http://qrng.anu.edu.au/API/jsonI.php?length=1&type=uint8".to_string(),
        script,
    };

    let response = r#"{"type":"uint8","length":1,"data":[4],"success":true}"#;
    let result = run_retrieval_with_data(&retrieve, response.to_string()).unwrap();

    match result {
        RadonTypes::Float(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}

#[test]
fn test_run_all_risk_premium() {
    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "https://wrapapi.com/use/aesedepece/ffzz/prima/0.0.3?wrapAPIKey=ql4DVWylABdXCpt1NUTLNEDwPH57aHGm".to_string(),
        script: vec![129, 24, 65],
    };
    let response = "84";
    let aggregate = RADAggregate {
        script: vec![129, 130, 24, 87, 3],
    };
    let tally = RADTally {
        script: vec![130, 130, 24, 87, 3, 130, 24, 52, 24, 80],
    };

    let retrieved = run_retrieval_with_data(&retrieve, response.to_string()).unwrap();
    let aggregated = run_aggregation(vec![retrieved], &aggregate).unwrap();
    let tallied = run_tally(vec![aggregated], &tally).unwrap();

    match tallied {
        RadonTypes::Boolean(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}

#[test]
fn test_run_all_murders() {
    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "https://wrapapi.com/use/aesedepece/ffzz/murders/0.0.2?wrapAPIKey=ql4DVWylABdXCpt1NUTLNEDwPH57aHGm".to_string(),
        script: vec![129, 24, 65],
    };
    let response = "307";
    let aggregate = RADAggregate {
        script: vec![129, 130, 24, 87, 3],
    };
    let tally = RADTally {
        script: vec![130, 130, 24, 87, 3, 130, 24, 52, 24, 200],
    };

    let retrieved = run_retrieval_with_data(&retrieve, response.to_string()).unwrap();
    let aggregated = run_aggregation(vec![retrieved], &aggregate).unwrap();
    let tallied = run_tally(vec![aggregated], &tally).unwrap();

    match tallied {
        RadonTypes::Boolean(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}

#[test]
fn test_run_all_air_quality() {
    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "http://airemadrid.herokuapp.com/api/estacion".to_string(),
        script: vec![
            135, 24, 69, 24, 112, 130, 24, 85, 0, 130, 24, 97, 101, 104, 111, 114, 97, 48, 24, 116,
            130, 24, 97, 101, 118, 97, 108, 111, 114, 24, 114,
        ],
    };
    // This response was modified because the original was about 100KB.
    let response = r#"[{"estacion_nombre":"Pza. de España","estacion_numero":4,"fecha":"03092019","hora0":{"estado":"Pasado","valor":"00008"}}]"#;
    let aggregate = RADAggregate {
        script: vec![129, 130, 24, 87, 3],
    };
    let tally = RADTally {
        script: vec![130, 130, 24, 87, 3, 130, 24, 52, 10],
    };

    let retrieved = run_retrieval_with_data(&retrieve, response.to_string()).unwrap();
    let aggregated = run_aggregation(vec![retrieved], &aggregate).unwrap();
    let tallied = run_tally(vec![aggregated], &tally).unwrap();

    match tallied {
        RadonTypes::Boolean(_) => {}
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}

#[test]
fn test_run_all_elections() {
    use crate::types::RadonType;

    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "https://wrapapi.com/use/aesedepece/ffzz/generales/0.0.3?wrapAPIKey=ql4DVWylABdXCpt1NUTLNEDwPH57aHGm".to_string(),
        script: vec![132, 24, 69, 24, 116, 130, 24, 97, 100, 80, 83, 79, 69, 24, 114],
    };
    let response = r#"{"PSOE":123,"PP":66,"Cs":57,"UP":42,"VOX":24,"ERC-SOBIRANISTES":15,"JxCAT-JUNTS":7,"PNV":6,"EH Bildu":4,"CCa-PNC":2,"NA+":2,"COMPROMÍS 2019":1,"PRC":1,"PACMA":0,"FRONT REPUBLICÀ":0,"BNG":0,"RECORTES CERO-GV":0,"NCa":0,"PACT":0,"ARA-MES-ESQUERRA":0,"GBAI":0,"PUM+J":0,"EN MAREA":0,"PCTE":0,"EL PI":0,"AxSI":0,"PCOE":0,"PCPE":0,"AVANT ADELANTE LOS VERDES":0,"EB":0,"CpM":0,"SOMOS REGIÓN":0,"PCPA":0,"PH":0,"UIG-SOM-CUIDES":0,"ERPV":0,"IZQP":0,"PCPC":0,"AHORA CANARIAS":0,"CxG":0,"PPSO":0,"CNV":0,"PREPAL":0,"C.Ex-C.R.Ex-P.R.Ex":0,"PR+":0,"P-LIB":0,"CILU-LINARES":0,"ANDECHA ASTUR":0,"JF":0,"PYLN":0,"FIA":0,"FE de las JONS":0,"SOLIDARIA":0,"F8":0,"DPL":0,"UNIÓN REGIONALISTA":0,"centrados":0,"DP":0,"VOU":0,"PDSJE-UDEC":0,"IZAR":0,"RISA":0,"C 21":0,"+MAS+":0,"UDT":0}"#;
    let aggregate = RADAggregate {
        script: vec![129, 130, 24, 87, 3],
    };
    let tally = RADTally {
        script: vec![129, 130, 24, 87, 3],
    };

    let retrieved = run_retrieval_with_data(&retrieve, response.to_string()).unwrap();
    let aggregated = run_aggregation(vec![retrieved], &aggregate).unwrap();
    let tallied = run_tally(vec![aggregated], &tally).unwrap();

    match tallied {
        RadonTypes::Float(radon_float) => {
            assert!((radon_float.value() - 123f64).abs() < std::f64::EPSILON)
        }
        err => panic!("Error in run_retrieval: {:?}", err),
    }
}

#[test]
fn test_run_football() {
    use crate::types::integer::RadonInteger;

    let retrieve = RADRetrieve {
        kind: RADType::HttpGet,
        url: "https://www.sofascore.com/event/8397714/json".to_string(),
        script: vec![
            137, 24, 69, 24, 116, 130, 24, 97, 101, 101, 118, 101, 110, 116, 24, 116, 130, 24, 97,
            105, 97, 119, 97, 121, 83, 99, 111, 114, 101, 24, 116, 130, 24, 97, 103, 99, 117, 114,
            114, 101, 110, 116, 24, 114, 24, 60,
        ],
    };
    let response = r#"{"event":{"homeTeam":{"name":"Ryazan-VDV","slug":"ryazan-vdv","gender":"F","national":false,"id":171120,"shortName":"Ryazan-VDV","subTeams":[]},"awayTeam":{"name":"Olympique Lyonnais","slug":"olympique-lyonnais","gender":"F","national":false,"id":26245,"shortName":"Lyon","subTeams":[]},"homeScore":{"current":0,"display":0,"period1":0,"normaltime":0},"awayScore":{"current":9,"display":9,"period1":5,"normaltime":9}}}"#;
    let retrieved = run_retrieval_with_data(&retrieve, response.to_string()).unwrap();
    let expected = RadonTypes::Integer(RadonInteger::from(9));
    assert_eq!(retrieved, expected)
}
