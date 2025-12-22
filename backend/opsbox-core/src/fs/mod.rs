pub mod entry_stream;

pub use entry_stream::{
  ArchiveKind, EntryMeta, EntrySource, EntryStream, FsEntryStream, GzipEntryStream, MultiFileEntryStream,
  TarEntryStream, TarGzEntryStream, create_archive_stream_from_reader, open_file_with_compression_detection,
  sniff_archive_kind,
};
