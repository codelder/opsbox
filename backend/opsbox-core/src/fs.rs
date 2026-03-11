pub mod entry_stream;
pub mod file_type;

pub use entry_stream::{
  EntryMeta, EntrySource, EntryStream, FsEntryStream, GzipEntryStream, MultiFileEntryStream, PrefixedReader,
  SniffArchiveKind, TarArchiveEntryStream, create_archive_stream_from_reader, extract_archive_entry,
  normalize_archive_entry_path,
  open_archive_typed, open_file_with_compression_detection, sniff_archive_kind,
};
pub use file_type::{FileKind, sniff_file_type};
