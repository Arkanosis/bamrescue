use byteorder:: {
    ReadBytesExt,
    WriteBytesExt,
};

use crc::{
    Crc,
    CRC_32_ISO_HDLC,
};

use futures::Future;

use std::collections::VecDeque;

use std::{
    io::{
        BufRead,
        Error,
        Read,
        Seek,
        SeekFrom,
        Write,
    },
    str,
};

// 100 blocks of 64 kiB, even accounting for a huge overhead,
// is still less than 10 MiB, which is trivially manageable.
// Additionally, there's no chance that 100 threads or more
// give any speedup inflating blocks of at most 64 kiB.
const MAX_FUTURES: usize = 100;

const BUFFER_SIZE: u64 = 65536;

const GZIP_IDENTIFIER: [u8; 2] = [0x1f, 0x8b];
const BGZF_IDENTIFIER: [u8; 2] = [0x42, 0x43];

const DEFLATE: u8 = 8;

const FEXTRA: u8 = 1 << 2;

const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

pub fn version() -> &'static str {
    return option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");
}

pub trait Rescuable: BufRead + Seek {}
impl<T: BufRead + Seek> Rescuable for T {}

pub trait ListenProgress {
    fn on_new_target(&mut self, target: u64);
    fn on_progress(&mut self, progress: u64);
    fn on_bad_block(&mut self);
    fn on_finished(&mut self);
}

struct BGZFBlock {
    header_bytes: Vec<u8>,
    deflated_payload_bytes: Vec<u8>,
    inflated_payload_crc32: u32,
    inflated_payload_size: u32,
    corrupted: bool,
    end_position: u64,
}

struct BGZFBlockStatus {
    corrupted: bool,
    inflated_payload_size: u32,
    block: Option<BGZFBlock>,
}

pub struct Results {
    pub blocks_count: u64,
    pub blocks_size: u64,
    pub bad_blocks_count: u64,
    pub bad_blocks_size: u64,
    pub truncated_in_block: bool,
    pub truncated_between_blocks: bool,
}

fn seek_next_block(reader: &mut dyn Rescuable, block_position: u64) {
    let mut current_position = block_position;
    reader.seek(SeekFrom::Start(current_position)).unwrap();

    let mut bytes = vec![];

    'seek: loop {
        let mut buffer_reader = reader.take(BUFFER_SIZE);
        let buffer_size = buffer_reader.read_to_end(&mut bytes).unwrap();
        for window in bytes.windows(4) {
            let mut correct_bytes = 0;
            if window[0] == GZIP_IDENTIFIER[0] {
                correct_bytes += 1;
            }
            if window[1] == GZIP_IDENTIFIER[1] {
                correct_bytes += 1;
            }
            if window[2] == DEFLATE {
                correct_bytes += 1;
            }
            if window[3] == FEXTRA {
                correct_bytes += 1;
            }

            if correct_bytes >= 3 {
                break 'seek;
            }
            current_position += 1;
        }
        if buffer_size < BUFFER_SIZE as usize {
            return;
        }
        {
            let (beginning, end) = bytes.split_at_mut(4);
            beginning.copy_from_slice(&end[end.len() - 4..]);
        }
        bytes.resize(4, 0);
        current_position -= 4;
    }

    reader.seek(SeekFrom::Start(current_position)).unwrap();
}

fn process_payload(block: Option<BGZFBlock>) -> Result<BGZFBlockStatus, Error> {
    match block {
        None => Ok(BGZFBlockStatus {
            corrupted: false,
            inflated_payload_size: 0,
            block: None,
        }),
        Some(block) => {
            let inflated_payload_bytes = match inflate::inflate_bytes(&block.deflated_payload_bytes) {
                Ok(inflated_payload_bytes) => inflated_payload_bytes,
                Err(_) => return Ok(BGZFBlockStatus {
                    corrupted: true,
                    inflated_payload_size: block.inflated_payload_size,
                    block: None,
                }),
            };

            let mut inflated_payload_digest = CRC32.digest();
            inflated_payload_digest.update(&inflated_payload_bytes);
            let inflated_payload_crc32 = inflated_payload_digest.finalize();
            if inflated_payload_crc32 != block.inflated_payload_crc32 {
                return Ok(BGZFBlockStatus {
                    corrupted: true,
                    inflated_payload_size: block.inflated_payload_size,
                    block: None,
                });
            }

            let inflated_payload_size = inflated_payload_bytes.len() as u32;
            if inflated_payload_size != block.inflated_payload_size {
                // TODO recoverable (wrong size is not a big issue if the CRC32 is correct)
                return Ok(BGZFBlockStatus {
                    corrupted: true,
                    inflated_payload_size: block.inflated_payload_size,
                    block: None,
                });
            }

            Ok(BGZFBlockStatus {
                corrupted: block.corrupted,
                inflated_payload_size: block.inflated_payload_size,
                block: if block.corrupted {
                    None
                } else {
                    Some(block)
                }
            })
        }
    }
}

fn write_block(writer: &mut Option<&mut dyn Write>, block: &Option<BGZFBlock>)  {
    if let Some(ref mut writer) = writer {
        if let Some(block) = block {
            writer.write_all(&block.header_bytes).unwrap();
            writer.write_all(&block.deflated_payload_bytes).unwrap();
            writer.write_u32::<byteorder::LittleEndian>(block.inflated_payload_crc32).unwrap();
            writer.write_u32::<byteorder::LittleEndian>(block.inflated_payload_size).unwrap();
        }
    }
}

fn report_progress(progress_listener: &mut Option<&mut dyn ListenProgress>, block: &Option<BGZFBlock>)  {
    if let Some(ref mut progress_listener) = progress_listener {
        if let Some(block) = block {
            progress_listener.on_progress(block.end_position);
        }
    }
}

fn report_bad_block(results: &mut Results, progress_listener: &mut Option<&mut dyn ListenProgress>, payload_status: &BGZFBlockStatus)  {
    results.bad_blocks_count += 1;
    results.bad_blocks_size += payload_status.inflated_payload_size as u64;
    if let Some(ref mut progress_listener) = progress_listener {
        progress_listener.on_bad_block();
    }
}

macro_rules! fail {
    ($fail_fast: expr, $results: expr, $previous_block: expr, $previous_block_corrupted: expr, $current_block_corrupted_ref: expr, $current_block_corrupted: expr, $truncated_in_block: expr) => {
        match $previous_block {
            None => {
                $current_block_corrupted_ref |= $previous_block_corrupted;
            },
            Some(ref mut block) => {
                block.corrupted |= $previous_block_corrupted;
            }
        }
        $current_block_corrupted_ref |= $current_block_corrupted;
        assert!($current_block_corrupted_ref || true); // TODO workaround the "unused assignment warning"
        if $truncated_in_block {
            $results.truncated_in_block = true;
        }
        if $fail_fast {
            $results.bad_blocks_count += 1;
            return $results;
        }
    }
}

fn process(reader: &mut dyn Rescuable, mut writer: Option<&mut dyn Write>, fail_fast: bool, threads: usize, progress_listener: &mut Option<&mut dyn ListenProgress>) -> Results {
    let reader_size = reader.seek(SeekFrom::End(0)).unwrap();
    reader.seek(SeekFrom::Start(0)).unwrap();
    if let Some(ref mut progress_listener) = progress_listener {
        progress_listener.on_new_target(reader_size);
    }

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
    let mut previous_block_position;
    let mut current_block_position = 0u64;
    let mut current_block_corrupted = false;
    'blocks: loop {
        if payload_status_futures.len() == MAX_FUTURES {
            let payload_status = payload_status_futures.pop_front().unwrap().wait().unwrap();
            if payload_status.corrupted {
                report_bad_block(&mut results, progress_listener, &payload_status);
                fail!(fail_fast, results, previous_block, false, current_block_corrupted, false, false);
            } else {
                write_block(&mut writer, &payload_status.block);
            }
            report_progress(progress_listener, &payload_status.block);
        }

        previous_block_position = current_block_position;
        current_block_position = reader.seek(SeekFrom::Current(0i64)).unwrap();
        current_block_corrupted = false;

        let mut header_bytes = vec![];
        {
            let mut header_reader = reader.take(12);
            match header_reader.read_to_end(&mut header_bytes) {
                Ok(header_size) => {
                    if header_size == 0 {
                        break 'blocks;
                    }

                    if header_size < 12 {
                        fail!(fail_fast, results, previous_block, true, current_block_corrupted, false, true);
                        break 'blocks;
                    }
                },
                Err(_) => {
                    fail!(fail_fast, results, previous_block, true, current_block_corrupted, false, true);
                    break 'blocks;
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
            if correct_bytes == 3 {
                fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, false);
                // single corrupted byte, can probably deal with it in place
                // TODO fix the four bytes for rescue
            } else {
                fail!(fail_fast, results, previous_block, true, current_block_corrupted, false, false);
                // multiple corrupted bytes, safer to jump to the next block
                seek_next_block(reader, previous_block_position + 1);
                continue 'blocks;
            }
        }

        // header_bytes[4..8] => modification time; can be anything
        // header_bytes[8] => extra flags; can be anything
        // header_bytes[9] => operating system; can be anything

        let extra_field_size = (&mut &header_bytes[10..12]).read_u16::<byteorder::LittleEndian>().unwrap();

        if writer.is_some() {
            {
                let mut extra_field_reader = reader.take(extra_field_size as u64);
                match extra_field_reader.read_to_end(&mut header_bytes) {
                    Ok(extra_field_actual_size) => {
                        if extra_field_actual_size < extra_field_size as usize {
                            fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, true);
                            break 'blocks;
                        }
                    },
                    Err(_) => {
                        fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, true);
                        break 'blocks;
                    }
                }
            }

            // TODO potential optimization:
            // Read the extra subfields from header_bytes instead of from reader and don't seek back
            reader.seek(SeekFrom::Current(-(extra_field_size as i64))).unwrap();
        }

        let mut bgzf_block_size = 0u16;

        let mut remaining_extra_field_size = extra_field_size;
        while remaining_extra_field_size > 4 {
            let mut extra_subfield_identifier = [0u8; 2];
            if let Err(_) = reader.read_exact(&mut extra_subfield_identifier) {
                fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, true);
                break 'blocks;
            }

            let extra_subfield_size = match reader.read_u16::<byteorder::LittleEndian>() {
                Ok(extra_subfield_size) => extra_subfield_size,
                Err(_) => {
                    fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, true);
                    break 'blocks;
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

            if extra_subfield_size > remaining_extra_field_size - 4 {
                fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, false);
                seek_next_block(reader, current_block_position + 1);
                continue 'blocks;
            }

            if correct_bytes == 4 ||
               (correct_bytes == 3 &&
                extra_field_size == 6) {
                if correct_bytes != 4 {
                    fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, false);
                    // single corrupted byte, but most likely at the right place anyway
                    // TODO fix the four bytes for rescue
                }
                bgzf_block_size = match reader.read_u16::<byteorder::LittleEndian>() {
                    Ok(bgzf_block_size) => bgzf_block_size + 1,
                    Err(_) => {
                        fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, true);
                        break 'blocks;
                    }
                };
            } else if let Err(_) = reader.seek(SeekFrom::Current(extra_subfield_size as i64)) {
                fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, true);
                break 'blocks;
            }

            remaining_extra_field_size -= 4 + extra_subfield_size;
        }

        if remaining_extra_field_size != 0u16 {
            fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, false);
            seek_next_block(reader, current_block_position + 1);
            continue 'blocks;
        }

        if bgzf_block_size == 0u16 {
            fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, false);
            seek_next_block(reader, current_block_position + 1);
            continue 'blocks;
        }

        if threads == 1 {
            let payload_status = process_payload(previous_block).unwrap();
            previous_block = None;
            if payload_status.corrupted {
                report_bad_block(&mut results, progress_listener, &payload_status);
                fail!(fail_fast, results, previous_block, false, current_block_corrupted, false, false);
            } else {
                write_block(&mut writer, &payload_status.block);
            }
            report_progress(progress_listener, &payload_status.block);
        } else {
            let payload_status_future = pool.spawn_fn(move || {
                process_payload(previous_block)
            });
            payload_status_futures.push_back(payload_status_future);
            previous_block = None;
        }

        let mut deflated_payload_bytes = vec![];
        {
            let deflated_payload_size = bgzf_block_size - 20u16 - extra_field_size;
            let mut deflated_payload_reader = reader.take(deflated_payload_size as u64);
            match deflated_payload_reader.read_to_end(&mut deflated_payload_bytes) {
                Ok(deflated_payload_read_size) => {
                    if deflated_payload_read_size < deflated_payload_size as usize {
                        fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, true);
                        break 'blocks;
                    }
                },
                Err(_) => {
                    fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, true);
                    break 'blocks;
                }
            }
        }

        let inflated_payload_crc32 = match reader.read_u32::<byteorder::LittleEndian>() {
            Ok(inflated_payload_crc32) => inflated_payload_crc32,
            Err(_) => {
                fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, true);
                break 'blocks;
            }
        };
        let inflated_payload_size = match reader.read_u32::<byteorder::LittleEndian>() {
            Ok(inflated_payload_size) => inflated_payload_size,
            Err(_) => {
                fail!(fail_fast, results, previous_block, false, current_block_corrupted, true, true);
                break 'blocks;
            }
        };

        previous_block = Some(BGZFBlock {
            header_bytes: header_bytes,
            deflated_payload_bytes: deflated_payload_bytes,
            inflated_payload_crc32: inflated_payload_crc32,
            inflated_payload_size: inflated_payload_size,
            corrupted: current_block_corrupted,
            end_position: reader.seek(SeekFrom::Current(0i64)).unwrap(),
        });

        results.blocks_count += 1;
        results.blocks_size += inflated_payload_size as u64;
    }

    let mut last_inflated_payload_size = 0u32;
    if threads == 1 {
        let payload_status = process_payload(previous_block).unwrap();
        previous_block = None;
        if payload_status.corrupted {
            report_bad_block(&mut results, progress_listener, &payload_status);
            fail!(fail_fast, results, previous_block, false, current_block_corrupted, false, false);
        } else {
            write_block(&mut writer, &payload_status.block);
        }
        last_inflated_payload_size = payload_status.inflated_payload_size;
        report_progress(progress_listener, &payload_status.block);
    } else {
        let payload_status_future = pool.spawn_fn(move || {
            process_payload(previous_block)
        });
        previous_block = None;
        payload_status_futures.push_back(payload_status_future);
        for payload_status_future in payload_status_futures.iter_mut() {
            let payload_status = payload_status_future.wait().unwrap();
            if payload_status.corrupted {
                report_bad_block(&mut results, progress_listener, &payload_status);
                fail!(fail_fast, results, previous_block, false, current_block_corrupted, false, false);
            } else {
                write_block(&mut writer, &payload_status.block);
            }
            last_inflated_payload_size = payload_status.inflated_payload_size;
            report_progress(progress_listener, &payload_status.block);
        }
    }
    if last_inflated_payload_size != 0u32 {
        results.truncated_between_blocks = true;
        write_block(&mut writer, &Some(BGZFBlock {
            header_bytes: vec![
                0x1f, 0x8b,             // gzip identifier
                0x08,                   // method (deflate)
                0x04,                   // flags (FEXTRA)
                0x00, 0x00, 0x00, 0x00, // modification time
                0x00,                   // extra flags
                0xff,                   // operating system (unknown)
                0x06, 0x00,             // extra field size (6 bytes)
                0x42, 0x43,             // bgzf identifier
                0x02, 0x00,             // extra subfield length (2 bytes)
                0x1b, 0x00,             // bgzf block size, minus one (28 bytes - 1)
            ],
            deflated_payload_bytes: vec![
                0x03, 0x00 // deflated empty string
            ],
            inflated_payload_crc32: 0,
            inflated_payload_size: 0,
            corrupted: false,
            end_position: 0
        }));
        if fail_fast {
            return results;
        }
    }

    if let Some(ref mut progress_listener) = progress_listener {
        progress_listener.on_finished();
    }

    results
}

pub fn check(reader: &mut dyn Rescuable, fail_fast: bool, threads: usize, progress_listener: &mut Option<&mut dyn ListenProgress>) -> Results {
    process(reader, None, fail_fast, threads, progress_listener)
}

pub fn rescue(reader: &mut dyn Rescuable, writer: &mut dyn Write, threads: usize, progress_listener: &mut Option<&mut dyn ListenProgress>) -> Results {
    process(reader, Some(writer), false, threads, progress_listener)
}
