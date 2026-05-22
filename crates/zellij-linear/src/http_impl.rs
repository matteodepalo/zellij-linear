use std::time::Duration;

use linear_client::http::{HttpClient, HttpError, HttpResponse, HttpVerb};

pub struct ReqwestClient {
    client: reqwest::blocking::Client,
}

impl ReqwestClient {
    pub fn new() -> Result<Self, HttpError> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("zellij-linear/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        Ok(Self { client })
    }
}

impl HttpClient for ReqwestClient {
    fn request(
        &self,
        url: &str,
        verb: HttpVerb,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<HttpResponse, HttpError> {
        let mut req = match verb {
            HttpVerb::Get => self.client.get(url),
            HttpVerb::Post => self.client.post(url),
        };
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        if verb == HttpVerb::Post {
            req = req.body(body.to_vec());
        }
        let resp = req
            .send()
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp
            .bytes()
            .map_err(|e| HttpError::Transport(e.to_string()))?
            .to_vec();
        Ok(HttpResponse { status, body })
    }
}
