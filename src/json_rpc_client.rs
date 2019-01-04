use crate::cli::CliCommand;
use failure::Fail;
use log::{info, warn};
use serde::Deserialize;
use std::{
    fmt,
    io::{self, BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::PathBuf,
};
use witnet_config::config::Config;
use witnet_config::loaders::toml;
use witnet_core::actors::config_manager::CONFIG_DEFAULT_FILENAME;

pub(crate) fn run(last_config: Option<PathBuf>, cmd: CliCommand) -> Result<(), failure::Error> {
    match cmd {
        CliCommand::Raw { config } => {
            // The -c/--config argument can come both after and before getBlockChain:
            // witnet cli -c witnet.toml getBlockChain
            // witnet cli getBlockChain -c witnet.toml
            // The last one takes priority
            let config = config.or(last_config);
            let mut stream = start_client(config)?;
            // The request is read from stdin, one line at a time
            let mut request = String::new();
            let stdin = io::stdin();
            let mut stdin = stdin.lock();
            loop {
                request.clear();
                let count = stdin.read_line(&mut request)?;
                if count == 0 {
                    break Ok(());
                }
                let response = send_request(&mut stream, &request)?;
                // The response includes a newline, so use print instead of println
                print!("{}", response);
            }
        }
        CliCommand::GetBlockChain { config } => {
            let config = config.or(last_config);
            let mut stream = start_client(config)?;
            let response = send_request(
                &mut stream,
                r#"{"jsonrpc": "2.0","method": "getBlockChain", "id": 1}"#,
            )?;
            info!("{}", response);
            let block_chain: ResponseBlockChain<'_> = parse_response(&response)?;

            for (epoch, hash) in block_chain {
                println!("Block for epoch #{} had digest {}", epoch, hash);
            }

            Ok(())
        }
    }
}

// Response of the getBlockChain JSON-RPC method
type ResponseBlockChain<'a> = Vec<(u32, &'a str)>;

// Quick and simple JSON-RPC client implementation

/// Generic response which is used to extract the result
#[derive(Debug, Deserialize)]
struct JsonRpcResponse<'a, T> {
    // Lifetimes allow zero-copy string deserialization
    jsonrpc: &'a str,
    id: Id<'a>,
    result: T,
}

/// A failed request returns an error with code and message
#[derive(Debug, Deserialize)]
struct JsonRpcError<'a> {
    jsonrpc: &'a str,
    id: Id<'a>,
    error: ServerError,
}

/// Id. Can be null, a number, or a string
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Id<'a> {
    Null,
    Number(u64),
    String(&'a str),
}

/// A failed request returns an error with code and message
#[derive(Debug, Deserialize, Fail)]
struct ServerError {
    code: i32,
    // This cannot be a &str because the error may outlive the current function
    message: String,
}

#[derive(Debug, Fail)]
struct ServerDisabled;

#[derive(Debug, Fail)]
struct ProtocolError(String);

// Required for Fail derive
impl fmt::Display for ServerDisabled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON-RPC server disabled by configuration")?;
        Ok(())
    }
}

// Required for Fail derive
impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!("{:?}", self))?;
        Ok(())
    }
}

// Required for Fail derive
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!(
            "Incompatible JSON-RPC version used by server: {}",
            self.0
        ))?;
        Ok(())
    }
}

fn start_client(config_path: Option<PathBuf>) -> Result<TcpStream, failure::Error> {
    let config_file = config_path.unwrap_or_else(|| PathBuf::from(CONFIG_DEFAULT_FILENAME));
    let config = Config::from_partial(&toml::from_file(&config_file)?);
    if !config.jsonrpc.enabled {
        return Err(ServerDisabled.into());
    }
    let addr = config.jsonrpc.server_address;
    info!("Connecting to JSON-RPC server at {}", addr);
    let stream = TcpStream::connect(addr);

    stream.map_err(|e| e.into())
}

fn send_request<S: Read + Write>(stream: &mut S, request: &str) -> Result<String, io::Error> {
    stream.write_all(request.as_bytes())?;
    // Write missing newline, if needed
    match bytecount::count(request.as_bytes(), b'\n') {
        0 => stream.write_all(b"\n")?,
        1 => {}
        _ => {
            warn!("The request contains more than one newline, only the first response will be returned");
        }
    }
    // Read only one line
    let mut r = BufReader::new(stream);
    let mut buf = String::new();
    r.read_line(&mut buf)?;
    Ok(buf)
}

fn parse_response<'a, T: Deserialize<'a>>(response: &'a str) -> Result<T, failure::Error> {
    match serde_json::from_str::<JsonRpcResponse<'a, T>>(response) {
        Ok(x) => {
            // x.id should also be checked if we want to support more than one call at a time
            if x.jsonrpc != "2.0" {
                Err(ProtocolError(x.jsonrpc.to_string()).into())
            } else {
                Ok(x.result)
            }
        }
        Err(e) => {
            info!("{}", e);
            let error_json: JsonRpcError<'a> = serde_json::from_str(response)?;
            Err(error_json.error.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_invalid() {
        let nothing: Result<(), _> = parse_response("");
        assert!(nothing.is_err());
        let asdf: Result<(), _> = parse_response("asdf");
        assert!(asdf.is_err());
    }

    #[test]
    fn parse_server_error() {
        let response =
            r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found"},"id":1}"#;
        let block_chain: Result<ResponseBlockChain<'_>, _> = parse_response(&response);
        assert!(block_chain.is_err());
    }

    #[test]
    fn parse_get_block_chain() {
        let response = r#"{"jsonrpc":"2.0","result":[[0,"ed28899af8c3148a4162736af942bc68c4466da93c5124dabfaa7c582af49e30"],[1,"9c9038cfb31a7050796920f91b17f4a68c7e9a795ee8962916b35d39fc1efefc"]],"id":1}"#;
        let block_chain: ResponseBlockChain<'_> = parse_response(&response).unwrap();
        assert_eq!(
            block_chain[0],
            (
                0,
                "ed28899af8c3148a4162736af942bc68c4466da93c5124dabfaa7c582af49e30"
            )
        );
        assert_eq!(
            block_chain[1],
            (
                1,
                "9c9038cfb31a7050796920f91b17f4a68c7e9a795ee8962916b35d39fc1efefc"
            )
        );
    }
}
