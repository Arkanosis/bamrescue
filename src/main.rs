extern crate bamrescue;
extern crate docopt;
extern crate rustc_serialize;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

use slog::Drain;

use std::fs::File;

use std::io::BufReader;

use std::process;

const USAGE: &str = "
Usage: bamrescue check [--quiet] <bamfile>
       bamrescue rescue <bamfile> <output>
       bamrescue -h | --help
       bamrescue --version

Commands:
    check        Check BAM file for corruption.
    rescue       Keep only non-corrupted blocks of BAM file.

Arguments:
    bamfile      BAM file to check or rescue.
    output       Rescued BAM file.

Options:
    -h, --help   Show this screen.
    -q, --quiet  Do not output statistics, stop at first error.
    --version    Show version.
";

#[derive(RustcDecodable)]
struct Args {
    cmd_check: bool,
    cmd_rescue: bool,
    arg_bamfile: String,
    arg_output: String,
    flag_quiet: bool,
    flag_version: bool,
}

fn main() {
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
    } else if args.cmd_check {
        File::open(&args.arg_bamfile).and_then(|bamfile| {
            let mut reader = BufReader::new(&bamfile);
            bamrescue::check(&mut reader, args.flag_quiet, &logger)
        }).unwrap_or_else(|cause| {
            error!(logger, "{}", cause);
            drop(logger);
            process::exit(1);
        });
    } else if args.cmd_rescue {
        File::open(&args.arg_bamfile).and_then(|bamfile| {
            File::create(&args.arg_output).and_then(|mut output| {
                let mut reader = BufReader::new(&bamfile);
                bamrescue::rescue(&mut reader, &mut output, &logger)
            })
        }).unwrap_or_else(|cause| {
            error!(logger, "Unable to rescue bam file: {}", cause);
            drop(logger);
            process::exit(1);
        });
    }
}
