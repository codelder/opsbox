use crate::search::Search;
use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
    routing::get,
};
use flate2::read::GzDecoder;
use minio::s3::{Client, types::S3Api};
use std::time::Instant;
use std::{io::BufReader, sync::Arc};
use std::{io::Read, pin::Pin};
use tokio::{io::AsyncRead, sync::mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::{StreamReader, SyncIoBridge};

use crate::{
    log_storage::{LogStorage, MinioLogStorage, grep_context_from_reader},
    search::{ReaderProvider, S3ReaderProvider},
};

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

pub fn router() -> Router<Client> {
    Router::new()
        .route("/stream", get(stream_mark))
        .route("/stream_json", get(stream_json))
}

pub async fn stream_markdown(
    State(client): State<Client>,
    Query(query): Query<SearchQuery>,
) -> HttpResponse<Body> {
    println!("stream_markdown start");
    let start = Instant::now();
    let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(8);
    let context_lines: usize = query.context.unwrap_or(3);
    let keyword = query.q.clone();
    println!("context_lines: {}", context_lines);
    println!("keyword: {}", keyword);

    let _ = tx.send(Ok(bytes::Bytes::from("# 搜索结果\n\n"))).await;
    println!("send Ok(bytes::Bytes::from(# 搜索结果\n\n)))");

    let start_for_fut = start;
    let fut = async move {
        match client.get_object("test", "codeler.tar.gz").send().await {
            Ok(object) => {
                // 将 MinIO 的 ObjectContent 转为字节流
                let (stream, _size) = match object.content.to_stream().await {
                    Ok(pair) => pair,
                    Err(e) => {
                        let _ = tx
                            .send(Ok(bytes::Bytes::from(format!("获取对象流失败: {:?}\n", e))))
                            .await;
                        return;
                    }
                };
                let reader = StreamReader::new(stream);

                // 在阻塞线程中桥接为同步 Read，边解压边解包
                tokio::task::spawn_blocking(move || {
                    let start_inner = start_for_fut;
                    let bucket = "test";
                    let object_name = "codeler.tar.gz";
                    let send_block = |s: &str| {
                        let _ = tx.blocking_send(Ok(bytes::Bytes::from(s.to_owned())));
                    };

                    // 将 AsyncRead 桥接为 std::io::Rea
                    let sync_reader = SyncIoBridge::new(reader);
                    let gz = GzDecoder::new(sync_reader);
                    let mut archive = tar::Archive::new(gz);
                    let entries = match archive.entries() {
                        Ok(e) => e,
                        Err(e) => {
                            send_block(&format!("读取 tar 条目失败: {:?}\n", e));
                            println!("stream_markdown failed after {:?}", start_inner.elapsed());
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
                        let mut reader = BufReader::with_capacity(8 * 1024, entry);
                        let (lines, merged) =
                            match grep_context_from_reader(&mut reader, &keyword, context_lines) {
                                Ok(Some((lines, merged))) => (lines, merged),
                                Ok(None) => continue,
                                Err(_) => continue,
                            };
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
                    println!("stream_markdown completed in {:?}", start_inner.elapsed());
                });
            }
            Err(e) => {
                let _ = tx
                    .send(Ok(bytes::Bytes::from(format!(
                        "连接 MinIO 失败: {:?}\n",
                        e
                    ))))
                    .await;
                println!("stream_markdown failed after {:?}", start_for_fut.elapsed());
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

pub async fn stream_json(
    State(client): State<Client>,
    Query(query): Query<SearchQuery>,
) -> HttpResponse<Body> {
    let bucket = "test";
    let object_name = "codeler.tar.gz";
    let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(8);
    if tx.send(Ok(bytes::Bytes::from("开始搜索"))).await.is_err() {
        return HttpResponse::builder()
            .status(500)
            .body(Body::from("发送失败"))
            .unwrap();
    }

    let context_lines: usize = query.context.unwrap_or(3);
    let keyword = query.q.clone();

    let fut = async move {
        let storage = LogStorage::new(MinioLogStorage::new(client, "test"));
        let stream = match storage.open_archive_stream("codeler.tar.gz").await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(Err(e)).await;
                return;
            }
        };
        let reader = StreamReader::new(stream);
        tokio::task::spawn_blocking(move || {
            let send_block = |s: &str| {
                let _ = tx.blocking_send(Ok(bytes::Bytes::from(s.to_owned())));
            };
            let sync_reader = SyncIoBridge::new(reader);

            let gz = GzDecoder::new(sync_reader);
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
                let mut reader = BufReader::with_capacity(8 * 1024, entry);
                let (lines, merged) =
                    match grep_context_from_reader(&mut reader, &keyword, context_lines) {
                        Ok(Some((lines, merged))) => (lines, merged),
                        Ok(None) => continue,
                        Err(_) => continue,
                    };
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
