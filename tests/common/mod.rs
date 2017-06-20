use byteorder::{
    LittleEndian,
    WriteBytesExt
};

use std::io::{
    Cursor
};

use slog::{
    Discard,
    Logger
};

fn bgzf_block(deflated_payload: Vec<u8>, inflated_payload_size: u32, inflated_payload_crc32: u32) -> Vec<u8> {
    let mut data = vec![
        0x1f, 0x8b,             // gzip identifier
        0x08,                   // method (deflate)
        0x04,                   // flags (FEXTRA)
        0x00, 0x00, 0x00, 0x00, // modification time
        0x00,                   // extra flags
        0xff,                   // operating system (unknown)
        0x06, 0x00,             // extra field length (6 bytes)
        0x42, 0x43,             // bgzf identifier
        0x02, 0x00,             // extra subfield length (2 bytes)
    ];

    data.write_u16::<LittleEndian>(deflated_payload.len() as u16 + 25u16).unwrap(); // bgzf block size, minus 1

    data.extend(deflated_payload);
    data.write_u32::<LittleEndian>(inflated_payload_crc32).unwrap();
    data.write_u32::<LittleEndian>(inflated_payload_size).unwrap();

    data
}

pub fn empty_bgzf_block() -> Vec<u8> {
    bgzf_block(vec![
        0x03, 0x00 // deflated empty string
    ], 0, 0)
}

pub fn regular_bgzf_block() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 5, 907060870)
}

pub fn bad_payload_crc32_bgzf_block() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0x25, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 5, 907060870)
}

pub fn bad_payload_size_bgzf_block() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 25, 907060870)
}

pub fn null_logger() -> Logger {
    Logger::root(Discard, o!())
}

pub fn empty_file() -> Cursor<Vec<u8>> {
    Cursor::new(vec![])
}

pub fn empty_bam() -> Cursor<Vec<u8>> {
    Cursor::new(empty_bgzf_block())
}

pub fn single_block_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(empty_bgzf_block());
    Cursor::new(data)
}

pub fn two_blocks_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(regular_bgzf_block());
    data.extend(empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(regular_bgzf_block());
    data.extend(regular_bgzf_block());
    data.extend(empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_empty_inside_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(regular_bgzf_block());
    data.extend(empty_bgzf_block());
    data.extend(regular_bgzf_block());
    data.extend(empty_bgzf_block());
    Cursor::new(data)
}

pub fn single_block_missing_empty_bam() -> Cursor<Vec<u8>> {
    Cursor::new(regular_bgzf_block())
}

pub fn single_block_missing_gzip_identifier_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(empty_bgzf_block());
    data[0] = 42;
    Cursor::new(data)
}

pub fn single_block_missing_bgzf_identifier_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(empty_bgzf_block());
    data[12] = 21;
    Cursor::new(data)
}

pub fn two_blocks_missing_empty_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(regular_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_bad_payload_crc32_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(regular_bgzf_block());
    data.extend(bad_payload_crc32_bgzf_block());
    data.extend(regular_bgzf_block());
    data.extend(empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_bad_payload_size_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(regular_bgzf_block());
    data.extend(bad_payload_size_bgzf_block());
    data.extend(regular_bgzf_block());
    data.extend(empty_bgzf_block());
    Cursor::new(data)
}
