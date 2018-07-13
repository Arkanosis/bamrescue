extern crate bamrescue;
extern crate docopt;
extern crate indicatif;
extern crate number_prefix;
#[macro_use]
extern crate serde_derive;

use indicatif::{
    ProgressBar,
    ProgressDrawTarget,
    ProgressStyle,
};

use std::{
    fs::File,

    io::BufReader,

    process,
};

const USAGE: &str = "
Usage: bamrescue check [--quiet] [--threads=<threads>] <bamfile>
       bamrescue rescue [--threads=<threads>] <bamfile> <output>
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

#[derive(Deserialize)]
struct Args {
    cmd_check: bool,
    cmd_rescue: bool,
    arg_bamfile: String,
    arg_output: String,
    flag_quiet: bool,
    flag_threads: usize,
    flag_version: bool,
}

struct ProgressListener {
    progress_bar: ProgressBar,
    blocks_count: u64,
    bad_blocks_count: u64,
}

impl ProgressListener {
    fn new() -> ProgressListener {
        ProgressListener {
            progress_bar: ProgressBar::hidden(),
            blocks_count: 0,
            bad_blocks_count: 0,
        }
    }
    fn update_message(&mut self) {
        self.progress_bar.set_message(&format!("{: >7} bgzf {} checked so far, {} corrupted.", self.blocks_count, if self.blocks_count > 1 { "blocks" } else { "block" }, self.bad_blocks_count));
    }
}

impl bamrescue::ListenProgress for ProgressListener {
    fn on_new_target(&mut self, target: u64) {
        self.progress_bar.set_length(target);
        self.progress_bar.set_style(ProgressStyle::default_bar()
            .template("[{wide_bar}] {percent:>3}% ({binary_bytes}/{binary_total_bytes}) [ETA: {eta_precise}]\n{msg}"));
        self.update_message();
        self.progress_bar.set_draw_target(ProgressDrawTarget::stderr());
    }
    fn on_progress(&mut self, progress: u64) {
        self.progress_bar.set_position(progress);
        self.blocks_count += 1;
        self.update_message();
    }
    fn on_bad_block(&mut self) {
        self.bad_blocks_count += 1;
        self.update_message();
    }
    fn on_finished(&mut self) {
        self.progress_bar.finish_with_message("");
    }
}

fn main() {
    let args: Args =
        docopt::Docopt::new(USAGE)
            .and_then(|docopts|
                docopts.argv(std::env::args().into_iter())
                   .deserialize()
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
        let mut progress_listener = ProgressListener::new();
        let mut reader = BufReader::new(&bamfile);
        let results = if args.cmd_check {
            bamrescue::check(&mut reader, args.flag_quiet, args.flag_threads, &mut Some(&mut progress_listener))
        } else  {
            let mut output = File::create(&args.arg_output).unwrap_or_else(|cause| {
                println!("bamrescue: can't open file: {}: {}", &args.arg_output, &cause);
                process::exit(1);
            });
            bamrescue::rescue(&mut reader, &mut output, args.flag_threads, &mut Some(&mut progress_listener))
        };
        if !args.flag_quiet {
            // TODO distinguish between repairable and unrepairable corruptions
            println!("bam file statistics:");
            match number_prefix::binary_prefix(results.blocks_size as f64) {
                number_prefix::Standalone(_) => println!("{: >7} bgzf {} checked ({} {} of bam payload)", results.blocks_count, if results.blocks_count > 1 { "blocks" } else { "block" }, results.blocks_size, if results.blocks_size > 1 { "bytes" } else { "byte" }),
                number_prefix::Prefixed(prefix, number) => println!("{: >7} bgzf {} checked ({:.0} {}B of bam payload)", results.blocks_count, if results.blocks_count > 1 { "blocks" } else { "block" }, number, prefix),
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
            if args.cmd_rescue {
                let good_blocks_count = results.blocks_count - results.bad_blocks_count;
                let good_blocks_size = results.blocks_size - results.bad_blocks_size;
                println!("{: >7} non-corrupted {} rescued ({:.2}% of total)", good_blocks_count, if good_blocks_count > 1 { "blocks" } else { "block" }, if results.blocks_count > 0 { (good_blocks_count * 100) / results.blocks_count } else { 0 });
                match number_prefix::binary_prefix(good_blocks_size as f64) {
                    number_prefix::Standalone(_) => println!("{: >7} {} of bam payload rescued ({:.2}% of total)", good_blocks_size, if good_blocks_size > 1 { "bytes" } else { "byte" }, if results.blocks_size > 0 { (good_blocks_size * 100) / results.blocks_size } else { 0 }),
                    number_prefix::Prefixed(prefix, number) => println!("{: >7.0} {}B of bam payload rescued ({:.2}% of total)", number, prefix, if results.blocks_size > 0 { (good_blocks_size * 100) / results.blocks_size } else { 0 }),
                }
            }
        }
        if args.cmd_check &&
           (results.bad_blocks_count > 0 ||
            results.truncated_in_block ||
            results.truncated_between_blocks) {
            process::exit(1);
        }
    }
}
