extern crate byteorder;
#[macro_use]
extern crate slog;

extern crate bamrescue;

mod common;

use std::io::SeekFrom;

// TODO FIXME: add a Write output and check that a properly written bam is returned
// (consider doing this manually *and* by calling bamrescue::check() on the result)

fn rescue(reader: &mut bamrescue::Rescuable, blocks_count: u64, bad_blocks_count: u64, truncated_in_block: bool, truncated_between_blocks: bool) {
    let mut writer = vec![];

    {
        let results = bamrescue::rescue(reader, &mut writer, 1, &common::null_logger());
        assert_eq!(results.blocks_count, blocks_count);
        assert_eq!(results.bad_blocks_count, bad_blocks_count);
        assert_eq!(results.truncated_in_block, truncated_in_block);
        assert_eq!(results.truncated_between_blocks, truncated_between_blocks);
        // TODO FIXME check rescued_bytes
    }
    reader.seek(SeekFrom::Start(0)).unwrap();
    {
        let results = bamrescue::rescue(reader, &mut writer, 4, &common::null_logger());
        assert_eq!(results.blocks_count, blocks_count);
        assert_eq!(results.bad_blocks_count, bad_blocks_count);
        assert_eq!(results.truncated_in_block, truncated_in_block);
        assert_eq!(results.truncated_between_blocks, truncated_between_blocks);
        // TODO FIXME check rescued_bytes
    }
}

#[test]
fn empty_file() {
    rescue(&mut common::empty_file(), 0, 0, false, false)
}

#[test]
fn empty_bam() {
    rescue(&mut common::empty_bam(), 1, 0, false, false)
}

#[test]
fn empty_with_extra_subfields_before_bam() {
    rescue(&mut common::empty_with_extra_subfields_before_bam(), 1, 0, false, false)
}

#[test]
fn empty_with_extra_subfields_after_bam() {
    rescue(&mut common::empty_with_extra_subfields_after_bam(), 1, 0, false, false)
}

#[test]
fn empty_with_extra_subfields_before_and_after_bam() {
    rescue(&mut common::empty_with_extra_subfields_before_and_after_bam(), 1, 0, false, false)
}

#[test]
fn empty_with_extra_similar_subfields_before_bam() {
    rescue(&mut common::empty_with_extra_similar_subfields_before_bam(), 1, 0, false, false)
}

#[test]
fn empty_with_extra_similar_subfields_after_bam() {
    rescue(&mut common::empty_with_extra_similar_subfields_after_bam(), 1, 0, false, false)
}

#[test]
fn empty_with_extra_similar_subfields_before_and_after_bam() {
    rescue(&mut common::empty_with_extra_similar_subfields_before_and_after_bam(), 1, 0, false, false)
}

#[test]
fn single_block_bam() {
    rescue(&mut common::single_block_bam(), 2, 0, false, false)
}

#[test]
fn two_blocks_bam() {
    rescue(&mut common::two_blocks_bam(), 3, 0, false, false)
}

#[test]
fn three_blocks_bam() {
    rescue(&mut common::three_blocks_bam(), 4, 0, false, false)
}

#[test]
fn three_blocks_empty_inside_bam() {
    rescue(&mut common::three_blocks_empty_inside_bam(), 4, 0, false, false)
}

#[test]
fn single_block_missing_gzip_identifier() {
    rescue(&mut common::single_block_missing_gzip_identifier_bam(), 2, 1, false, false)
}

#[test]
fn single_block_missing_bgzf_identifier() {
    rescue(&mut common::single_block_missing_bgzf_identifier_bam(), 2, 1, false, false)
}

#[test]
fn single_block_missing_empty_bam() {
    rescue(&mut common::single_block_missing_empty_bam(), 1, 0, false, true)
}

#[test]
fn two_blocks_missing_empty_bam() {
    rescue(&mut common::two_blocks_missing_empty_bam(), 2, 0, false, true)
}

#[test]
fn three_blocks_bad_inflated_payload_crc32_bam() {
    rescue(&mut common::three_blocks_bad_inflated_payload_crc32_bam(), 4, 1, false, false)
}

#[test]
fn three_blocks_bad_inflated_payload_size_bam() {
    rescue(&mut common::three_blocks_bad_inflated_payload_size_bam(), 4, 1, false, false)
}

// TODO same tests as the two following ones, but with blocks of len >> 65536,
// including with a header over a block boundary to rescue that the loop works
// properly

#[test]
fn three_blocks_too_small_bgzf_size_bam() {
    rescue(&mut common::three_blocks_too_small_bgzf_size_bam(), 4, 1, false, false)
}

#[test]
fn three_blocks_too_large_bgzf_size_bam() {
    rescue(&mut common::three_blocks_too_large_bgzf_size_bam(), 4, 1, false, false)
}

#[test]
fn three_blocks_with_extra_subfields_before_bam() {
    rescue(&mut common::three_blocks_with_extra_subfields_before_bam(), 4, 0, false, false)
}

#[test]
fn three_blocks_with_extra_subfields_after_bam() {
    rescue(&mut common::three_blocks_with_extra_subfields_after_bam(), 4, 0, false, false)
}

#[test]
fn three_blocks_with_extra_subfields_before_and_after_bam() {
    rescue(&mut common::three_blocks_with_extra_subfields_before_and_after_bam(), 4, 0, false, false)
}

#[test]
fn three_blocks_with_extra_similar_subfields_before_bam() {
    rescue(&mut common::three_blocks_with_extra_similar_subfields_before_bam(), 4, 0, false, false)
}

#[test]
fn three_blocks_with_extra_similar_subfields_after_bam() {
    rescue(&mut common::three_blocks_with_extra_similar_subfields_after_bam(), 4, 0, false, false)
}

#[test]
fn three_blocks_with_extra_similar_subfields_before_and_after_bam() {
    rescue(&mut common::three_blocks_with_extra_similar_subfields_before_and_after_bam(), 4, 0, false, false)
}
