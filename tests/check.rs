extern crate byteorder;
#[macro_use]
extern crate slog;

extern crate bamrescue;

mod common;

fn check(reader: &mut bamrescue::Rescuable) {
    match bamrescue::check(reader, false, &common::null_logger()) {
        Ok(()) => (),
        Err(error) => panic!(error),
    }
}

#[test]
fn empty_file() {
    check(&mut common::empty_file())
}

#[test]
fn empty_bam() {
    check(&mut common::empty_bam())
}

#[test]
fn single_block_bam() {
    check(&mut common::single_block_bam())
}

#[test]
fn two_blocks_bam() {
    check(&mut common::two_blocks_bam())
}

#[test]
fn three_blocks_bam() {
    check(&mut common::three_blocks_bam())
}

#[test]
fn three_blocks_empty_inside_bam() {
    check(&mut common::three_blocks_empty_inside_bam())
}

#[test]
#[should_panic]
fn single_block_missing_gzip_identifier() {
    check(&mut common::single_block_missing_gzip_identifier_bam())
}

#[test]
#[should_panic]
fn single_block_missing_bgzf_identifier() {
    check(&mut common::single_block_missing_bgzf_identifier_bam())
}

#[test]
#[should_panic]
fn single_block_missing_empty_bam() {
    check(&mut common::single_block_missing_empty_bam())
}

#[test]
#[should_panic]
fn two_blocks_missing_empty_bam() {
    check(&mut common::two_blocks_missing_empty_bam())
}

#[test]
#[should_panic]
fn three_blocks_bad_inflated_payload_crc32_bam() {
    check(&mut common::three_blocks_bad_inflated_payload_crc32_bam())
}

#[test]
#[should_panic]
fn three_blocks_bad_inflated_payload_size_bam() {
    check(&mut common::three_blocks_bad_inflated_payload_size_bam())
}

#[test]
#[should_panic]
fn three_blocks_too_small_deflated_payload_size_bam() {
    check(&mut common::three_blocks_too_small_deflated_payload_size_bam())
}

#[test]
#[should_panic]
fn three_blocks_too_large_deflated_payload_size_bam() {
    check(&mut common::three_blocks_too_large_deflated_payload_size_bam())
}
