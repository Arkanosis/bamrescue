extern crate bamrescue;
extern crate docopt;
extern crate number_prefix;
extern crate rustc_serialize;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

use slog::Drain;

use std::{
    fs::File,

    io::BufReader,

    process,
};

const USAGE: &str = "
Usage: bamrescue check [--quiet] [--threads=<threads>] <bamfile>
       bamrescue rescue <bamfile> <output>
       bamrescue -h | --help
       bamrescue --version

Commands:
    check                Check BAM file for corruption.
    rescue               Keep only non-corrupted blocks of BAM file.

Arguments:
    bamfile              BAM file to check or rescue.
    output               Rescued BAM file.

Options:
    -h, --help           Show this screen.
    -q, --quiet          Do not output statistics, stop at first error.
    --threads=<threads>  Number of threads to use, 0 for auto [default: 0].
    --version            Show version.
";

#[derive(RustcDecodable)]
struct Args {
    cmd_check: bool,
    cmd_rescue: bool,
    arg_bamfile: String,
    arg_output: String,
    flag_quiet: bool,
    flag_threads: usize,
    flag_version: bool,
}

fn main() {
    // TODO FIXME remove loggers alltogether (?)
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog::LevelFilter(drain, slog::Level::Info).fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let logger = slog::Logger::root(drain, o!());

    let args: Args =
        docopt::Docopt::new(USAGE)
            .and_then(|docopts|
                docopts.argv(std::env::args().into_iter())
                   .decode()
            )
            .unwrap_or_else(|error|
                error.exit()
            );

    if args.flag_version {
        println!("bamrescue v{}", bamrescue::version());
    } else if args.cmd_check || args.cmd_rescue {
        let bamfile = File::open(&args.arg_bamfile).unwrap_or_else(|cause| {
            println!("bamrescue: can't open file: {}: {}", &args.arg_bamfile, &cause);
            process::exit(1);
        });
        let mut reader = BufReader::new(&bamfile);
        let results = if args.cmd_check {
            bamrescue::check(&mut reader, args.flag_quiet, args.flag_threads, &logger)
        } else  {
            let mut output = File::create(&args.arg_output).unwrap_or_else(|cause| {
                println!("bamrescue: can't open file: {}: {}", &args.arg_output, &cause);
                process::exit(1);
            });
            bamrescue::rescue(&mut reader, &mut output, args.flag_threads, &logger)
        };
        if !args.flag_quiet {
            // TODO distinguish between repairable and unrepairable corruptions
            println!("bam file statistics:");
            match number_prefix::binary_prefix(results.blocks_size as f64) {
                number_prefix::Standalone(_) => println!("{: >7} bgzf {} found ({} {} of bam payload)", results.blocks_count, if results.blocks_count > 1 { "blocks" } else { "block" }, results.blocks_size, if results.blocks_size > 1 { "bytes" } else { "byte" }),
                number_prefix::Prefixed(prefix, number) => println!("{: >7} bgzf {} found ({:.0} {}B of bam payload)", results.blocks_count, if results.blocks_count > 1 { "blocks" } else { "block" }, number, prefix),
            }
            println!("{: >7} corrupted {} found ({:.2}% of total)", results.bad_blocks_count, if results.bad_blocks_count > 1 { "blocks" } else { "block" }, if results.blocks_count > 0 { (results.bad_blocks_count * 100) / results.blocks_count } else { 0 });
            match number_prefix::binary_prefix(results.bad_blocks_size as f64) {
                number_prefix::Standalone(_) => println!("{: >7} {} of bam payload lost ({:.2}% of total)", results.bad_blocks_size, if results.bad_blocks_size > 1 { "bytes" } else { "byte" }, if results.blocks_size > 0 { (results.bad_blocks_size * 100) / results.blocks_size } else { 0 }),
                number_prefix::Prefixed(prefix, number) => println!("{: >7.0} {}B of bam payload lost ({:.2}% of total)", number, prefix, if results.blocks_size > 0 { (results.bad_blocks_size * 100) / results.blocks_size } else { 0 }),
            }
            if results.truncated_in_block {
                println!("        file truncated in a bgzf block");
            }
            if results.truncated_between_blocks {
                println!("        file truncated between two bgzf block");
            }
        }
        if results.bad_blocks_count > 0 ||
           results.truncated_in_block ||
           results.truncated_between_blocks {
            process::exit(1);
        }
    }
}
