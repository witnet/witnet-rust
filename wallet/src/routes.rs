//! Defines functions and macros related to request-to-handler routing.

/// Helper macro to add multiple JSON-RPC methods at once
#[macro_export]
macro_rules! routes {
    // No args: do nothing
    ($io:expr $(,)+) => {};
    ($io:expr, ($method_jsonrpc:expr, $method_rust:expr $(,)*), $($args:tt)*) => {
        // Base case:
        {
            $io.add_method($method_jsonrpc, move |params: rpc::Params| {
                $method_rust(params).and_then(|result| match json::to_value(result) {
                    Ok(value) => future::ok(value),
                    Err(err) => {
                        log::error!("Error serializing the response result: {}", err);
                        future::err(rpc::Error {
                            code: rpc::ErrorCode::ServerError(err_codes::SERIALIZATION_ERROR),
                            message: "Failed to serialize the response".into(),
                            data: None,
                        })
                    }
                })
            });
        }
        // Recursion!
        routes!($io, $($args)*);
    };
}

/// Macro to add multiple JSON-RPC methods that forward the request to the Node at once
#[macro_export]
macro_rules! forwarded_routes {
    ($io:expr $(,)*) => {};
    ($io:expr, $method_jsonrpc:expr, $($args:tt)*) => {
        // Base case:
        {
            $io.add_method($method_jsonrpc, move |params: rpc::Params| {
                client::send(client::request($method_jsonrpc).params(params))
            });
        }
        // Recursion!
        forwarded_routes!($io, $($args)*);
    };
}
