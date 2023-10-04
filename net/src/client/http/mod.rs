use isahc::prelude::*;

use failure::Fail;
use isahc::config::RedirectPolicy;
use isahc::http;
use isahc::http::request::Builder;

/// Maximum number of HTTP redirects to follow
const MAX_REDIRECTS: u32 = 4;

/// A surf-alike HTTP client that additionally supports proxies (HTTP(S), SOCKS4 and SOCKS5)
#[derive(Clone, Debug)]
pub struct WitnetHttpClient {
    client: isahc::HttpClient,
}

impl WitnetHttpClient {
    /// Simple wrapper around `isahc::HttpClient::send_async`.
    pub async fn send(
        &self,
        request: WitnetHttpRequest,
    ) -> Result<WitnetHttpResponse, WitnetHttpError> {
        Ok(WitnetHttpResponse::from(
            self.client
                .send_async(request.req)
                .await
                .map_err(|e| WitnetHttpError::HttpRequestError { msg: e.to_string() })?,
        ))
    }
}

/// Errors for WitnetHttpClient and other auxiliary structures in this module.
#[derive(Clone, Debug, Eq, Fail, PartialEq)]
pub enum WitnetHttpError {
    /// Error when trying to build a WitnetHttpClient.
    #[fail(
        display = "Error when trying to build a WitnetHttpClient. Underlying error: {}",
        msg
    )]
    ClientBuildError {
        /// An error message.
        msg: String,
    },
    /// HTTP request error.
    #[fail(display = "HTTP request error. Underlying error: {}", msg)]
    HttpRequestError {
        /// An error message.
        msg: String,
    },
    /// The provided proxy URI is invalid.
    #[fail(
        display = "The provided proxy address is not a valid URI ({}). Underlying error: {}",
        address, msg
    )]
    InvalidProxyUri {
        /// The provided invalid address.
        address: String,
        /// An error message.
        msg: String,
    },
    /// Found an unknown HTTP status code.
    #[fail(
        display = "Unknown HTTP status code ({}). Underlying error: {}",
        code, msg
    )]
    UnknownStatusCode {
        /// The unknown status code.
        code: u16,
        /// An error message.
        msg: String,
    },
    /// Found an unknown HTTP version.
    #[fail(display = "Unknown HTTP version ({})", version)]
    UnknownVersion {
        /// The unknown HTTP version.
        version: String,
    },
    /// Tried to process an HTTP request with an unsupported HTTP method.
    #[fail(
        display = "Tried to process an HTTP request with an unsupported HTTP method {}",
        method
    )]
    UnsupportedMethod {
        /// The unsupported HTTP method.
        method: String,
    },
    /// Error taking body from request.
    #[fail(display = "Error taking body from request: {}", msg)]
    TakeBodyError {
        /// An error message
        msg: String,
    },
}

impl WitnetHttpClient {
    /// Create a new `WitnetHttpClient`
    pub fn new(
        proxy: impl Into<Option<isahc::http::Uri>>,
        follow_redirects: bool,
    ) -> Result<Self, WitnetHttpError> {
        // Build an `isahc::HttpClient`. Will use the proxy URI, if any
        let client = isahc::HttpClient::builder()
            .proxy(proxy)
            .redirect_policy(if follow_redirects {
                RedirectPolicy::Limit(MAX_REDIRECTS)
            } else {
                RedirectPolicy::None
            })
            .build()
            .map_err(|err| WitnetHttpError::ClientBuildError {
                msg: err.to_string(),
            })?;

        Ok(Self { client })
    }
}

/// Alias for the specific type of body that we use.
pub type WitnetHttpBody = isahc::AsyncBody;
/// Alias for our request builder.
pub type WitnetHttpRequestBuilder = http::request::Builder;
type Request = http::Request<WitnetHttpBody>;

/// Enables interoperability between `isahc::Request` and `surf::http::Request`.
pub struct WitnetHttpRequest {
    req: isahc::Request<WitnetHttpBody>,
}

impl WitnetHttpRequest {
    /// Allows creating a `WitnetHttpRequest` using the same API from `http::request::Builder`.
    pub fn build<F, E>(mut f: F) -> Result<Self, E>
    where
        F: FnMut(WitnetHttpRequestBuilder) -> Result<Request, E>,
    {
        Ok(Self {
            req: f(Builder::new())?,
        })
    }
}

impl From<isahc::Request<isahc::AsyncBody>> for WitnetHttpRequest {
    fn from(req: isahc::Request<isahc::AsyncBody>) -> Self {
        Self { req }
    }
}

/// Enables interoperability between `isahc::Response` and `surf::http::Response`.
pub struct WitnetHttpResponse {
    res: isahc::Response<isahc::AsyncBody>,
}

impl WitnetHttpResponse {
    #[inline]
    /// Simple wrapper around `isahc::Response::status`.
    pub fn inner(self) -> isahc::Response<isahc::AsyncBody> {
        self.res
    }
}

impl From<isahc::Response<isahc::AsyncBody>> for WitnetHttpResponse {
    #[inline]
    fn from(res: isahc::Response<isahc::AsyncBody>) -> Self {
        Self { res }
    }
}

/// Enables interoperability between `isahc::http::Method` and `surf::http::Method`.
pub struct WitnetHttpMethod {
    method: isahc::http::Method,
}

impl From<isahc::http::Method> for WitnetHttpMethod {
    #[inline]
    fn from(method: isahc::http::Method) -> Self {
        Self { method }
    }
}

impl From<WitnetHttpMethod> for isahc::http::Method {
    #[inline]
    fn from(method: WitnetHttpMethod) -> Self {
        method.method
    }
}

/// Enables interoperability between `isahc::http::version::Version` and `surf::http::Version`.
pub struct WitnetHttpVersion {
    version: isahc::http::version::Version,
}

impl From<isahc::http::version::Version> for WitnetHttpVersion {
    #[inline]
    fn from(version: isahc::http::version::Version) -> Self {
        Self { version }
    }
}

impl From<WitnetHttpVersion> for isahc::http::version::Version {
    #[inline]
    fn from(version: WitnetHttpVersion) -> Self {
        version.version
    }
}
