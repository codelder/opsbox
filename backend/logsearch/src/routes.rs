use crate::{
    search::{Search as _, SearchResult},
    storage::{ReaderProvider as _, S3ReaderProvider},
};
use axum::{
    Router,
    body::Body,
    extract::{Json, OriginalUri},
    http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
    routing::post,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchBody {
    #[serde(default)]
    pub q: Vec<String>,
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

fn highlight_with_mark(input: &str, keywords: &[String]) -> String {
    // 过滤空关键词，避免死循环
    let non_empty: Vec<&str> = keywords
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    if non_empty.is_empty() {
        return escape_html(input);
    }
    let mut out = String::with_capacity(input.len() + 16);
    let mut start_idx = 0usize;
    while start_idx < input.len() {
        let mut best_pos: Option<usize> = None;
        let mut best_kw: &str = "";

        for &kw in &non_empty {
            if let Some(pos_rel) = input[start_idx..].find(kw) {
                let pos_abs = start_idx + pos_rel;
                match best_pos {
                    None => {
                        best_pos = Some(pos_abs);
                        best_kw = kw;
                    }
                    Some(bp) => {
                        if pos_abs < bp || (pos_abs == bp && kw.len() > best_kw.len()) {
                            best_pos = Some(pos_abs);
                            best_kw = kw;
                        }
                    }
                }
            }
        }

        match best_pos {
            None => {
                out.push_str(&escape_html(&input[start_idx..]));
                break;
            }
            Some(pos) => {
                out.push_str(&escape_html(&input[start_idx..pos]));
                out.push_str("<mark>");
                let end = pos + best_kw.len();
                out.push_str(&escape_html(&input[pos..end]));
                out.push_str("</mark>");
                start_idx = end;
            }
        }
    }
    out
}

pub fn router() -> Router {
    Router::new().route("/stream", post(stream_mark2))
}

fn render_result(result: SearchResult, query: &SearchBody) -> String {
    let mut buf = String::new();
    buf.push_str(&format!(
        "\n## 文件 s3://{}/{}::{}\n\n",
        "test", "codeler.tar.gz", result.path
    ));
    buf.push_str("<pre>\n");
    for (chunk_idx, (s, e)) in result.merged.iter().copied().enumerate() {
        for i in s..=e {
            use std::fmt::Write as _;
            let highlighted = highlight_with_mark(&result.lines[i], &query.q);
            let _ = write!(&mut buf, "{:>6} | {}\n", i + 1, highlighted);
        }
        if chunk_idx + 1 < result.merged.len() {
            buf.push_str("       ...\n");
        }
    }
    buf.push_str("</pre>\n\n");
    buf
}

async fn stream_mark2(
    original_uri: OriginalUri,
    body: Result<Json<SearchBody>, axum::extract::rejection::JsonRejection>,
) -> HttpResponse<Body> {
    let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(8);

    let body = match body {
        Ok(json) => {
            let b = json.0;
            eprintln!("[logsearch] body ok: q={:?}, context={:?}", b.q, b.context);
            b
        }
        Err(err) => {
            eprintln!("[logsearch] body error: {} | uri={}", err, original_uri.0);
            return HttpResponse::builder()
                .status(400)
                .header(
                    CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                )
                .body(Body::from(format!(
                    "body error: {}\nuri: {}\n",
                    err, original_uri.0
                )))
                .unwrap();
        }
    };

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

        let Ok(mut stream) = s3reader.search(&body.q, body.context.unwrap_or(3)).await else {
            return;
        };

        while let Some(result) = stream.recv().await {
            let buf = render_result(result, &body);
            let _ = tx.send(Ok(bytes::Bytes::from(buf))).await;
        }
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
