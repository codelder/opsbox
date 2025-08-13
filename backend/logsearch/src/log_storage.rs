use flate2::read::GzDecoder;
use std::io::Read;
use tar::Archive;

enum Log {
    BytesLog(Vec<u8>),
    ArchiveLog(Archive<Box<dyn Read>>),
    GzipLog(GzDecoder<Box<dyn Read>>),
}
struct LogStorage {
    logs: Vec<Log>,
}

impl LogStorage {
    fn new() -> Self {
        Self { logs: vec![] }
    }
}
