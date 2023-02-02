use std::convert::TryFrom;

use isahc::prelude::*;

use async_trait::async_trait;
use failure::Fail;
use isahc::config::RedirectPolicy;

/// Maximum number of HTTP redirects to follow
const MAX_REDIRECTS: u32 = 4;

/// A surf-alike HTTP client that additionally supports proxies (HTTP(S), SOCKS4 and SOCKS5)
#[derive(Clone, Debug)]
pub struct WitnetHttpClient {
    client: isahc::HttpClient,
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

    /// Turn this `WitnetHttpClient` into a `surf`-compatible client
    pub fn as_surf_client(&self) -> surf::Client {
        surf::Client::with_http_client(self.clone())
    }
}

/// Enables interoperability between `isahc::Request` and `surf::http::Request`.
pub struct WitnetHttpRequest {
    req: isahc::Request<isahc::AsyncBody>,
}

impl From<isahc::Request<isahc::AsyncBody>> for WitnetHttpRequest {
    fn from(req: isahc::Request<isahc::AsyncBody>) -> Self {
        Self { req }
    }
}

impl WitnetHttpRequest {
    /// Create `WitnetHttpRequest` from `surf::http::Request`
    pub async fn try_from(
        mut req: surf::http::Request,
    ) -> Result<WitnetHttpRequest, WitnetHttpError> {
        let method = isahc::http::Method::from(WitnetHttpMethod::try_from(req.method())?);
        let version = isahc::http::Version::from(WitnetHttpVersion::try_from(
            req.version().unwrap_or(surf::http::Version::Http1_1),
        )?);
        let uri = req.url().to_string();
        let body_bytes =
            req.take_body()
                .into_bytes()
                .await
                .map_err(|err| WitnetHttpError::TakeBodyError {
                    msg: err.to_string(),
                })?;
        // Use from_bytes_static to avoid error RequestBodyNotRewindable when following an HTTP
        // redirect
        let body = isahc::AsyncBody::from_bytes_static(body_bytes);
        let headers: Vec<(String, String)> = req
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();

        // Start to build an isahc request with the basic parts
        let mut req = isahc::http::Request::builder()
            .method(method)
            .version(version)
            .uri(uri);

        // Attach the headers to the builder
        for (key, value) in headers {
            req = req.header(key, value)
        }

        // Attach the body to the builder and compose the request itself
        let req = req
            .body(body)
            .map_err(|err| WitnetHttpError::HttpRequestError {
                msg: err.to_string(),
            })?;

        Ok(WitnetHttpRequest::from(req))
    }
}

/// Enables interoperability between `isahc::Response` and `surf::http::Response`.
pub struct WitnetHttpResponse {
    res: isahc::Response<isahc::AsyncBody>,
}

impl From<isahc::Response<isahc::AsyncBody>> for WitnetHttpResponse {
    #[inline]
    fn from(res: isahc::Response<isahc::AsyncBody>) -> Self {
        Self { res }
    }
}

impl TryFrom<WitnetHttpResponse> for surf::http::Response {
    type Error = WitnetHttpError;

    fn try_from(res: WitnetHttpResponse) -> Result<Self, Self::Error> {
        // Get the different parts of the isahc response
        let (parts, body) = res.res.into_parts();
        let status = WitnetHttpStatusCode::from(parts.status);
        let version = WitnetHttpVersion::from(parts.version);
        let headers: Vec<(String, String)> = parts
            .headers
            .iter()
            .map(|(key, value)| {
                (
                    key.to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let body_reader = futures::io::BufReader::new(body);

        // Create a surf response and set all the relevant parts
        // Version and headers are currently not used, but they are included here for completion and
        // future readiness.
        let mut res = surf::http::Response::new(status);
        res.set_version(Some(surf::http::Version::try_from(version)?));
        res.set_body(surf::Body::from_reader(body_reader, None));
        for (key, value) in headers {
            res.insert_header(key.as_str(), value);
        }

        Ok(res)
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

impl TryFrom<surf::http::Method> for WitnetHttpMethod {
    type Error = WitnetHttpError;

    fn try_from(method: surf::http::Method) -> Result<Self, Self::Error> {
        let method = match method {
            surf::http::Method::Get => isahc::http::Method::GET,
            surf::http::Method::Post => isahc::http::Method::POST,
            method => Err(Self::Error::UnsupportedMethod {
                method: method.to_string(),
            })?,
        };

        Ok(WitnetHttpMethod::from(method))
    }
}

impl From<WitnetHttpMethod> for isahc::http::Method {
    #[inline]
    fn from(method: WitnetHttpMethod) -> Self {
        method.method
    }
}

/// Enables interoperability between `isahc::http::StatusCode` and `surf::StatusCode`.
pub struct WitnetHttpStatusCode {
    status: isahc::http::StatusCode,
}

impl From<isahc::http::StatusCode> for WitnetHttpStatusCode {
    #[inline]
    fn from(status: isahc::http::StatusCode) -> Self {
        Self { status }
    }
}

impl TryFrom<WitnetHttpStatusCode> for surf::StatusCode {
    type Error = WitnetHttpError;

    fn try_from(status: WitnetHttpStatusCode) -> Result<Self, Self::Error> {
        let code = status.status.as_u16();
        surf::StatusCode::try_from(code).map_err(|err| WitnetHttpError::UnknownStatusCode {
            code,
            msg: err.to_string(),
        })
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

impl TryFrom<surf::http::Version> for WitnetHttpVersion {
    type Error = WitnetHttpError;

    fn try_from(version: surf::http::Version) -> Result<Self, Self::Error> {
        let version = match version {
            surf::http::Version::Http0_9 => isahc::http::version::Version::HTTP_09,
            surf::http::Version::Http1_0 => isahc::http::version::Version::HTTP_10,
            surf::http::Version::Http1_1 => isahc::http::version::Version::HTTP_11,
            surf::http::Version::Http2_0 => isahc::http::version::Version::HTTP_2,
            surf::http::Version::Http3_0 => isahc::http::version::Version::HTTP_3,
            other => Err(Self::Error::UnknownVersion {
                version: other.to_string(),
            })?,
        };

        Ok(Self::from(version))
    }
}

impl TryFrom<WitnetHttpVersion> for surf::http::Version {
    type Error = WitnetHttpError;

    fn try_from(version: WitnetHttpVersion) -> Result<Self, Self::Error> {
        let version = match version.version {
            isahc::http::version::Version::HTTP_09 => surf::http::Version::Http0_9,
            isahc::http::version::Version::HTTP_10 => surf::http::Version::Http1_0,
            isahc::http::version::Version::HTTP_11 => surf::http::Version::Http1_1,
            isahc::http::version::Version::HTTP_2 => surf::http::Version::Http2_0,
            isahc::http::version::Version::HTTP_3 => surf::http::Version::Http3_0,
            other => Err(Self::Error::UnknownVersion {
                version: format!("{:?}", other),
            })?,
        };

        Ok(version)
    }
}

impl From<WitnetHttpVersion> for isahc::http::version::Version {
    #[inline]
    fn from(version: WitnetHttpVersion) -> Self {
        version.version
    }
}

#[async_trait]
impl surf::HttpClient for WitnetHttpClient {
    async fn send(&self, req: surf::http::Request) -> Result<surf::http::Response, surf::Error> {
        // Transform surf request into isahc request
        let req = WitnetHttpRequest::try_from(req)
            .await
            .map_err(surf::Error::from_display)?
            .req;

        // Send HTTP request and wait for response
        let res = self.client.send_async(req).await?;

        // Transform isahc response into surf response
        let res = surf::http::Response::try_from(WitnetHttpResponse::from(res))
            .map_err(surf::Error::from_display)?;

        Ok(res)
    }
}
