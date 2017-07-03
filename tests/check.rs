extern crate byteorder;
#[macro_use]
extern crate slog;

extern crate bamrescue;

mod common;

fn check(reader: &mut bamrescue::Rescuable, blocks_count: u64, bad_blocks_count: u64, truncated_in_block: bool, truncated_between_blocks: bool) {
    let results = bamrescue::check(reader, false, &common::null_logger());
    assert_eq!(results.blocks_count, blocks_count);
    assert_eq!(results.bad_blocks_count, bad_blocks_count);
    assert_eq!(results.truncated_in_block, truncated_in_block);
    assert_eq!(results.truncated_between_blocks, truncated_between_blocks);

}

#[test]
fn empty_file() {
    check(&mut common::empty_file(), 0, 0, false, false)
}

#[test]
fn empty_bam() {
    check(&mut common::empty_bam(), 1, 0, false, false)
}

#[test]
fn empty_with_extra_subfields_before_bam() {
    check(&mut common::empty_with_extra_subfields_before_bam(), 1, 0, false, false)
}

#[test]
fn empty_with_extra_subfields_after_bam() {
    check(&mut common::empty_with_extra_subfields_after_bam(), 1, 0, false, false)
}

#[test]
fn empty_with_extra_subfields_before_and_after_bam() {
    check(&mut common::empty_with_extra_subfields_before_and_after_bam(), 1, 0, false, false)
}

#[test]
fn single_block_bam() {
    check(&mut common::single_block_bam(), 2, 0, false, false)
}

#[test]
fn two_blocks_bam() {
    check(&mut common::two_blocks_bam(), 3, 0, false, false)
}

#[test]
fn three_blocks_bam() {
    check(&mut common::three_blocks_bam(), 4, 0, false, false)
}

#[test]
fn three_blocks_empty_inside_bam() {
    check(&mut common::three_blocks_empty_inside_bam(), 4, 0, false, false)
}

#[test]
#[should_panic]
fn single_block_missing_gzip_identifier() {
    check(&mut common::single_block_missing_gzip_identifier_bam(), 2, 1, false, false)
}

#[test]
#[should_panic]
fn single_block_missing_bgzf_identifier() {
    check(&mut common::single_block_missing_bgzf_identifier_bam(), 2, 1, false, false)
}

#[test]
fn single_block_missing_empty_bam() {
    check(&mut common::single_block_missing_empty_bam(), 1, 0, false, true)
}

#[test]
fn two_blocks_missing_empty_bam() {
    check(&mut common::two_blocks_missing_empty_bam(), 2, 0, false, true)
}

#[test]
fn three_blocks_bad_inflated_payload_crc32_bam() {
    check(&mut common::three_blocks_bad_inflated_payload_crc32_bam(), 4, 1, false, false)
}

#[test]
fn three_blocks_bad_inflated_payload_size_bam() {
    check(&mut common::three_blocks_bad_inflated_payload_size_bam(), 4, 1, false, false)
}

#[test]
#[should_panic]
fn three_blocks_too_small_bgzf_size_bam() {
    check(&mut common::three_blocks_too_small_bgzf_size_bam(), 4, 1, false, false)
}

#[test]
#[should_panic]
fn three_blocks_too_large_bgzf_size_bam() {
    check(&mut common::three_blocks_too_large_bgzf_size_bam(), 4, 1, false, false)
}

#[test]
fn three_blocks_with_extra_subfields_before_bam() {
    check(&mut common::three_blocks_with_extra_subfields_before_bam(), 4, 0, false, false)
}

#[test]
fn three_blocks_with_extra_subfields_after_bam() {
    check(&mut common::three_blocks_with_extra_subfields_after_bam(), 4, 0, false, false)
}

#[test]
fn three_blocks_with_extra_subfields_before_and_after_bam() {
    check(&mut common::three_blocks_with_extra_subfields_before_and_after_bam(), 4, 0, false, false)
}
