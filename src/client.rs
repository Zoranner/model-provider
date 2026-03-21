//! 共享 HTTP 客户端：JSON POST，默认 Bearer；[`HttpClient::post_json_with_headers`] 自定义请求头（如 Anthropic `x-api-key`）；[`HttpClient::post_json_query`] 在 URL 上附加 query 且不带 Bearer（如 Google Gemini `key`）。
//!
//! SSE 流式响应见 [`HttpClient::post_bearer_sse`] 等。

use std::pin::Pin;
use std::time::Duration;

use futures::Stream;
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};

use crate::error::{Error, Result};
use crate::sse::{SseByteStream, SseEvent};

/// `text/event-stream` 解析后的 SSE 事件流。
pub type SseEventStream = Pin<Box<dyn Stream<Item = Result<SseEvent>> + Send>>;

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

    /// POST JSON，自定义请求头（不含 `Content-Type`，本方法会设置 `application/json`）。
    #[cfg(feature = "anthropic")]
    pub async fn post_json_with_headers<Req, Resp, F>(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &Req,
        map_err_body: F,
    ) -> Result<Resp>
    where
        Req: Serialize + ?Sized,
        Resp: DeserializeOwned,
        F: FnOnce(String) -> String,
    {
        let mut req = self
            .inner
            .post(url)
            .header("Content-Type", "application/json");
        for (name, value) in headers {
            req = req.header(*name, *value);
        }
        let response = req.json(body).send().await?;

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

    /// POST JSON，URL query 参数（如 `key`），无 `Authorization` 头；本方法会设置 `Content-Type: application/json`。
    #[cfg(feature = "google")]
    pub async fn post_json_query<Req, Resp, F>(
        &self,
        url: &str,
        query: &[(&str, &str)],
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
            .query(query)
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

    /// POST JSON + Bearer，成功时返回 SSE 事件流（`Accept: text/event-stream`）。
    pub async fn post_bearer_sse<Req, F>(
        &self,
        url: &str,
        bearer_token: &str,
        body: &Req,
        map_err_body: F,
    ) -> Result<SseEventStream>
    where
        Req: Serialize + ?Sized,
        F: FnOnce(String) -> String,
    {
        let response = self
            .inner
            .post(url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await?;
        into_sse_stream(response, map_err_body).await
    }

    /// POST JSON + 自定义头，成功时返回 SSE 事件流。
    #[cfg(feature = "anthropic")]
    pub async fn post_json_with_headers_sse<Req, F>(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &Req,
        map_err_body: F,
    ) -> Result<SseEventStream>
    where
        Req: Serialize + ?Sized,
        F: FnOnce(String) -> String,
    {
        let mut req = self
            .inner
            .post(url)
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json");
        for (name, value) in headers {
            req = req.header(*name, *value);
        }
        let response = req.json(body).send().await?;
        into_sse_stream(response, map_err_body).await
    }

    /// POST JSON + URL query，成功时返回 SSE 事件流。
    #[cfg(feature = "google")]
    pub async fn post_json_query_sse<Req, F>(
        &self,
        url: &str,
        query: &[(&str, &str)],
        body: &Req,
        map_err_body: F,
    ) -> Result<SseEventStream>
    where
        Req: Serialize + ?Sized,
        F: FnOnce(String) -> String,
    {
        let response = self
            .inner
            .post(url)
            .query(query)
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await?;
        into_sse_stream(response, map_err_body).await
    }
}

async fn into_sse_stream<F>(response: reqwest::Response, map_err_body: F) -> Result<SseEventStream>
where
    F: FnOnce(String) -> String,
{
    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await?;
        return Err(Error::Api {
            status: status.as_u16(),
            message: map_err_body(body_text),
        });
    }
    Ok(Box::pin(SseByteStream::new(response.bytes_stream())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[derive(Serialize)]
    struct EchoReq {
        n: i32,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct EchoResp {
        msg: String,
    }

    #[tokio::test]
    async fn post_bearer_json_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/echo"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "msg": "hi" })),
            )
            .mount(&server)
            .await;

        let client = HttpClient::new(Duration::from_secs(5)).unwrap();
        let url = format!("{}/echo", server.uri());
        let out: EchoResp = client
            .post_bearer_json(&url, "tok", &EchoReq { n: 1 }, |s| s)
            .await
            .unwrap();
        assert_eq!(out.msg, "hi");
    }

    #[tokio::test]
    async fn post_bearer_json_api_error_invokes_map_err_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/echo"))
            .respond_with(ResponseTemplate::new(422).set_body_string("upstream"))
            .mount(&server)
            .await;

        let client = HttpClient::new(Duration::from_secs(5)).unwrap();
        let url = format!("{}/echo", server.uri());
        let err = client
            .post_bearer_json::<EchoReq, EchoResp, _>(&url, "k", &EchoReq { n: 0 }, |s| {
                format!("wrapped:{s}")
            })
            .await
            .unwrap_err();

        match err {
            Error::Api { status, message } => {
                assert_eq!(status, 422);
                assert_eq!(message, "wrapped:upstream");
            }
            e => panic!("unexpected {e:?}"),
        }
    }

    #[tokio::test]
    async fn post_bearer_json_non_json_success_body_returns_parse() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/echo"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&server)
            .await;

        let client = HttpClient::new(Duration::from_secs(5)).unwrap();
        let url = format!("{}/echo", server.uri());
        let err = client
            .post_bearer_json::<EchoReq, EchoResp, _>(&url, "k", &EchoReq { n: 1 }, |s| s)
            .await
            .unwrap_err();

        assert!(matches!(err, Error::Parse(_)));
    }

    #[tokio::test]
    async fn post_bearer_json_wrong_shape_returns_parse() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/echo"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "wrong": true })),
            )
            .mount(&server)
            .await;

        let client = HttpClient::new(Duration::from_secs(5)).unwrap();
        let url = format!("{}/echo", server.uri());
        let err = client
            .post_bearer_json::<EchoReq, EchoResp, _>(&url, "k", &EchoReq { n: 1 }, |s| s)
            .await
            .unwrap_err();

        assert!(matches!(err, Error::Parse(_)));
    }

    #[tokio::test]
    async fn post_bearer_json_sends_bearer_and_json_content_type() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token-check"))
            .and(header("Authorization", "Bearer secret-key"))
            .and(header("content-type", "application/json"))
            .and(body_json(serde_json::json!({ "n": 42 })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "msg": "ok" })),
            )
            .mount(&server)
            .await;

        let client = HttpClient::new(Duration::from_secs(5)).unwrap();
        let url = format!("{}/token-check", server.uri());
        client
            .post_bearer_json::<EchoReq, EchoResp, _>(&url, "secret-key", &EchoReq { n: 42 }, |s| s)
            .await
            .unwrap();
    }

    #[cfg(feature = "anthropic")]
    #[tokio::test]
    async fn post_json_with_headers_success_and_custom_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/m"))
            .and(header("x-api-key", "secret"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(header("content-type", "application/json"))
            .and(body_json(serde_json::json!({ "n": 7 })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "msg": "ok" })),
            )
            .mount(&server)
            .await;

        let client = HttpClient::new(Duration::from_secs(5)).unwrap();
        let url = format!("{}/m", server.uri());
        let headers = [("x-api-key", "secret"), ("anthropic-version", "2023-06-01")];
        let out: EchoResp = client
            .post_json_with_headers(&url, &headers, &EchoReq { n: 7 }, |s| s)
            .await
            .unwrap();
        assert_eq!(out.msg, "ok");
    }

    #[cfg(feature = "anthropic")]
    #[tokio::test]
    async fn post_json_with_headers_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/m"))
            .respond_with(ResponseTemplate::new(403).set_body_string("denied"))
            .mount(&server)
            .await;

        let client = HttpClient::new(Duration::from_secs(5)).unwrap();
        let url = format!("{}/m", server.uri());
        let err = client
            .post_json_with_headers::<EchoReq, EchoResp, _>(
                &url,
                &[("x-api-key", "k")],
                &EchoReq { n: 1 },
                |s| s,
            )
            .await
            .unwrap_err();

        match err {
            Error::Api { status, message } => {
                assert_eq!(status, 403);
                assert_eq!(message, "denied");
            }
            e => panic!("unexpected {e:?}"),
        }
    }

    #[cfg(feature = "google")]
    #[tokio::test]
    async fn post_json_query_success_and_sends_query() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/models/m:generateContent"))
            .and(query_param("key", "secret-key"))
            .and(header("content-type", "application/json"))
            .and(body_json(serde_json::json!({ "n": 3 })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "msg": "ok" })),
            )
            .mount(&server)
            .await;

        let client = HttpClient::new(Duration::from_secs(5)).unwrap();
        let url = format!("{}/v1beta/models/m:generateContent", server.uri());
        let out: EchoResp = client
            .post_json_query(&url, &[("key", "secret-key")], &EchoReq { n: 3 }, |s| s)
            .await
            .unwrap();
        assert_eq!(out.msg, "ok");
    }

    #[cfg(feature = "google")]
    #[tokio::test]
    async fn post_json_query_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/q"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad"))
            .mount(&server)
            .await;

        let client = HttpClient::new(Duration::from_secs(5)).unwrap();
        let url = format!("{}/q", server.uri());
        let err = client
            .post_json_query::<EchoReq, EchoResp, _>(
                &url,
                &[("key", "k")],
                &EchoReq { n: 1 },
                |s| s,
            )
            .await
            .unwrap_err();

        match err {
            Error::Api { status, message } => {
                assert_eq!(status, 400);
                assert_eq!(message, "bad");
            }
            e => panic!("unexpected {e:?}"),
        }
    }
}
