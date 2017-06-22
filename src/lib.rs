extern crate byteorder;
extern crate crc;
extern crate inflate;
extern crate number_prefix;
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

pub fn check(reader: &mut Rescuable, quiet: bool, logger: &slog::Logger) -> Result<(), Error> {
    info!(logger, "Checking integrity…");

    let mut blocks_count = 0u64;
    let mut blocks_size = 0u64;
    let mut bad_blocks_count = 0u64;
    let mut bad_blocks_size = 0u64;
    let mut truncated_in_block = false;
    let mut truncated_between_blocks = false;

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
                        if quiet {
                            return Err(Error::new(ErrorKind::InvalidData, format!("Invalid bam file: unexpected end of file while reading header of block {}", blocks_count)));
                        }
                        truncated_in_block = true;
                        bad_blocks_count += 1;
                        break;
                    }
                },
                Err(error) => {
                    if quiet {
                        return Err(error);
                    }
                    truncated_in_block = true;
                    bad_blocks_count += 1;
                    break;
                }
            }
        }

        if header_bytes[0..2] != GZIP_IDENTIFIER {
            // TODO recoverable if only a bitflip or two
            if quiet {
                return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: gzip identitifer not found"));
            }
            // TODO seek right position, see below
            panic!("Unexpected byte while checking header of block {}", blocks_count);
        }

        if header_bytes[2] != DEFLATE {
            if quiet {
                return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: gzip compression method is not deflate"));
            }
            // TODO seek right position, see below
            panic!("Unexpected byte while checking header of block {}", blocks_count);
        }

        if header_bytes[3] != FEXTRA {
            if quiet {
                return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: unexpected gzip flags"));
            }
            // TODO seek right position, see below
            panic!("Unexpected byte while checking header of block {}", blocks_count);
        }

        // header_bytes[4..8] => modification time; can be anything
        // header_bytes[8] => extra flags; can be anything
        // header_bytes[9] => operating system; can be anything

        let extra_field_size = 6u16;
        if header_bytes[10..16] != [
            0x06, 0x00, // extra field length (6 bytes)
            0x42, 0x43, // bgzf identifier
            0x02, 0x00  // extra subfield length (2 bytes)
        ] {
            if quiet {
                return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: bgzf block size not found in gzip extra field"));
            }
            // TODO recoverable if only a bitflip or two
            // TODO FIXME handle other extra subfields, skip until we get the right one and skip after too
            // TODO FIXME don't forget to update extra_field_size
            panic!("Unexpected byte while checking header of block {}", blocks_count);
        }

        // TODO if not at the right position for the next header, fix the previous header / payload or
        // the current header, seek to the right position and “continue”l

        match check_payload(&previous_block) {
            Ok(_) => (),
            Err(error) => {
                if quiet {
                    return Err(error);
                }
                bad_blocks_count += 1;
            }
        }

        let block_size = match reader.read_u16::<byteorder::LittleEndian>() {
            Ok(block_size) => block_size + 1,
            Err(error) => {
                if quiet {
                    return Err(error);
                }
                truncated_in_block = true;
                bad_blocks_count += 1;
                break;
            }
        };

        let mut deflated_payload_bytes = vec![];
        {
            let deflated_payload_size = block_size - 20u16 - extra_field_size;
            let mut deflated_payload_reader = reader.take(deflated_payload_size as u64);
            match deflated_payload_reader.read_to_end(&mut deflated_payload_bytes) {
                Ok(deflated_payload_read_size) => {
                    if deflated_payload_read_size < deflated_payload_size as usize {
                        if quiet {
                            return Err(Error::new(ErrorKind::InvalidData, format!("Invalid bam file: unexpected end of file while reading payload of block {}", blocks_count)));
                        }
                        truncated_in_block = true;
                        bad_blocks_count += 1;
                        break;
                    }
                },
                Err(error) => {
                    if quiet {
                        return Err(error);
                    }
                    truncated_in_block = true;
                    bad_blocks_count += 1;
                    break;
                }
            }
        }

        let inflated_payload_crc32 = match reader.read_u32::<byteorder::LittleEndian>() {
            Ok(inflated_payload_crc32) => inflated_payload_crc32,
            Err(error) => {
                if quiet {
                    return Err(error);
                }
                truncated_in_block = true;
                bad_blocks_count += 1;
                break;
            }
        };
        let inflated_payload_size = match reader.read_u32::<byteorder::LittleEndian>() {
            Ok(inflated_payload_size) => inflated_payload_size,
            Err(error) => {
                if quiet {
                    return Err(error);
                }
                truncated_in_block = true;
                bad_blocks_count += 1;
                break;
            }
        };

        previous_block = Some(BGZFBlock {
            id: blocks_count,
            header_bytes: header_bytes,
            deflated_payload_bytes: deflated_payload_bytes,
            inflated_payload_crc32: inflated_payload_crc32,
            inflated_payload_size: inflated_payload_size,
        });

        blocks_count += 1;
    }

    match check_payload(&previous_block) {
        Ok(_) => (),
        Err(error) => {
            if quiet {
                return Err(error);
            }
            bad_blocks_count += 1;
        }
    }

    match previous_block {
        None => (),
        Some(last_block) => {
            if last_block.inflated_payload_size != 0 {
                if quiet {
                    return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: unexpected end of file while last bgzf block was not empty"));
                }
                truncated_between_blocks = true;
            }
        }
    }

    if !quiet {
        // TODO distinguish between repairable and unrepairable corruptions
        println!("bam file statistics:");
        match number_prefix::binary_prefix(blocks_size as f64) {
            number_prefix::Standalone(_) => println!("{: >7} bgzf {} found ({} {} of bam payload)", blocks_count, if blocks_count > 1 { "blocks" } else { "block" }, blocks_size, if blocks_size > 1 { "bytes" } else { "byte" }),
            number_prefix::Prefixed(prefix, number) => println!("{: >7} bgzf {} found ({:.0} {}B of bam payload)", blocks_count, if blocks_count > 1 { "blocks" } else { "block" }, number, prefix),
        }
        println!("{: >7} corrupted {} found ({:.2}% of total)", bad_blocks_count, if bad_blocks_count > 1 { "blocks" } else { "block" }, if blocks_count > 0 { (bad_blocks_count * 100) / blocks_count } else { 0 });
        match number_prefix::binary_prefix(bad_blocks_size as f64) {
            number_prefix::Standalone(_) => println!("{: >7} {} of bam payload lost ({:.2}% of total)", bad_blocks_size, if bad_blocks_size > 1 { "bytes" } else { "byte" }, if blocks_size > 0 { (bad_blocks_size * 100) / blocks_size } else { 0 }),
            number_prefix::Prefixed(prefix, number) => println!("{: >7.0} {}B of bam payload lost ({:.2}% of total)", number, prefix, if blocks_size > 0 { (bad_blocks_size * 100) / blocks_size } else { 0 }),
        }
        if truncated_in_block {
            println!("        file truncated in a bgzf block");
        }
        if truncated_between_blocks {
            println!("        file truncated between two bgzf block");
        }
    }

    if bad_blocks_count > 0 {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: corrupted bgzf blocks found"));
    }

    if truncated_in_block {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: unexpected end of file while checking a bgzf block"));
    }

    if truncated_between_blocks {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: unexpected end of file while last bgzf block was not empty"));
    }

    Ok(())
}

pub fn rescue(reader: &mut Rescuable, writer: &mut Write, logger: &slog::Logger) -> Result<(), Error> {
    info!(logger, "Rescuing file…");

    error!(logger, "bamrescue::rescue() is not yet implemented");
    unimplemented!();
}
