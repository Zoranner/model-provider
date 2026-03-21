//! 将 `text/event-stream` 字节流解析为 SSE 事件（`event` + `data`）。

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Stream;
use reqwest::Error as ReqwestError;

use crate::error::{Error, Result};

/// 一条 SSE 事件：`event` 可选（无 `event:` 行时仅 `data`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

struct SseParser {
    buf: Vec<u8>,
    pending: VecDeque<SseEvent>,
    current_event: Option<String>,
    current_data: Vec<String>,
}

impl SseParser {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            pending: VecDeque::new(),
            current_event: None,
            current_data: Vec::new(),
        }
    }

    fn push_bytes(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
        self.drain_lines();
    }

    /// 上游已关闭：处理缓冲区尾部（无换行符的残留行）并 flush。
    fn finalize(&mut self) {
        if !self.buf.is_empty() {
            let line = String::from_utf8_lossy(&self.buf).into_owned();
            self.buf.clear();
            self.process_line(&line);
        }
        self.flush_event();
    }

    fn drain_lines(&mut self) {
        loop {
            let pos = self.buf.iter().position(|&b| b == b'\n');
            let Some(i) = pos else {
                break;
            };
            let mut line = self.buf.drain(..=i).collect::<Vec<u8>>();
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line).into_owned();
            self.process_line(&line);
        }
    }

    fn process_line(&mut self, line: &str) {
        if line.is_empty() {
            self.flush_event();
            return;
        }
        if line.starts_with(':') {
            return;
        }
        if let Some(v) = line.strip_prefix("event:") {
            self.current_event = Some(v.trim().to_string());
            return;
        }
        if let Some(v) = line.strip_prefix("data:") {
            self.current_data.push(v.trim_start().to_string());
        }
    }

    fn flush_event(&mut self) {
        if self.current_event.is_none() && self.current_data.is_empty() {
            return;
        }
        let data = self.current_data.join("\n");
        self.current_data.clear();
        let ev = SseEvent {
            event: self.current_event.take(),
            data,
        };
        if !ev.data.is_empty() || ev.event.is_some() {
            self.pending.push_back(ev);
        }
    }

    fn pop_event(&mut self) -> Option<SseEvent> {
        self.pending.pop_front()
    }
}

/// 将 HTTP 响应体字节流解析为 [`SseEvent`] 流。
pub struct SseByteStream<S> {
    inner: S,
    parser: SseParser,
    finished: bool,
}

impl<S> SseByteStream<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            parser: SseParser::new(),
            finished: false,
        }
    }
}

impl<S> Stream for SseByteStream<S>
where
    S: Stream<Item = std::result::Result<Bytes, ReqwestError>> + Unpin,
{
    type Item = Result<SseEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();
        loop {
            if let Some(ev) = this.parser.pop_event() {
                return Poll::Ready(Some(Ok(ev)));
            }
            if this.finished {
                return Poll::Ready(None);
            }
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    this.parser.finalize();
                    this.finished = true;
                    continue;
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(Error::Http(e)))),
                Poll::Ready(Some(Ok(bytes))) => {
                    this.parser.push_bytes(&bytes);
                }
            }
        }
    }
}
