extern crate byteorder;
#[macro_use]
extern crate slog;

use byteorder::ReadBytesExt;

use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::string::String;

const GZIP_IDENTIFIER: [u8; 2] = [0x1f, 0x8b];
const BGZF_IDENTIFIER: [u8; 2] = [0x42, 0x43];

const DEFLATE: u8 = 8;

const FHCRC: u8 = 1 << 1;
const FEXTRA: u8 = 1 << 2;
const FNAME: u8 = 1 << 3;
const FCOMMENT: u8 = 1 << 4;

pub fn version() -> &'static str {
    return option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");
}

enum BGZFBlockInformation {
    EOF,
    Size(u32)
}

fn check_block(reader: &mut BufReader<File>, blocks_count: &mut u64, logger: &slog::Logger) -> Result<BGZFBlockInformation, Error> {
    let mut gzip_identifier = [0u8; 2];
    let read_bytes = reader.read(&mut gzip_identifier)?;
    if read_bytes == 0 {
        return Ok(BGZFBlockInformation::EOF);
    }
    if read_bytes != 2 || gzip_identifier != GZIP_IDENTIFIER {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: gzip identitifer not found"));
    }
    *blocks_count += 1;
    debug!(logger, "Checking block {}", blocks_count);

    let mut compression_method = [0u8; 1];
    reader.read_exact(&mut compression_method)?;
    if compression_method[0] != DEFLATE {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: gzip compression method is not deflate"));
    }

    let mut flags = [0u8; 1];
    reader.read_exact(&mut flags)?;

    let mut modification_time = [0u8; 4];
    reader.read_exact(&mut modification_time)?;

    let mut extra_flags = [0u8; 1];
    reader.read_exact(&mut extra_flags)?;

    let mut operating_system = [0u8; 1];
    reader.read_exact(&mut operating_system)?;

    let mut block_size = 0u16;
    let mut extra_field_length = 0;
    if (flags[0] & FEXTRA) != 0 {
        extra_field_length = reader.read_u16::<byteorder::LittleEndian>()?;
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
    }
    if block_size == 0 {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: bgzf block size not found in gzip extra field"));
    }

    if (flags[0] & FNAME) != 0 {
        let mut file_name = vec![];
        reader.read_until(0u8, &mut file_name)?;
        debug!(logger, "\tFile name is \"{}\"", String::from_utf8(file_name).unwrap_or(String::from("<invalid_encoding>")));
    }

    if (flags[0] & FCOMMENT) != 0 {
        let mut file_comment = vec![];
        reader.read_until(0u8, &mut file_comment)?;
        debug!(logger, "\tFile comment is \"{}\"", String::from_utf8(file_comment).unwrap_or(String::from("<invalid_encoding>")));
    }

    if (flags[0] & FHCRC) != 0 {
        let mut header_crc16 = [0u8; 2];
        reader.read_exact(&mut header_crc16)?;
        // TODO check the CRC16 against the 2 least signigicant bytes of the CRC32 of the header
    }

    reader.seek(SeekFrom::Current((block_size - extra_field_length - 20u16) as i64))?;
    // TODO actually read data to compute its CRC32

    let mut data_crc32 = [0u8; 4];
    reader.read_exact(&mut data_crc32)?;
    // TODO check the CRC32 against the uncompressed data

    let data_size = reader.read_u32::<byteorder::LittleEndian>()?;
    debug!(logger, "\tData size is {} bytes", data_size);

    debug!(logger, "\tAt offset {}", reader.seek(SeekFrom::Current(0))?);

    return Ok(BGZFBlockInformation::Size(data_size));
}

pub fn check(bamfile: &str, logger: &slog::Logger) -> Result<(), Error> {
    info!(logger, "Checking integrity of {}…", bamfile);

    let mut reader = BufReader::new(File::open(bamfile)?);

    let mut blocks_count = 0u64;
    let mut data_size = 0u32;
    loop {
        data_size = match check_block(&mut reader, &mut blocks_count, &logger)? {
            BGZFBlockInformation::EOF => if data_size == 0u32 { break } else { return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: unexpected end of file while last bgzf block was not empty")); },
            BGZFBlockInformation::Size(data_size) => data_size,
        };
    }

    println!("bam file statistics:");
    println!("{: >7} bgzf blocks found", blocks_count);
    println!("{: >7} corrupted blocks found", 0);
    Ok(())
}

pub fn repair(bamfile: &str, output: &str, logger: &slog::Logger) -> Result<(), Error> {
    info!(logger, "Repairing {} and writing output to {}…", bamfile, output);

    error!(logger, "bamrescue::repair() is not yet implemented");
    unimplemented!();
}
