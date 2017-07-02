extern crate byteorder;
extern crate crc;
extern crate inflate;
#[macro_use]
extern crate slog;

use byteorder::ReadBytesExt;

use crc::crc32::Hasher32;

use std::io::{
    BufRead,
    Error,
    ErrorKind,
    Read,
    Seek,
    SeekFrom,
    Write
};

use std::str;

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
    id: u64,
    header_bytes: Vec<u8>,
    deflated_payload_bytes: Vec<u8>,
    inflated_payload_crc32: u32,
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

fn check_payload(block: &Option<BGZFBlock>) -> Result<(), Error> {
    match *block {
        None => Ok(()),
        Some(ref block) => {
            let inflated_payload_bytes = match inflate::inflate_bytes(&block.deflated_payload_bytes) {
                Ok(inflated_payload_bytes) => inflated_payload_bytes,
                Err(error) => return Err(Error::new(ErrorKind::InvalidData, format!("Invalid bam file: unable to inflate payload of block {}: {}", block.id, error))),
            };

            let mut inflated_payload_digest = crc::crc32::Digest::new(crc::crc32::IEEE);
            inflated_payload_digest.write(&inflated_payload_bytes);
            let inflated_payload_crc32 = inflated_payload_digest.sum32();
            if inflated_payload_crc32 != block.inflated_payload_crc32 {
                return Err(Error::new(ErrorKind::InvalidData, format!("Invalid bam file: incorrect payload CRC32 of block {}", block.id)));
            }

            let inflated_payload_size = inflated_payload_bytes.len() as u32;
            if inflated_payload_size != block.inflated_payload_size {
                // TODO recoverable (wrong size is not a big issue if the CRC32 is correct)
                return Err(Error::new(ErrorKind::InvalidData, format!("Invalid bam file: incorrect payload size of block {}", block.id)));
            }

            Ok(())
        }
    }
}

pub fn check(reader: &mut Rescuable, fail_fast: bool, logger: &slog::Logger) -> Results {
    info!(logger, "Checking integrity…");

    let mut results = Results {
        blocks_count: 0u64,
        blocks_size: 0u64,
        bad_blocks_count: 0u64,
        bad_blocks_size: 0u64,
        truncated_in_block: false,
        truncated_between_blocks: false,
    };

    let mut previous_block: Option<BGZFBlock> = None;
    loop {
        let mut header_bytes = vec![];
        {
            let mut header_reader = reader.take(16);
            match header_reader.read_to_end(&mut header_bytes) {
                Ok(header_size) => {
                    if header_size == 0 {
                        break;
                    }

                    if header_size < 16 {
                        results.truncated_in_block = true;
                        results.bad_blocks_count += 1;
                        if fail_fast {
                            return results;
                        }
                        break;
                    }
                },
                Err(error) => {
                    results.truncated_in_block = true;
                    results.bad_blocks_count += 1;
                    if fail_fast {
                        return results;
                    }
                    break;
                }
            }
        }

        if header_bytes[0..2] != GZIP_IDENTIFIER {
            // TODO recoverable if only a bitflip or two
            results.bad_blocks_count += 1;
            if fail_fast {
                return results;
            }
            // TODO seek right position, see below
            panic!("Unexpected byte while checking header of block {}", results.blocks_count);
        }

        if header_bytes[2] != DEFLATE {
            results.bad_blocks_count += 1;
            if fail_fast {
                return results;
            }
            // TODO seek right position, see below
            panic!("Unexpected byte while checking header of block {}", results.blocks_count);
        }

        if header_bytes[3] != FEXTRA {
            results.bad_blocks_count += 1;
            if fail_fast {
                return results;
            }
            // TODO seek right position, see below
            panic!("Unexpected byte while checking header of block {}", results.blocks_count);
        }

        // header_bytes[4..8] => modification time; can be anything
        // header_bytes[8] => extra flags; can be anything
        // header_bytes[9] => operating system; can be anything

        let mut bgzf_block_size = 0u16;

        let mut extra_field_size = 6u16;
        if header_bytes[10..16] != [
            0x06, 0x00, // extra field length (6 bytes)
            0x42, 0x43, // bgzf identifier
            0x02, 0x00  // extra subfield length (2 bytes)
        ] {
            // TODO recoverable if only a bitflip or two

            extra_field_size = match (&mut &header_bytes[10..12]).read_u16::<byteorder::LittleEndian>() {
                Ok(extra_field_size) => extra_field_size,
                Err(error) => {
                    results.bad_blocks_count += 1;
                    if fail_fast {
                        return results;
                    }
                    break;
                }
            };

            if header_bytes[12..16] == [
                0x42, 0x43, // bgzf identifier
                0x02, 0x00  // extra subfield length (2 bytes)
            ] {
                bgzf_block_size = match reader.read_u16::<byteorder::LittleEndian>() {
                    Ok(bgzf_block_size) => bgzf_block_size + 1,
                    Err(error) => {
                        results.truncated_in_block = true;
                        results.bad_blocks_count += 1;
                        if fail_fast {
                            return results;
                        }
                        break;
                    }
                };
                match reader.seek(SeekFrom::Current((extra_field_size - 6u16) as i64)) {
                    Ok(_) => (),
                    Err(error) => {
                        results.truncated_in_block = true;
                        results.bad_blocks_count += 1;
                        if fail_fast {
                            return results;
                        }
                        break;
                    }
                }
                // TODO the bgzf extra subfield is the first, but check the other subfields nonetheless
            } else {
                let first_extra_subfield_size = match (&mut &header_bytes[14..16]).read_u16::<byteorder::LittleEndian>() {
                    Ok(first_extra_subfield_size) => first_extra_subfield_size,
                    Err(error) => {
                        results.bad_blocks_count += 1;
                        if fail_fast {
                            return results;
                        }
                        break;
                    }
                };

                if first_extra_subfield_size > extra_field_size {
                    results.bad_blocks_count += 1;
                    if fail_fast {
                        return results;
                    }
                    break;
                }

                match reader.seek(SeekFrom::Current(first_extra_subfield_size as i64)) {
                    Ok(_) => (),
                    Err(error) => {
                        results.truncated_in_block = true;
                        results.bad_blocks_count += 1;
                        if fail_fast {
                            return results;
                        }
                        break;
                    }
                }

                let mut remaining_extra_field_size = extra_field_size - first_extra_subfield_size;
                while remaining_extra_field_size > 4 {
                    let mut extra_subfield_identifier = [0u8; 2];
                    match reader.read_exact(&mut extra_subfield_identifier) {
                        Ok(_) => (),
                        Err(error) => {
                            results.truncated_in_block = true;
                            results.bad_blocks_count += 1;
                            if fail_fast {
                                return results;
                            }
                            break;
                        }
                    }

                    let extra_subfield_size = match reader.read_u16::<byteorder::LittleEndian>() {
                        Ok(extra_subfield_size) => extra_subfield_size,
                        Err(error) => {
                            results.truncated_in_block = true;
                            results.bad_blocks_count += 1;
                            if fail_fast {
                                return results;
                            }
                            break;
                        }
                    };

                    if extra_subfield_identifier == BGZF_IDENTIFIER {
                        if extra_subfield_size != 2 {
                            results.bad_blocks_count += 1;
                            if fail_fast {
                                return results;
                            }
                            break;
                        }
                        bgzf_block_size = match reader.read_u16::<byteorder::LittleEndian>() {
                            Ok(bgzf_block_size) => bgzf_block_size + 1,
                            Err(error) => {
                                results.truncated_in_block = true;
                                results.bad_blocks_count += 1;
                                if fail_fast {
                                    return results;
                                }
                                break;
                            }
                        };
                    } else {
                        match reader.seek(SeekFrom::Current(extra_subfield_size as i64)) {
                            Ok(_) => (),
                            Err(error) => {
                                results.truncated_in_block = true;
                                results.bad_blocks_count += 1;
                                if fail_fast {
                                    return results;
                                }
                                break;
                            }
                        }
                    }

                    remaining_extra_field_size -= 4 + extra_subfield_size;
                }

                if bgzf_block_size == 0u16 {
                    results.bad_blocks_count += 1;
                    if fail_fast {
                        return results;
                    }
                    break;
                }
            }
        }

        // TODO if not at the right position for the next header, fix the previous header / payload or
        // the current header, seek to the right position and “continue”

        match check_payload(&previous_block) {
            Ok(_) => (),
            Err(error) => {
                results.bad_blocks_count += 1;
                results.bad_blocks_size += match previous_block {
                    None => 0,
                    Some(ref previous_block) => previous_block.inflated_payload_size as u64,
                };
                if fail_fast {
                    return results;
                }
            }
        }

        if bgzf_block_size == 0 {
            bgzf_block_size = match reader.read_u16::<byteorder::LittleEndian>() {
                Ok(bgzf_block_size) => bgzf_block_size + 1,
                Err(error) => {
                    results.truncated_in_block = true;
                    results.bad_blocks_count += 1;
                    if fail_fast {
                        return results;
                    }
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
                        results.truncated_in_block = true;
                        results.bad_blocks_count += 1;
                        if fail_fast {
                            return results;
                        }
                        break;
                    }
                },
                Err(error) => {
                    results.truncated_in_block = true;
                    results.bad_blocks_count += 1;
                    if fail_fast {
                        return results;
                    }
                    break;
                }
            }
        }

        let inflated_payload_crc32 = match reader.read_u32::<byteorder::LittleEndian>() {
            Ok(inflated_payload_crc32) => inflated_payload_crc32,
            Err(error) => {
                results.truncated_in_block = true;
                results.bad_blocks_count += 1;
                if fail_fast {
                    return results;
                }
                break;
            }
        };
        let inflated_payload_size = match reader.read_u32::<byteorder::LittleEndian>() {
            Ok(inflated_payload_size) => inflated_payload_size,
            Err(error) => {
                results.truncated_in_block = true;
                results.bad_blocks_count += 1;
                if fail_fast {
                    return results;
                }
                break;
            }
        };

        previous_block = Some(BGZFBlock {
            id: results.blocks_count,
            header_bytes: header_bytes,
            deflated_payload_bytes: deflated_payload_bytes,
            inflated_payload_crc32: inflated_payload_crc32,
            inflated_payload_size: inflated_payload_size,
        });

        results.blocks_count += 1;
        results.blocks_size += inflated_payload_size as u64;
    }

    match check_payload(&previous_block) {
        Ok(_) => (),
        Err(error) => {
            results.bad_blocks_count += 1;
            results.bad_blocks_size += match previous_block {
                None => 0,
                Some(ref previous_block) => previous_block.inflated_payload_size as u64,
            };
            if fail_fast {
                return results;
            }
        }
    }

    match previous_block {
        None => (),
        Some(ref last_block) => {
            if last_block.inflated_payload_size != 0 {
                results.truncated_between_blocks = true;
                if fail_fast {
                    return results;
                }
            }
        }
    }

    results
}

pub fn rescue(reader: &mut Rescuable, writer: &mut Write, logger: &slog::Logger) -> Result<(), Error> {
    info!(logger, "Rescuing file…");

    error!(logger, "bamrescue::rescue() is not yet implemented");
    unimplemented!();
}
