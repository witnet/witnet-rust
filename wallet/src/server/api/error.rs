use failure::Fail;
use jsonrpc_core as rpc;
use serde_json::json;

use super::validation;
use crate::*;

#[derive(Debug, Fail)]
pub enum ApiError {
    #[fail(display = "{}", _0)]
    Internal(#[cause] failure::Error),
    #[fail(display = "Validation Errors: {:?}", _0)]
    Validation(validation::ValidationErrors),
}

pub fn internal<E: Fail>(err: E) -> ApiError {
    ApiError::Internal(failure::Error::from(err))
}

impl From<actix::MailboxError> for ApiError {
    fn from(err: actix::MailboxError) -> Self {
        internal(err)
    }
}

impl From<diesel::r2d2::PoolError> for ApiError {
    fn from(err: diesel::r2d2::PoolError) -> Self {
        internal(err)
    }
}

impl From<error::Error> for ApiError {
    fn from(err: error::Error) -> Self {
        internal(err)
    }
}

impl From<ApiError> for jsonrpc_core::Error {
    fn from(err: ApiError) -> Self {
        match err {
            ApiError::Internal(cause) => {
                let mut error = rpc::Error::new(rpc::ErrorCode::InternalError);
                error.data = Some(json!({ "cause": format!("{}", cause) }));

                error
            }
            ApiError::Validation(causes) => rpc::Error {
                code: rpc::ErrorCode::ServerError(400),
                message: "Validation Error".into(),
                data: Some(serde_json::to_value(causes).expect("serialization of errors failed")),
            },
        }
    }
}

impl From<validation::ValidationErrors> for ApiError {
    fn from(err: validation::ValidationErrors) -> Self {
        ApiError::Validation(err)
    }
}

impl From<crypto::Error> for ApiError {
    fn from(err: crypto::Error) -> Self {
        internal(err)
    }
}

impl From<failure::Error> for ApiError {
    fn from(err: failure::Error) -> Self {
        ApiError::Internal(err)
    }
}

impl<T> From<std::sync::PoisonError<T>> for ApiError {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        ApiError::Internal(failure::format_err!(
            "Mutex poison error! Restart the application."
        ))
    }
}
