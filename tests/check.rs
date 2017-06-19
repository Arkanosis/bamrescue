#[macro_use]
extern crate slog;

extern crate bamrescue;

mod common;

#[test]
fn empty_file() {
    match bamrescue::check(&mut common::empty_file(), false, &common::null_logger()) {
        Ok(()) => (),
        Err(error) => panic!(error),
    }
}

#[test]
fn empty_bam() {
    match bamrescue::check(&mut common::empty_bam(), false, &common::null_logger()) {
        Ok(()) => (),
        Err(error) => panic!(error),
    }
}
