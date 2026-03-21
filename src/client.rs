//! 共享 HTTP 客户端（Bearer + JSON）

use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: Client,
}

impl HttpClient {
    pub fn new(timeout: Duration) -> Result<Self> {
        let inner = Client::builder().timeout(timeout).build()?;
        Ok(Self { inner })
    }

    /// POST JSON，`Authorization: Bearer {token}`。非 2xx 时用 `map_err_body` 处理响应体文案。
    pub async fn post_bearer_json<Req, Resp, F>(
        &self,
        url: &str,
        bearer_token: &str,
        body: &Req,
        map_err_body: F,
    ) -> Result<Resp>
    where
        Req: Serialize + ?Sized,
        Resp: DeserializeOwned,
        F: FnOnce(String) -> String,
    {
        let response = self
            .inner
            .post(url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await?;

        let status = response.status();
        let body_text = response.text().await?;

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                message: map_err_body(body_text),
            });
        }

        serde_json::from_str(&body_text).map_err(|e| Error::Parse(e.to_string()))
    }
}
