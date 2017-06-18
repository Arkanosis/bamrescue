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
    SeekFrom,
    Write
};

use std::str;

const GZIP_IDENTIFIER: [u8; 2] = [0x1f, 0x8b];
const BGZF_IDENTIFIER: [u8; 2] = [0x42, 0x43];

const DEFLATE: u8 = 8;

const EXTRA: u8 = 1 << 2;

pub fn version() -> &'static str {
    return option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");
}

enum BGZFBlockInformation {
    EOF,
    Size(u32)
}

pub trait Rescuable: BufRead + Seek {}
impl<T: BufRead + Seek> Rescuable for T {}

fn check_block_header(reader: &mut Rescuable, logger: &slog::Logger) -> Result<BGZFBlockInformation, Error> {
    let mut gzip_identifier = [0u8; 2];
    let read_bytes = reader.read(&mut gzip_identifier)?;
    if read_bytes == 0 {
        return Ok(BGZFBlockInformation::EOF);
    }
    if read_bytes != 2 || gzip_identifier != GZIP_IDENTIFIER {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: gzip identitifer not found"));
    }

    let mut compression_method = [0u8; 1];
    reader.read_exact(&mut compression_method)?;
    if compression_method[0] != DEFLATE {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: gzip compression method is not deflate"));
    }

    let mut flags = [0u8; 1];
    reader.read_exact(&mut flags)?;
    if flags[0] != EXTRA {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: unexpected gzip flags"));
    }

    let mut modification_time = [0u8; 4];
    reader.read_exact(&mut modification_time)?;

    let mut extra_flags = [0u8; 1];
    reader.read_exact(&mut extra_flags)?;

    let mut operating_system = [0u8; 1];
    reader.read_exact(&mut operating_system)?;

    let mut block_size = 0u16;

    let extra_field_length = reader.read_u16::<byteorder::LittleEndian>()?;
    debug!(logger, "\tExtra field length of {} bytes", extra_field_length);

    let mut remaining_extra_field_length = extra_field_length;
    while remaining_extra_field_length > 0 {
        let mut subfield_identifier = [0u8; 2];
        reader.read_exact(&mut subfield_identifier)?;

        let subfield_length = reader.read_u16::<byteorder::LittleEndian>()?;
        debug!(logger, "\t\tSubfield length of {} bytes", subfield_length);

        if subfield_identifier == BGZF_IDENTIFIER {
            debug!(logger, "\t\t\tSubfield is bgzf metadata");

            if subfield_length != 2 {
                return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: bgzf block size is not a 16 bits number"));
            }

            block_size = reader.read_u16::<byteorder::LittleEndian>()? + 1;
            debug!(logger, "\t\t\t\tbgzf block size is {} bytes", block_size);
        } else {
            reader.seek(SeekFrom::Current(subfield_length as i64))?;
        }

        remaining_extra_field_length -= 4 + subfield_length;
    }
    if block_size == 0 {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: bgzf block size not found in gzip extra field"));
    }

    Ok(BGZFBlockInformation::Size((block_size - extra_field_length - 20u16) as u32))
}

fn check_block_payload(reader: &mut Rescuable, deflated_payload_size: u32, logger: &slog::Logger) -> Result<BGZFBlockInformation, Error> {
    let mut payload_digest = crc::crc32::Digest::new(crc::crc32::IEEE);
    let inflated_payload_size;
    {
        let mut deflated_bytes = vec![];
        let mut deflate_reader = reader.take(deflated_payload_size as u64);
        deflate_reader.read_to_end(&mut deflated_bytes)?;
        let inflated_bytes = match inflate::inflate_bytes(&deflated_bytes) {
            Ok(inflated_bytes) => inflated_bytes,
            Err(error) => return Err(Error::new(ErrorKind::InvalidData, format!("Invalid bam file: unable to inflate payload: {}", error))),
        };
        payload_digest.write(&inflated_bytes);
        inflated_payload_size = inflated_bytes.len();
    }

    let mut data_crc32 = [0u8; 4];
    reader.read_exact(&mut data_crc32)?;
    let payload_crc32 = payload_digest.sum32();

    if data_crc32[0] != ((payload_crc32 & 0xff) as u8) ||
        data_crc32[1] != (((payload_crc32 >> 8) & 0xff) as u8) ||
        data_crc32[2] != (((payload_crc32 >> 16) & 0xff) as u8) ||
        data_crc32[3] != (((payload_crc32 >> 24) & 0xff) as u8) {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: incorrect payload CRC32"));
        }

    let data_size = reader.read_u32::<byteorder::LittleEndian>()?;
    debug!(logger, "\tData size is {} bytes", data_size);
    if data_size as usize != inflated_payload_size {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: incorrect payload size"));
    }

    Ok(BGZFBlockInformation::Size(data_size))
}

fn check_block(reader: &mut Rescuable, logger: &slog::Logger) -> Result<BGZFBlockInformation, Error> {
    let deflated_payload_size = match check_block_header(reader, &logger)? {
        BGZFBlockInformation::EOF => return Ok(BGZFBlockInformation::EOF),
        BGZFBlockInformation::Size(deflated_payload_size) => deflated_payload_size,
    };

    let payload_position = reader.seek(SeekFrom::Current(0i64))?;

    match check_block_payload(reader, deflated_payload_size, &logger) {
        Ok(bgzf_information) => Ok(bgzf_information),
        Err(error) => {
            reader.seek(SeekFrom::Start(payload_position + deflated_payload_size as u64 + 8u64))?;
            Err(error)
        }
    }
}

pub fn check(reader: &mut Rescuable, quiet: bool, logger: &slog::Logger) -> Result<(), Error> {
    info!(logger, "Checking integrity…");

    let mut blocks_count = 0u64;
    let mut blocks_size = 0u64;
    let mut bad_blocks_count = 0u64;
    let mut bad_blocks_size = 0u64;
    let mut truncated = false;
    let mut data_size = 0u32;
    loop {
        let block_offset = reader.seek(SeekFrom::Current(0))?;
        debug!(logger, "Checking block {} at offset {}", blocks_count + 1, block_offset);

        match check_block(reader, &logger) {
            Ok(bgzf_size) => {
                data_size = match bgzf_size {
                    BGZFBlockInformation::EOF => {
                        if data_size != 0u32 {
                            truncated = true;
                            if quiet {
                                return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: unexpected end of file while last bgzf block was not empty"));
                            }
                        }
                        break
                    },
                    BGZFBlockInformation::Size(data_size) => data_size,
                };
            },
            Err(error) => {
                if quiet {
                    return Err(error);
                }
                bad_blocks_count += 1;
                let current_offset = reader.seek(SeekFrom::Current(0))?;
                bad_blocks_size += current_offset - block_offset;
            }
        }


        blocks_count += 1;
        blocks_size += data_size as u64;
    }

    if !quiet {
        println!("bam file statistics:");
        match number_prefix::binary_prefix(blocks_size as f64) {
            number_prefix::Standalone(_) => println!("{: >7} bgzf {} found ({} {} of bam payload)", blocks_count, if blocks_count > 1 { "blocks" } else { "block" }, blocks_size, if blocks_size > 1 { "bytes" } else { "byte" }),
            number_prefix::Prefixed(prefix, number) => println!("{: >7} bgzf {} found ({:.0} {}B of bam payload)", blocks_count, if blocks_count > 1 { "blocks" } else { "block" }, number, prefix),
        }
        println!("{: >7} corrupted {} found ({:.2}% of total)", bad_blocks_count, if bad_blocks_count > 1 { "blocks" } else { "block" }, (bad_blocks_count * 100) / blocks_count);
        match number_prefix::binary_prefix(bad_blocks_size as f64) {
            number_prefix::Standalone(_) => println!("{: >7} {} of bam payload lost ({:.2}% of total)", bad_blocks_size, if bad_blocks_size > 1 { "bytes" } else { "byte" }, (bad_blocks_size * 100) / blocks_size),
            number_prefix::Prefixed(prefix, number) => println!("{: >7.0} {}B of bam payload lost ({:.2}% of total)", number, prefix, (bad_blocks_size * 100) / blocks_size),
        }
        if truncated {
            println!("        file truncated");
        }
    }

    if bad_blocks_count > 0 {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: corrupted bgzf blocks found"));
    }

    if truncated {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: unexpected end of file while last bgzf block was not empty"));
    }

    Ok(())
}

pub fn rescue(reader: &mut Rescuable, writer: &mut Write, logger: &slog::Logger) -> Result<(), Error> {
    info!(logger, "Rescuing file…");

    error!(logger, "bamrescue::rescue() is not yet implemented");
    unimplemented!();
}
