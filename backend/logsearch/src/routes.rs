use crate::{search::Search, storage::{ReaderProvider as _, S3ReaderProvider}};
use axum::{
    Router,
    body::Body,
    extract::Query,
    http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
    routing::get,
};
use std::{io::Read, pin::Pin};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::SyncIoBridge;


#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub context: Option<usize>,
}

fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn highlight_with_mark(input: &str, keyword: &str) -> String {
    if keyword.is_empty() {
        return escape_html(input);
    }
    let mut out = String::with_capacity(input.len() + 16);
    let mut start_idx = 0usize;
    while let Some(pos) = input[start_idx..].find(keyword) {
        let abs = start_idx + pos;
        out.push_str(&escape_html(&input[start_idx..abs]));
        out.push_str("<mark>");
        out.push_str(&escape_html(&input[abs..abs + keyword.len()]));
        out.push_str("</mark>");
        start_idx = abs + keyword.len();
    }
    out.push_str(&escape_html(&input[start_idx..]));
    out
}

pub fn router() -> Router {
    Router::new()
        .route("/stream", get(stream_mark))
}

pub async fn stream_mark(Query(query): Query<SearchQuery>) -> HttpResponse<Body> {
    let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(8);
    let _ = tx.send(Ok(bytes::Bytes::from("# 搜索结果\n\n"))).await;

    let fut = async move {
        let s3reader = S3ReaderProvider::new(
            "http://192.168.50.61:9002",
            "admin",
            "G5t3o6f2",
            "test",
            "codeler.tar.gz",
        )
        .open()
        .await
        .unwrap();

        tokio::task::spawn_blocking(move || {
            let reader_blocking: Box<dyn Read + Send> =
                Box::new(SyncIoBridge::new(Pin::from(s3reader)));

            let send_block = |s: String| {
                let _ = tx.blocking_send(Ok(bytes::Bytes::from(s.to_owned())));
            };

            reader_blocking.search(
                &query.q,
                query.context.unwrap_or(3),
                send_block,
                |s, lines, merged| {
                    let mut buf = String::new();
                    buf.push_str(&format!(
                        "\n## 文件 s3://{}/{}::{}\n\n",
                        "test", "codeler.tar.gz", s
                    ));
                    buf.push_str("<pre>\n");
                    for (chunk_idx, (s, e)) in merged.iter().copied().enumerate() {
                        for i in s..=e {
                            use std::fmt::Write as _;
                            let highlighted = highlight_with_mark(&lines[i], &query.q);
                            let _ = write!(&mut buf, "{:>6} | {}\n", i + 1, highlighted);
                        }
                        if chunk_idx + 1 < merged.len() {
                            buf.push_str("       ...\n");
                        }
                    }
                    buf.push_str("</pre>\n\n");
                    buf
                },
            )
        });
    };

    tokio::spawn(fut);

    let body = axum::body::Body::from_stream(ReceiverStream::new(rx));
    HttpResponse::builder()
        .status(200)
        .header(
            CONTENT_TYPE,
            HeaderValue::from_static("text/markdown; charset=utf-8"),
        )
        .body(body)
        .unwrap()
}
