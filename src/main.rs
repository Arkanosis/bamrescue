extern crate docopt;
extern crate rustc_serialize;

const USAGE: &'static str = "
Usage: bamrescue <source> <destination>
       bamrescue -h | --help
       bamrescue --version

Arguments:
    source      BAM file to check or repair.
    destination Repaired BAM file.

Options:
    -h, --help  Show this screen.
    --version   Show version.
";

#[derive(RustcDecodable)]
struct Args {
    arg_source: String,
    arg_destination: String,
    flag_version: bool,
}

fn main() {
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
        println!("bamrescue v{}", option_env!("CARGO_PKG_VERSION").unwrap_or("unknown"));
    } else {
        println!("Rescuing {} to {}", args.arg_source, args.arg_destination);
    }
}
