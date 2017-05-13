#[macro_use]
extern crate slog;

pub fn check(bamfile: String, logger: slog::Logger) {
    info!(logger, "Checking integrity of {}…", bamfile);
}

pub fn repair(bamfile: String, output: String, logger: slog::Logger) {
    info!(logger, "Repairing {} and writing output to {}…", bamfile, output);
}
