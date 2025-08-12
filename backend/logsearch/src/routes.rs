use std::io::{BufRead, BufReader};

use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
    routing::get,
};
use bytes::Bytes;
use flate2::read::GzDecoder;
use minio::s3::{Client, types::S3Api};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub context: Option<usize>,
}

fn is_probably_text(sample: &[u8]) -> bool {
    if sample.is_empty() {
        return true;
    }
    if sample.contains(&0) {
        return false;
    }
    let printable = sample
        .iter()
        .filter(|b| matches!(**b, 0x09 | 0x0A | 0x0D | 0x20..=0x7E))
        .count();
    let ratio = printable as f32 / sample.len() as f32;
    if ratio >= 0.95 {
        return true;
    }
    std::str::from_utf8(sample).is_ok()
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

pub fn router() -> Router<Client> {
    Router::new().route("/stream", get(stream_markdown))
}

pub async fn stream_markdown(
    State(client): State<Client>,
    Query(query): Query<SearchQuery>,
) -> HttpResponse<Body> {
    println!("stream_markdown");
    let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(8);
    let context_lines: usize = query.context.unwrap_or(3);
    let keyword = query.q.clone();
    println!("context_lines: {}", context_lines);
    println!("keyword: {}", keyword);

    let _ = tx.send(Ok(bytes::Bytes::from("# 搜索结果\n\n"))).await;
    println!("send Ok(bytes::Bytes::from(# 搜索结果\n\n)))");

    let fut = async move {
        match client.get_object("test", "codeler.tar.gz").send().await {
            Ok(object) => {
                let gz_bytes: Bytes = match object.content.to_segmented_bytes().await {
                    Ok(seg) => seg.to_bytes(),
                    Err(e) => {
                        let _ = tx
                            .send(Ok(bytes::Bytes::from(format!("获取对象失败: {:?}\n", e))))
                            .await;
                        return;
                    }
                };
                tokio::task::spawn_blocking(move || {
                    let bucket = "test";
                    let object_name = "codeler.tar.gz";
                    let send_block = |s: &str| {
                        let _ = tx.blocking_send(Ok(bytes::Bytes::from(s.to_owned())));
                    };
                    let gz = GzDecoder::new(gz_bytes.as_ref());
                    let mut archive = tar::Archive::new(gz);
                    let entries = match archive.entries() {
                        Ok(e) => e,
                        Err(e) => {
                            send_block(&format!("读取 tar 条目失败: {:?}\n", e));
                            return;
                        }
                    };
                    for entry in entries {
                        let entry = match entry {
                            Ok(e) => e,
                            Err(_) => continue,
                        };
                        let path_in_tar = match entry.path() {
                            Ok(p) => p.into_owned(),
                            Err(_) => continue,
                        };
                        let mut reader = BufReader::new(entry);
                        let sample_len;
                        let sample = match reader.fill_buf() {
                            Ok(buf) => {
                                sample_len = buf.len().min(4096);
                                &buf[..sample_len]
                            }
                            Err(_) => continue,
                        };
                        if !is_probably_text(sample) {
                            continue;
                        }
                        let lines: Vec<String> = match reader.lines().collect() {
                            Ok(ls) => ls,
                            Err(_) => continue,
                        };
                        let mut ranges: Vec<(usize, usize)> = Vec::new();
                        for (idx, line) in lines.iter().enumerate() {
                            if line.contains(&keyword) {
                                let s = idx.saturating_sub(context_lines);
                                let e = std::cmp::min(
                                    idx + context_lines,
                                    lines.len().saturating_sub(1),
                                );
                                ranges.push((s, e));
                            }
                        }
                        if ranges.is_empty() {
                            continue;
                        }
                        ranges.sort_by_key(|r| r.0);
                        let mut merged: Vec<(usize, usize)> = Vec::new();
                        for (s, e) in ranges {
                            if let Some(last) = merged.last_mut() {
                                if s <= last.1 + 1 {
                                    if e > last.1 {
                                        last.1 = e;
                                    }
                                    continue;
                                }
                            }
                            merged.push((s, e));
                        }
                        let full_path =
                            format!("s3://{}/{}::{}", bucket, object_name, path_in_tar.display());
                        send_block(&format!("\n## 文件: {}\n\n", full_path));
                        send_block("<pre>\n");
                        let mut buf = String::new();
                        for (chunk_idx, (s, e)) in merged.iter().copied().enumerate() {
                            for i in s..=e {
                                use std::fmt::Write as _;
                                let highlighted = highlight_with_mark(&lines[i], &keyword);
                                let _ = write!(&mut buf, "{:>6} | {}\n", i + 1, highlighted);
                            }
                            if chunk_idx + 1 < merged.len() {
                                buf.push_str("       ...\n");
                            }
                        }
                        send_block(&buf);
                        send_block("</pre>\n\n");
                    }
                });
            }
            Err(e) => {
                let _ = tx
                    .send(Ok(bytes::Bytes::from(format!(
                        "连接 MinIO 失败: {:?}\n",
                        e
                    ))))
                    .await;
            }
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
