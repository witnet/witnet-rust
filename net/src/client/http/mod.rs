use std::time::Duration;

use reqwest;
use thiserror::Error;

/// Maximum number of HTTP redirects to follow
const MAX_REDIRECTS: usize = 4;

/// A surf-alike HTTP client that additionally supports proxies (HTTP(S), SOCKS4 and SOCKS5)
#[derive(Clone, Debug)]
pub struct WitnetHttpClient {
    client: reqwest::Client,
}

impl WitnetHttpClient {
    /// Simple wrapper around `isahc::HttpClient::send_async`.
    ///
    /// Opinionated in only one thing: if a timeout is not specified, it uses a 10 seconds timeout.
    pub async fn send(
        &self,
        request: reqwest::RequestBuilder,
        timeout: Option<Duration>,
    ) -> Result<WitnetHttpResponse, WitnetHttpError> {
        let timeout = timeout.unwrap_or(Duration::from_secs(10));
        let req = match request.timeout(timeout).build() {
            Ok(req) => req,
            Err(e) => return Err(WitnetHttpError::HttpRequestError { msg: e.to_string() }),
        };

        Ok(WitnetHttpResponse::from(
            self.client
                .execute(req)
                .await
                .map_err(|e| WitnetHttpError::HttpRequestError { msg: e.to_string() })?,
        ))
    }
}

/// Errors for WitnetHttpClient and other auxiliary structures in this module.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum WitnetHttpError {
    /// Error when trying to build a WitnetHttpClient.
    #[error("Error when trying to build a WitnetHttpClient. Underlying error: {msg}")]
    ClientBuildError {
        /// An error message.
        msg: String,
    },
    /// HTTP request error.
    #[error("HTTP request error. Underlying error: {msg}")]
    HttpRequestError {
        /// An error message.
        msg: String,
    },
    /// The provided proxy URI is invalid.
    #[error("The provided proxy address is not a valid URI ({address}). Underlying error: {msg}")]
    InvalidProxyUri {
        /// The provided invalid address.
        address: String,
        /// An error message.
        msg: String,
    },
    /// Found an unknown HTTP status code.
    #[error("Unknown HTTP status code ({code}). Underlying error: {msg}")]
    UnknownStatusCode {
        /// The unknown status code.
        code: u16,
        /// An error message.
        msg: String,
    },
    /// Found an unknown HTTP version.
    #[error("Unknown HTTP version ({version})")]
    UnknownVersion {
        /// The unknown HTTP version.
        version: String,
    },
    /// Tried to process an HTTP request with an unsupported HTTP method.
    #[error("Tried to process an HTTP request with an unsupported HTTP method {method}")]
    UnsupportedMethod {
        /// The unsupported HTTP method.
        method: String,
    },
    /// Error taking body from request.
    #[error("Error taking body from request: {msg}")]
    TakeBodyError {
        /// An error message
        msg: String,
    },
}

impl WitnetHttpClient {
    /// Create a new `WitnetHttpClient`
    pub fn new(
        proxy: Option<reqwest::Url>,
        follow_redirects: bool,
    ) -> Result<Self, WitnetHttpError> {
        let redirect_policy = if follow_redirects {
            reqwest::redirect::Policy::limited(MAX_REDIRECTS)
        } else {
            reqwest::redirect::Policy::none()
        };

        let client = match proxy {
            Some(proxy) => {
                let proxy = reqwest::Proxy::http(proxy).map_err(|err| {
                    WitnetHttpError::ClientBuildError {
                        msg: err.to_string(),
                    }
                })?;

                reqwest::Client::builder()
                    .proxy(proxy)
                    .redirect(redirect_policy)
                    .build()
                    .map_err(|err| WitnetHttpError::ClientBuildError {
                        msg: err.to_string(),
                    })?
            }
            None => reqwest::Client::builder()
                .redirect(redirect_policy)
                .build()
                .map_err(|err| WitnetHttpError::ClientBuildError {
                    msg: err.to_string(),
                })?,
        };

        Ok(Self { client })
    }

    /// Build an HTTP GET request
    pub fn get(self, url: reqwest::Url) -> reqwest::RequestBuilder {
        self.client.get(url)
    }

    /// Build an HTTP POST request
    pub fn post(self, url: reqwest::Url) -> reqwest::RequestBuilder {
        self.client.post(url)
    }

    /// Build an HTTP HEAD request
    pub fn head(self, url: reqwest::Url) -> reqwest::RequestBuilder {
        self.client.head(url)
    }
}

/// Alias for the specific type of body that we use.
pub type WitnetHttpBody = reqwest::Body;

/// Wrapper around reqwest::Response
pub struct WitnetHttpResponse {
    res: reqwest::Response,
}

impl WitnetHttpResponse {
    #[inline]
    /// Simple wrapper around `reqwest::Response::status`.
    pub fn inner(self) -> reqwest::Response {
        self.res
    }
}

impl From<reqwest::Response> for WitnetHttpResponse {
    #[inline]
    fn from(res: reqwest::Response) -> Self {
        Self { res }
    }
}

/// Wrapper around reqwest::Method
pub struct WitnetHttpMethod {
    method: reqwest::Method,
}

impl From<reqwest::Method> for WitnetHttpMethod {
    #[inline]
    fn from(method: reqwest::Method) -> Self {
        Self { method }
    }
}

impl From<WitnetHttpMethod> for reqwest::Method {
    #[inline]
    fn from(method: WitnetHttpMethod) -> Self {
        method.method
    }
}

/// Wrapper around reqwest::Version
pub struct WitnetHttpVersion {
    version: reqwest::Version,
}

impl From<reqwest::Version> for WitnetHttpVersion {
    #[inline]
    fn from(version: reqwest::Version) -> Self {
        Self { version }
    }
}

impl From<WitnetHttpVersion> for reqwest::Version {
    #[inline]
    fn from(version: WitnetHttpVersion) -> Self {
        version.version
    }
}
