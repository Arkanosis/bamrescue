mod common;

use std::io::{
    Cursor,
    SeekFrom
};

fn rescue(reader: &mut dyn bamrescue::Rescuable, blocks_count: u64, bad_blocks_count: u64, truncated_in_block: bool, truncated_between_blocks: bool, rescued_bytes: Vec<u8>) {
    let mut writer = vec![];
    {
        let results = bamrescue::rescue(reader, &mut writer, 1, &mut None);
        assert_eq!(results.blocks_count, blocks_count);
        assert_eq!(results.bad_blocks_count, bad_blocks_count);
        assert_eq!(results.truncated_in_block, truncated_in_block);
        assert_eq!(results.truncated_between_blocks, truncated_between_blocks);
        assert_eq!(writer, rescued_bytes);
        let results = bamrescue::check(&mut Cursor::new(writer), true, 4, &mut None);
        assert_eq!(results.bad_blocks_count, 0);
        assert_eq!(results.truncated_in_block, false);
        assert_eq!(results.truncated_between_blocks, false);
    }
    reader.seek(SeekFrom::Start(0)).unwrap();
    writer = vec![];
    {
        let results = bamrescue::rescue(reader, &mut writer, 4, &mut None);
        assert_eq!(results.blocks_count, blocks_count);
        assert_eq!(results.bad_blocks_count, bad_blocks_count);
        assert_eq!(results.truncated_in_block, truncated_in_block);
        assert_eq!(results.truncated_between_blocks, truncated_between_blocks);
        assert_eq!(writer, rescued_bytes);
        let results = bamrescue::check(&mut Cursor::new(writer), true, 4, &mut None);
        assert_eq!(results.bad_blocks_count, 0);
        assert_eq!(results.truncated_in_block, false);
        assert_eq!(results.truncated_between_blocks, false);
    }
}

#[test]
fn empty_file() {
    rescue(&mut common::empty_file(), 0, 0, false, false, vec![])
}

#[test]
fn empty_bam() {
    rescue(&mut common::empty_bam(), 1, 0, false, false, common::empty_bgzf_block())
}

#[test]
fn empty_with_extra_subfields_before_bam() {
    rescue(&mut common::empty_with_extra_subfields_before_bam(), 1, 0, false, false, common::empty_bgzf_block_with_extra_subfields_before())
}

#[test]
fn empty_with_extra_subfields_after_bam() {
    rescue(&mut common::empty_with_extra_subfields_after_bam(), 1, 0, false, false, common::empty_bgzf_block_with_extra_subfields_after())
}

#[test]
fn empty_with_extra_subfields_before_and_after_bam() {
    rescue(&mut common::empty_with_extra_subfields_before_and_after_bam(), 1, 0, false, false, common::empty_bgzf_block_with_extra_subfields_before_and_after())
}

#[test]
fn empty_with_extra_similar_subfields_before_bam() {
    rescue(&mut common::empty_with_extra_similar_subfields_before_bam(), 1, 0, false, false, common::empty_bgzf_block_with_extra_similar_subfields_before())
}

#[test]
fn empty_with_extra_similar_subfields_after_bam() {
    rescue(&mut common::empty_with_extra_similar_subfields_after_bam(), 1, 0, false, false, common::empty_bgzf_block_with_extra_similar_subfields_after())
}

#[test]
fn empty_with_extra_similar_subfields_before_and_after_bam() {
    rescue(&mut common::empty_with_extra_similar_subfields_before_and_after_bam(), 1, 0, false, false, common::empty_bgzf_block_with_extra_similar_subfields_before_and_after())
}

#[test]
fn single_block_bam() {
    rescue(&mut common::single_block_bam(), 2, 0, false, false, common::single_block())
}

#[test]
fn two_blocks_bam() {
    rescue(&mut common::two_blocks_bam(), 3, 0, false, false, common::two_blocks())
}

#[test]
fn three_blocks_bam() {
    rescue(&mut common::three_blocks_bam(), 4, 0, false, false, common::three_blocks())
}

#[test]
fn three_blocks_empty_inside_bam() {
    rescue(&mut common::three_blocks_empty_inside_bam(), 4, 0, false, false, common::three_blocks_empty_inside())
}

#[test]
fn single_block_missing_gzip_identifier() {
    rescue(&mut common::single_block_missing_gzip_identifier_bam(), 2, 1, false, false, common::empty_bgzf_block())
}

#[test]
fn single_block_missing_bgzf_identifier() {
    rescue(&mut common::single_block_missing_bgzf_identifier_bam(), 2, 1, false, false, common::empty_bgzf_block())
}

#[test]
fn single_block_missing_empty_bam() {
    rescue(&mut common::single_block_missing_empty_bam(), 1, 0, false, true, common::single_block())
}

#[test]
fn two_blocks_missing_empty_bam() {
    rescue(&mut common::two_blocks_missing_empty_bam(), 2, 0, false, true, common::two_blocks())
}

#[test]
fn three_blocks_bad_inflated_payload_crc32_bam() {
    rescue(&mut common::three_blocks_bad_inflated_payload_crc32_bam(), 4, 1, false, false, common::two_blocks())
}

#[test]
fn three_blocks_bad_inflated_payload_size_bam() {
    rescue(&mut common::three_blocks_bad_inflated_payload_size_bam(), 4, 1, false, false, common::two_blocks())
}

// TODO same tests as the two following ones, but with blocks of len >> 65536,
// including with a header over a block boundary to rescue that the loop works
// properly

#[test]
fn three_blocks_too_small_bgzf_size_bam() {
    rescue(&mut common::three_blocks_too_small_bgzf_size_bam(), 4, 1, false, false, common::two_blocks())
}

#[test]
fn three_blocks_too_large_bgzf_size_bam() {
    rescue(&mut common::three_blocks_too_large_bgzf_size_bam(), 4, 1, false, false, common::two_blocks())
}

#[test]
fn three_blocks_with_extra_subfields_before_bam() {
    rescue(&mut common::three_blocks_with_extra_subfields_before_bam(), 4, 0, false, false, common::three_blocks_with_extra_subfields_before())
}

#[test]
fn three_blocks_with_extra_subfields_after_bam() {
    rescue(&mut common::three_blocks_with_extra_subfields_after_bam(), 4, 0, false, false, common::three_blocks_with_extra_subfields_after())
}

#[test]
fn three_blocks_with_extra_subfields_before_and_after_bam() {
    rescue(&mut common::three_blocks_with_extra_subfields_before_and_after_bam(), 4, 0, false, false, common::three_blocks_with_extra_subfields_before_and_after())
}

#[test]
fn three_blocks_with_extra_similar_subfields_before_bam() {
    rescue(&mut common::three_blocks_with_extra_similar_subfields_before_bam(), 4, 0, false, false, common::three_blocks_with_extra_similar_subfields_before())
}

#[test]
fn three_blocks_with_extra_similar_subfields_after_bam() {
    rescue(&mut common::three_blocks_with_extra_similar_subfields_after_bam(), 4, 0, false, false, common::three_blocks_with_extra_similar_subfields_after())
}

#[test]
fn three_blocks_with_extra_similar_subfields_before_and_after_bam() {
    rescue(&mut common::three_blocks_with_extra_similar_subfields_before_and_after_bam(), 4, 0, false, false, common::three_blocks_with_extra_similar_subfields_before_and_after())
}
