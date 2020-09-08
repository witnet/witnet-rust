use std::str::FromStr;
use std::sync::Arc;

use futures03::future::BoxFuture;
use surf::http::{self, Response, Uri};
use surf::middleware::{Body, HttpClient as SurfHttpClient};
use surf::Client;

type Request = http::Request<Body>;

/// A surf-alike HTTP client that additionally supports proxies (HTTP(S), SOCKS4 and SOCKS5)
#[derive(Clone, Debug)]
pub struct WitnetHttpClient {
    client: Arc<isahc::HttpClient>,
}

impl WitnetHttpClient {
    /// Create a new `WitnetHttpClient`
    pub fn new(proxy: &Option<String>) -> Self {
        let proxy_address = proxy.as_ref().map(|x| Uri::from_str(x.as_str()).unwrap());
        let client = Arc::new(
            if let Some(proxy_address) = proxy_address {
                isahc::HttpClient::builder().proxy(proxy_address)
            } else {
                isahc::HttpClient::builder()
            }
            .build()
            .unwrap(),
        );

        Self { client }
    }

    /// Turn this `WitnetHttpClient` into a `surf`-compatible client
    pub fn as_surf_client(&self) -> Client<Self> {
        Client::with_client(self.clone())
    }
}

impl SurfHttpClient for WitnetHttpClient {
    type Error = isahc::Error;

    fn send(&self, req: Request) -> BoxFuture<'static, Result<Response<Body>, Self::Error>> {
        let client = self.client.clone();
        Box::pin(async move {
            // Compose HTTP request
            let (parts, body) = req.into_parts();
            let body = isahc::Body::reader(body);
            let req: http::Request<isahc::Body> = http::Request::from_parts(parts, body);

            // Send HTTP request and wait for response
            let res = client.send_async(req).await?;

            // Read HTTP response
            let (parts, body) = res.into_parts();
            let body = Body::from_reader(body);
            let res = http::Response::from_parts(parts, body);

            Ok(res)
        })
    }
}
