use std::io::{
    Cursor
};

use slog::{
    Discard,
    Logger
};

pub fn null_logger() -> Logger {
    Logger::root(Discard, o!())
}

pub fn empty_file() -> Cursor<Vec<u8>> {
    Cursor::new(vec![])
}

pub fn empty_bam() -> Cursor<Vec<u8>> {
    Cursor::new(vec![
        0x1f, 0x8b,             // gzip identifier
        0x08,                   // method (deflate)
        0x04,                   // flags (FEXTRA)
        0x00, 0x00, 0x00, 0x00, // modification time
        0x00,                   // extra flags
        0xff,                   // operating system (unknown)
        0x06, 0x00,             // extra field length (6 bytes)
        0x42, 0x43,             // bgzf identifier
        0x02, 0x00,             // extra subfield length (2 bytes)
        0x1b, 0x00,             // bgzf block size -1 (27 => block size is 28)
        0x03, 0x00,             // deflated empty payload
        0x00, 0x00, 0x00, 0x00, // uncompressed deflate payload CRC32 (0)
        0x00, 0x00, 0x00, 0x00  // uncompressed deflate payload size (0)
    ])
}
