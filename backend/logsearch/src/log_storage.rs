use tar::Archive;

enum Log {
    BytesLog(Vec<u8>),
    ArchiveLog(Archive),
    GzipLog(Gzip),
}
struct LogStorage {
    logs: Vec<Log>,
}

impl LogStorage {
    fn new() -> Self {
        Self { logs: vec![] }
    }
}
