extern crate bamrescue;
extern crate docopt;
extern crate rustc_serialize;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

use slog::Drain;
use std::io::Write;
use std::process;

const USAGE: &str = "
Usage: bamrescue check <bamfile>
       bamrescue repair <bamfile> <output>
       bamrescue -h | --help
       bamrescue --version

Commands:
    check       Check BAM file for corruption.
    repair      Keep only non-corrupted blocks of BAM file.

Arguments:
    bamfile     BAM file to check or repair.
    output      Repaired BAM file.

Options:
    -h, --help  Show this screen.
    --version   Show version.
";

#[derive(RustcDecodable)]
struct Args {
    cmd_check: bool,
    cmd_repair: bool,
    arg_bamfile: String,
    arg_output: String,
    flag_version: bool,
}

fn main() {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
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
        bamrescue::check(&args.arg_bamfile, &logger).unwrap_or_else(|error| {
            writeln!(&mut std::io::stderr(), "bamrescue: Unable to check bam file: {}", error).unwrap();
            process::exit(1);
        });
    } else if args.cmd_repair {
        bamrescue::repair(&args.arg_bamfile, &args.arg_output, &logger).unwrap_or_else(|error| {
            writeln!(&mut std::io::stderr(), "bamrescue: Unable to repair bam file: {}", error).unwrap();
            process::exit(1);
        });
    }
}
