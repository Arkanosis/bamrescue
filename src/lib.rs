#[macro_use]
extern crate slog;

use std::fs::File;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Read;

pub fn version() -> &'static str {
    return option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");
}

pub fn check(bamfile: &str, logger: &slog::Logger) -> Result<(), Error> {
    info!(logger, "Checking integrity of {}…", bamfile);

    let mut file = File::open(bamfile)?;
    let mut buffer = [0; 2];
    let bytes_read = file.read(&mut buffer)?;

    if bytes_read != 2 || buffer != [0x1f, 0x8b] {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid bam file: gzip magic number not found"));
    }

    error!(logger, "bamrescue::check() is not yet fully= implemented");
    unimplemented!();
}

pub fn repair(bamfile: &str, output: &str, logger: &slog::Logger) -> Result<(), Error> {
    info!(logger, "Repairing {} and writing output to {}…", bamfile, output);

    error!(logger, "bamrescue::repair() is not yet implemented");
    unimplemented!();
}
