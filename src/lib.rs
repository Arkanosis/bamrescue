extern crate byteorder;
extern crate crc;
extern crate futures;
extern crate futures_cpupool;
extern crate inflate;
#[macro_use]
extern crate slog;

use byteorder::ReadBytesExt;

use crc::crc32::Hasher32;

use futures::Future;

use std::collections::VecDeque;

use std::io::{
    BufRead,
    Error,
    Read,
    Seek,
    SeekFrom,
    Write
};

use std::str;

// 100 blocks of 64 kiB, even accounting for a huge overhead,
// is still less than 10 MiB, which is trivially manageable.
// Additionally, there's no chance that 100 threads or more
// give any speedup inflating blocks of at most 64 kiB.
const MAX_FUTURES: usize = 100;

const GZIP_IDENTIFIER: [u8; 2] = [0x1f, 0x8b];
const BGZF_IDENTIFIER: [u8; 2] = [0x42, 0x43];

const DEFLATE: u8 = 8;

const FEXTRA: u8 = 1 << 2;

pub fn version() -> &'static str {
    return option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");
}

pub trait Rescuable: BufRead + Seek {}
impl<T: BufRead + Seek> Rescuable for T {}

struct BGZFBlock {
    header_bytes: Vec<u8>,
    deflated_payload_bytes: Vec<u8>,
    inflated_payload_crc32: u32,
    inflated_payload_size: u32,
}

struct BGZFBlockStatus {
    corrupted: bool,
    inflated_payload_size: u32,
}

pub struct Results {
    pub blocks_count: u64,
    pub blocks_size: u64,
    pub bad_blocks_count: u64,
    pub bad_blocks_size: u64,
    pub truncated_in_block: bool,
    pub truncated_between_blocks: bool,
}

fn check_payload(block: &Option<BGZFBlock>) -> Result<BGZFBlockStatus, Error> {
    match *block {
        None => Ok(BGZFBlockStatus {
            corrupted: false,
            inflated_payload_size: 0,
        }),
        Some(ref block) => {
            let inflated_payload_bytes = match inflate::inflate_bytes(&block.deflated_payload_bytes) {
                Ok(inflated_payload_bytes) => inflated_payload_bytes,
                Err(_) => return Ok(BGZFBlockStatus {
                    corrupted: true,
                    inflated_payload_size: block.inflated_payload_size,
                }),
            };

            let mut inflated_payload_digest = crc::crc32::Digest::new(crc::crc32::IEEE);
            inflated_payload_digest.write(&inflated_payload_bytes);
            let inflated_payload_crc32 = inflated_payload_digest.sum32();
            if inflated_payload_crc32 != block.inflated_payload_crc32 {
                return Ok(BGZFBlockStatus {
                    corrupted: true,
                    inflated_payload_size: block.inflated_payload_size,
                });
            }

            let inflated_payload_size = inflated_payload_bytes.len() as u32;
            if inflated_payload_size != block.inflated_payload_size {
                // TODO recoverable (wrong size is not a big issue if the CRC32 is correct)
                return Ok(BGZFBlockStatus {
                    corrupted: true,
                    inflated_payload_size: block.inflated_payload_size,
                });
            }

            Ok(BGZFBlockStatus {
                corrupted: false,
                inflated_payload_size: block.inflated_payload_size,
            })
        }
    }
}

macro_rules! fail {
    ($fail_fast: expr, $results: expr, $truncated_in_block: expr) => {
        $results.bad_blocks_count += 1;
        if $truncated_in_block {
            $results.truncated_in_block = true;
        }
        if $fail_fast {
            return $results;
        }
    }
}

pub fn check(reader: &mut Rescuable, fail_fast: bool, threads: usize, logger: &slog::Logger) -> Results {
    info!(logger, "Checking integrity…");

    let mut results = Results {
        blocks_count: 0u64,
        blocks_size: 0u64,
        bad_blocks_count: 0u64,
        bad_blocks_size: 0u64,
        truncated_in_block: false,
        truncated_between_blocks: false,
    };

    let pool;
    if threads == 0 {
        pool = futures_cpupool::CpuPool::new_num_cpus();
    } else {
        pool = futures_cpupool::CpuPool::new(threads);
    }

    let mut payload_status_futures = VecDeque::<futures_cpupool::CpuFuture<BGZFBlockStatus, Error>>::with_capacity(MAX_FUTURES);

    let mut previous_block: Option<BGZFBlock> = None;
    loop {
        if payload_status_futures.len() == MAX_FUTURES {
            let payload_status = payload_status_futures.pop_front().unwrap().wait().unwrap();
            if payload_status.corrupted {
                results.bad_blocks_size += payload_status.inflated_payload_size as u64;
                fail!(fail_fast, results, false);
            }
        }

        let mut header_bytes = vec![];
        {
            let mut header_reader = reader.take(12);
            match header_reader.read_to_end(&mut header_bytes) {
                Ok(header_size) => {
                    if header_size == 0 {
                        break;
                    }

                    if header_size < 12 {
                        fail!(fail_fast, results, true);
                        break;
                    }
                },
                Err(_) => {
                    fail!(fail_fast, results, true);
                    break;
                }
            }
        }

        let mut correct_bytes = 0;
        if header_bytes[0] == GZIP_IDENTIFIER[0] {
            correct_bytes += 1;
        }
        if header_bytes[1] == GZIP_IDENTIFIER[1] {
            correct_bytes += 1;
        }
        if header_bytes[2] == DEFLATE {
            correct_bytes += 1;
        }
        if header_bytes[3] == FEXTRA {
            correct_bytes += 1;
        }

        if correct_bytes < 4 {
            fail!(fail_fast, results, false);
            if correct_bytes == 3 {
                // single corrupted byte, can probably deal with it in place
                // TODO fix the four bytes for rescue
            } else {
                // mutliple corrupted bytes, safer to jump to the next block
                // TODO FIXME check the next bytes, and if not correct, jump to the next block
                panic!("Unexpected byte while checking header of block {}", results.blocks_count);
            }
        }

        // header_bytes[4..8] => modification time; can be anything
        // header_bytes[8] => extra flags; can be anything
        // header_bytes[9] => operating system; can be anything

        let extra_field_size = (&mut &header_bytes[10..12]).read_u16::<byteorder::LittleEndian>().unwrap();

        // TODO add the next extra_field_size bytes to header_bytes for rescue

        let mut bgzf_block_size = 0u16;

        let mut remaining_extra_field_size = extra_field_size;
        while remaining_extra_field_size > 4 {
            let mut extra_subfield_identifier = [0u8; 2];
            match reader.read_exact(&mut extra_subfield_identifier) {
                Ok(_) => (),
                Err(_) => {
                    fail!(fail_fast, results, true);
                    break;
                }
            }

            let extra_subfield_size = match reader.read_u16::<byteorder::LittleEndian>() {
                Ok(extra_subfield_size) => extra_subfield_size,
                Err(_) => {
                    fail!(fail_fast, results, true);
                    break;
                }
            };

            let mut correct_bytes = 0;
            if extra_subfield_identifier[0] == BGZF_IDENTIFIER[0] {
                correct_bytes += 1;
            }
            if extra_subfield_identifier[1] == BGZF_IDENTIFIER[1] {
                correct_bytes += 1;
            }
            if extra_subfield_size & 0xff == 2 {
                correct_bytes += 1;
            }
            if extra_subfield_size & 0xff00 == 0 {
                correct_bytes += 1;
            }

            if correct_bytes == 4 ||
               (correct_bytes == 3 &&
                extra_field_size == 6) {
                if correct_bytes != 4 {
                    fail!(fail_fast, results, false);
                    // single corrupted byte, but most likely at the right place anyway
                    // TODO fix the four bytes for rescue
                }
                bgzf_block_size = match reader.read_u16::<byteorder::LittleEndian>() {
                    Ok(bgzf_block_size) => bgzf_block_size + 1,
                    Err(_) => {
                        fail!(fail_fast, results, true);
                        break;
                    }
                };
            } else {
                match reader.seek(SeekFrom::Current(extra_subfield_size as i64)) {
                    Ok(_) => (),
                    Err(_) => {
                        fail!(fail_fast, results, true);
                        break;
                    }
                }
            }

            remaining_extra_field_size -= 4 + extra_subfield_size;
        }

        if remaining_extra_field_size != 0 {
            fail!(fail_fast, results, false);
            // TODO FIXME check the next bytes, and if not correct, jump to the next block
            panic!("Unexpected byte while checking header of block {}", results.blocks_count);
        }

        if bgzf_block_size == 0u16 {
            fail!(fail_fast, results, false);
            // TODO FIXME check the next bytes, and if not correct, jump to the next block
            panic!("Unexpected byte while checking header of block {}", results.blocks_count);
        }

        // TODO if not at the right position for the next header, fix the previous header / payload or
        // the current header, seek to the right position and “continue”

        if threads == 1 {
            let payload_status = check_payload(&previous_block).unwrap();
            if payload_status.corrupted {
                results.bad_blocks_size += payload_status.inflated_payload_size as u64;
                fail!(fail_fast, results, false);
            }
        } else {
            let payload_status_future = pool.spawn_fn(move || {
                check_payload(&previous_block)
            });
            payload_status_futures.push_back(payload_status_future);
            previous_block = None;
        }

        if bgzf_block_size == 0 {
            bgzf_block_size = match reader.read_u16::<byteorder::LittleEndian>() {
                Ok(bgzf_block_size) => bgzf_block_size + 1,
                Err(_) => {
                    fail!(fail_fast, results, true);
                    break;
                }
            };
        }

        let mut deflated_payload_bytes = vec![];
        {
            let deflated_payload_size = bgzf_block_size - 20u16 - extra_field_size;
            let mut deflated_payload_reader = reader.take(deflated_payload_size as u64);
            match deflated_payload_reader.read_to_end(&mut deflated_payload_bytes) {
                Ok(deflated_payload_read_size) => {
                    if deflated_payload_read_size < deflated_payload_size as usize {
                        fail!(fail_fast, results, true);
                        break;
                    }
                },
                Err(_) => {
                    fail!(fail_fast, results, true);
                    break;
                }
            }
        }

        let inflated_payload_crc32 = match reader.read_u32::<byteorder::LittleEndian>() {
            Ok(inflated_payload_crc32) => inflated_payload_crc32,
            Err(_) => {
                fail!(fail_fast, results, true);
                break;
            }
        };
        let inflated_payload_size = match reader.read_u32::<byteorder::LittleEndian>() {
            Ok(inflated_payload_size) => inflated_payload_size,
            Err(_) => {
                fail!(fail_fast, results, true);
                break;
            }
        };

        previous_block = Some(BGZFBlock {
            header_bytes: header_bytes,
            deflated_payload_bytes: deflated_payload_bytes,
            inflated_payload_crc32: inflated_payload_crc32,
            inflated_payload_size: inflated_payload_size,
        });

        results.blocks_count += 1;
        results.blocks_size += inflated_payload_size as u64;
    }

    let mut last_inflated_payload_size = 0u32;
    if threads == 1 {
        let payload_status = check_payload(&previous_block).unwrap();
        if payload_status.corrupted {
            results.bad_blocks_size += payload_status.inflated_payload_size as u64;
            fail!(fail_fast, results, false);
        }
        last_inflated_payload_size = payload_status.inflated_payload_size;
    } else {
        let payload_status_future = pool.spawn_fn(move || {
            check_payload(&previous_block)
        });
        payload_status_futures.push_back(payload_status_future);
        for payload_status_future in payload_status_futures.iter_mut() {
            let payload_status = payload_status_future.wait().unwrap();
            if payload_status.corrupted {
                results.bad_blocks_size += payload_status.inflated_payload_size as u64;
                fail!(fail_fast, results, false);
            }
            last_inflated_payload_size = payload_status.inflated_payload_size;
        }
    }
    if last_inflated_payload_size != 0 {
        results.truncated_between_blocks = true;
        if fail_fast {
            return results;
        }
    }

    results
}

pub fn rescue(reader: &mut Rescuable, writer: &mut Write, logger: &slog::Logger) -> Result<(), Error> {
    info!(logger, "Rescuing file…");

    error!(logger, "bamrescue::rescue() is not yet implemented");
    unimplemented!();
}
