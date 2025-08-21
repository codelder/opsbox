use std::io::{self, BufRead, BufReader, Read};

use async_trait::async_trait;
use flate2::read::GzDecoder;
use tar::Archive;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("fill buf error: {0}")]
    FillBuf(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub trait BlockingSearch {
    fn search(
        self,
        keywords: &[String],
        context_lines: usize,
    ) -> Result<tokio::sync::mpsc::Receiver<SearchResult>, SearchError>;
}

#[async_trait]
pub trait Search {
    async fn search(
        self,
        keywords: &[String],
        context_lines: usize,
    ) -> Result<tokio::sync::mpsc::Receiver<SearchResult>, SearchError>;
}

fn is_probably_text(reader: &mut impl BufRead) -> bool {
    let sample_len;
    let sample = {
        let Ok(buf) = reader.fill_buf() else {
            return false;
        };
        sample_len = buf.len().min(4096);
        &buf[..sample_len]
    };

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

fn grep_context_from_reader<R: BufRead>(
    reader: &mut R,
    keywords: &[String],
    context_lines: usize,
) -> Result<Option<(Vec<String>, Vec<(usize, usize)>)>, SearchError> {
    if !is_probably_text(reader) {
        return Ok(None);
    }

    let lines: Vec<String> = reader
        .lines()
        .collect::<io::Result<Vec<_>>>()
        .map_err(|e| SearchError::FillBuf(e.to_string()))?;

    // 对每个关键词收集命中范围，任一关键词未命中则直接 None
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for kw in keywords {
        let mut hit_any = false;
        for (idx, line) in lines.iter().enumerate() {
            if line.contains(&kw) {
                hit_any = true;
                let s = idx.saturating_sub(context_lines);
                let e = std::cmp::min(idx + context_lines, lines.len().saturating_sub(1));
                ranges.push((s, e));
            }
        }
        if !hit_any {
            return Ok(None);
        }
    }

    if ranges.is_empty() {
        return Ok(None);
    }

    // 合并相邻/重叠范围（与原逻辑一致）
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

    Ok(Some((lines, merged)))
}

// fn grep_context_from_reader<R: BufRead>(
//     reader: &mut R,
//     keyword: &str,
//     context_lines: usize,
// ) -> Result<Option<(Vec<String>, Vec<(usize, usize)>)>, SearchError> {
//     // 采样以判断是否为文本
//     if !is_probably_text(reader) {
//         return Ok(None);
//     }

//     // 读全行（按需也可改为边读边输出）
//     let lines: Vec<String> = reader
//         .lines()
//         .collect::<io::Result<Vec<_>>>()
//         .map_err(|e| SearchError::FillBuf(e.to_string()))?;

//     // 寻找匹配范围
//     let mut ranges: Vec<(usize, usize)> = Vec::new();
//     for (idx, line) in lines.iter().enumerate() {
//         if line.contains(keyword) {
//             let s = idx.saturating_sub(context_lines);
//             let e = std::cmp::min(idx + context_lines, lines.len().saturating_sub(1));
//             ranges.push((s, e));
//         }
//     }
//     if ranges.is_empty() {
//         return Ok(None);
//     }

//     // 合并相邻/重叠范围
//     ranges.sort_by_key(|r| r.0);
//     let mut merged: Vec<(usize, usize)> = Vec::new();
//     for (s, e) in ranges {
//         if let Some(last) = merged.last_mut() {
//             if s <= last.1 + 1 {
//                 if e > last.1 {
//                     last.1 = e;
//                 }
//                 continue;
//             }
//         }
//         merged.push((s, e));
//     }

//     Ok(Some((lines, merged)))
// }
pub struct SearchResult {
    pub path: String,
    pub lines: Vec<String>,
    pub merged: Vec<(usize, usize)>,
}

impl SearchResult {
    fn new(path: String, lines: Vec<String>, merged: Vec<(usize, usize)>) -> Self {
        Self {
            path,
            lines,
            merged,
        }
    }
}

impl BlockingSearch for Box<dyn Read + Send> {
    fn search(
        self,
        keywords: &[String],
        context_lines: usize,
    ) -> Result<tokio::sync::mpsc::Receiver<SearchResult>, SearchError> {
        let (tx, rx) = tokio::sync::mpsc::channel::<SearchResult>(8);
        let keywords_owned: Vec<String> = keywords.to_owned();
        tokio::task::spawn_blocking(move || -> Result<(), SearchError> {
            let mut archive = tar::Archive::new(GzDecoder::new(self));
            for entry in archive.entries()?.flatten() {
                let path = entry
                    .path()
                    .ok()
                    .map(|p| p.into_owned().display().to_string()) // 拿到 owned String
                    .unwrap_or_default();
                let mut reader = BufReader::with_capacity(8192, entry);
                if let Ok(Some((lines, merged))) =
                    grep_context_from_reader(&mut reader, &keywords_owned, context_lines)
                {
                    let _ = tx.blocking_send(SearchResult::new(path, lines, merged));
                }
            }
            Ok(())
        });
        Ok(rx)
    }
}

impl<T: Read + Send + 'static> BlockingSearch for Archive<T> {
    fn search(
        mut self,
        keywords: &[String],
        context_lines: usize,
    ) -> Result<tokio::sync::mpsc::Receiver<SearchResult>, SearchError> {
        let (tx, rx) = tokio::sync::mpsc::channel::<SearchResult>(8);
        let keywords_owned: Vec<String> = keywords.to_owned();
        tokio::task::spawn_blocking(move || -> Result<(), SearchError> {
            for entry in self.entries()?.flatten() {
                let path = entry
                    .path()
                    .ok()
                    .map(|p| p.into_owned().display().to_string()) // 拿到 owned String
                    .unwrap_or_default();
                let mut reader = BufReader::with_capacity(8192, entry);
                if let Ok(Some((lines, merged))) =
                    grep_context_from_reader(&mut reader, &keywords_owned, context_lines)
                {
                    let _ = tx.blocking_send(SearchResult::new(path, lines, merged));
                }
            }
            Ok(())
        });
        Ok(rx)
    }
}

#[async_trait]
impl Search for tokio::fs::ReadDir {
    async fn search(
        self,
        keywords: &[String],
        context_lines: usize,
    ) -> Result<tokio::sync::mpsc::Receiver<SearchResult>, SearchError> {
        let (tx, rx) = tokio::sync::mpsc::channel::<SearchResult>(8);

        let keywords_owned: Vec<String> = keywords.to_owned();
        tokio::spawn({
            let mut stack = vec![self];
            async move {
                while let Some(mut rd) = stack.pop() {
                    while let Some(entry) =
                        rd.next_entry().await.map_err(SearchError::Io)?
                    {
                        let path = entry.path();
                        let fty = entry.file_type().await.map_err(SearchError::Io)?;
                        if fty.is_dir() {
                            if let Ok(sub) = tokio::fs::read_dir(&path).await {
                                stack.push(sub);
                            }
                            continue;
                        }
                        if fty.is_file() {
                            let txf = tx.clone();
                            tokio::task::spawn_blocking(move || -> Result<(), SearchError> {
                                let file = std::fs::File::open(&path)?;
                                let mut reader = std::io::BufReader::with_capacity(8192, file);
                                if let Ok(Some((lines, merged))) =
                                    grep_context_from_reader(&mut reader, &keywords_owned, context_lines)
                                {
                                    let _ = txf.blocking_send(SearchResult::new(
                                        path.display().to_string(),
                                        lines,
                                        merged,
                                    ));
                                }
                                Ok(())
                            });
                        }
                    }
                }
                Ok::<(), SearchError>(())
            }
        });

        Ok(rx)
    }
}
