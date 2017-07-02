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

fn bgzf_block(deflated_payload: Vec<u8>, inflated_payload_size: u32, inflated_payload_crc32: u32, bgzf_size_delta: i32, extra_subfields_before_bgzf: Option<Vec<u8>>, extra_subfields_after_bgzf: Option<Vec<u8>>) -> Vec<u8> {
    let mut data = vec![
        0x1f, 0x8b,             // gzip identifier
        0x08,                   // method (deflate)
        0x04,                   // flags (FEXTRA)
        0x00, 0x00, 0x00, 0x00, // modification time
        0x00,                   // extra flags
        0xff,                   // operating system (unknown)
    ];

    let bgzf_extra_subfield_header = vec![
        0x42, 0x43,             // bgzf identifier
        0x02, 0x00,             // extra subfield length (2 bytes)
    ];

    let mut extra_field_size = 6u16;

    let extra_field_before =  match extra_subfields_before_bgzf {
        None => vec![],
        Some(extra_subfields) => {
            extra_field_size += extra_subfields.len() as u16;
            extra_subfields
        }
    };

    let extra_field_after = match extra_subfields_after_bgzf {
        None => vec![],
        Some(extra_subfields) => {
            extra_field_size += extra_subfields.len() as u16;
            extra_subfields
        }
    };

    data.write_u16::<LittleEndian>(extra_field_size).unwrap();
    data.extend(&extra_field_before);
    data.extend(&bgzf_extra_subfield_header);
    data.write_u16::<LittleEndian>((19i32 + extra_field_size as i32 + deflated_payload.len() as i32 + bgzf_size_delta) as u16).unwrap(); // bgzf block size, minus 1
    data.extend(&extra_field_after);

    data.extend(&deflated_payload);
    data.write_u32::<LittleEndian>(inflated_payload_crc32).unwrap();
    data.write_u32::<LittleEndian>(inflated_payload_size).unwrap();

    data
}

fn gzip_extra_subfields() -> Vec<u8> {
    let extra_subfield = vec![
        0x42, 0x21,             // arbitrary unknown identifier
        0x07, 0x00,             // arbitrary extra subfield length (7 bytes)
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x7
    ];

    let mut data = vec![];
    data.extend(&extra_subfield);
    data.extend(&extra_subfield);
    data.extend(&extra_subfield);

    data
}

pub fn empty_bgzf_block() -> Vec<u8> {
    bgzf_block(vec![
        0x03, 0x00 // deflated empty string
    ], 0, 0, 0, None, None)
}

pub fn empty_bgzf_block_with_extra_subfields_before() -> Vec<u8> {
    bgzf_block(vec![
        0x03, 0x00 // deflated empty string
    ], 0, 0, 0, Some(gzip_extra_subfields()), None)
}

pub fn empty_bgzf_block_with_extra_subfields_after() -> Vec<u8> {
    bgzf_block(vec![
        0x03, 0x00 // deflated empty string
    ], 0, 0, 0, None, Some(gzip_extra_subfields()))
}

pub fn empty_bgzf_block_with_extra_subfields_before_and_after() -> Vec<u8> {
    bgzf_block(vec![
        0x03, 0x00 // deflated empty string
    ], 0, 0, 0, Some(gzip_extra_subfields()), Some(gzip_extra_subfields()))
}

pub fn regular_bgzf_block() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 5, 907060870, 0, None, None)
}

pub fn regular_bgzf_block_with_extra_subfields_before() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 5, 907060870, 0, Some(gzip_extra_subfields()), None)
}

pub fn regular_bgzf_block_with_extra_subfields_after() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 5, 907060870, 0, None, Some(gzip_extra_subfields()))
}

pub fn regular_bgzf_block_with_extra_subfields_before_and_after() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 5, 907060870, 0, Some(gzip_extra_subfields()), Some(gzip_extra_subfields()))
}

pub fn bad_inflated_payload_crc32_bgzf_block() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0x25, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 5, 907060870, 0, None, None)
}

pub fn bad_inflated_payload_size_bgzf_block() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 25, 907060870, 0, None, None)
}

pub fn too_small_bgzf_size_bgzf_block() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 25, 907060870, -5i32, None, None)
}

pub fn too_large_bgzf_size_bgzf_block() -> Vec<u8> {
    bgzf_block(vec![
        0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00 // deflated "hello"
    ], 25, 907060870, -5i32, None, None)
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

pub fn empty_with_extra_subfields_before_bam() -> Cursor<Vec<u8>> {
    Cursor::new(empty_bgzf_block_with_extra_subfields_before())
}

pub fn empty_with_extra_subfields_after_bam() -> Cursor<Vec<u8>> {
    Cursor::new(empty_bgzf_block_with_extra_subfields_after())
}

pub fn empty_with_extra_subfields_before_and_after_bam() -> Cursor<Vec<u8>> {
    Cursor::new(empty_bgzf_block_with_extra_subfields_before_and_after())
}

pub fn single_block_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}

pub fn two_blocks_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&regular_bgzf_block());
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&regular_bgzf_block());
    data.extend(&regular_bgzf_block());
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_empty_inside_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&empty_bgzf_block());
    data.extend(&regular_bgzf_block());
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}

pub fn single_block_missing_empty_bam() -> Cursor<Vec<u8>> {
    Cursor::new(regular_bgzf_block())
}

pub fn single_block_missing_gzip_identifier_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&empty_bgzf_block());
    data[0] = 42;
    Cursor::new(data)
}

pub fn single_block_missing_bgzf_identifier_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&empty_bgzf_block());
    data[12] = 21;
    Cursor::new(data)
}

pub fn two_blocks_missing_empty_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&regular_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_bad_inflated_payload_crc32_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&bad_inflated_payload_crc32_bgzf_block());
    data.extend(&regular_bgzf_block());
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_bad_inflated_payload_size_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&bad_inflated_payload_size_bgzf_block());
    data.extend(&regular_bgzf_block());
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_too_small_bgzf_size_bam() -> Cursor<Vec<u8>> {
    let mut data = too_small_bgzf_size_bgzf_block();
    data.extend(&regular_bgzf_block());
    data.extend(&regular_bgzf_block());
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_too_large_bgzf_size_bam() -> Cursor<Vec<u8>> {
    let mut data = too_large_bgzf_size_bgzf_block();
    data.extend(&regular_bgzf_block());
    data.extend(&regular_bgzf_block());
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_with_extra_subfields_before_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&regular_bgzf_block_with_extra_subfields_before());
    data.extend(&regular_bgzf_block());
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_with_extra_subfields_after_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&regular_bgzf_block_with_extra_subfields_after());
    data.extend(&regular_bgzf_block());
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}

pub fn three_blocks_with_extra_subfields_before_and_after_bam() -> Cursor<Vec<u8>> {
    let mut data = regular_bgzf_block();
    data.extend(&regular_bgzf_block_with_extra_subfields_before_and_after());
    data.extend(&regular_bgzf_block());
    data.extend(&empty_bgzf_block());
    Cursor::new(data)
}
