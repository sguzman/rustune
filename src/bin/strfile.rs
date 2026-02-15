use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{ArgAction, Parser};
use tracing::{debug, instrument};

use rustune::datfile::dat_path_for_text;
use rustune::logging::init_logging;
use rustune::strfile_builder::{BuildOptions, build_dat_from_text};

#[derive(Debug, Parser)]
#[command(name = "strfile")]
#[command(about = "Build fortune-mod .dat index files")]
struct Args {
    #[arg(short = 'c', default_value = "%")]
    delimiter: String,
    #[arg(short = 'r', action = ArgAction::SetTrue)]
    randomize_offsets: bool,
    #[arg(short = 'o', action = ArgAction::SetTrue)]
    order_offsets: bool,
    #[arg(short = 's', action = ArgAction::SetTrue)]
    silent: bool,
    #[arg(long = "allow-empty", action = ArgAction::SetTrue)]
    allow_empty: bool,
    #[arg(value_name = "INPUT")]
    input: PathBuf,
    #[arg(value_name = "OUTPUT")]
    output: Option<PathBuf>,
}

fn main() {
    init_logging("warn,rustune=info,strfile=info");
    let args = Args::parse();
    if let Err(err) = run(args) {
        eprintln!("strfile: {err:#}");
        std::process::exit(1);
    }
}

#[instrument(skip_all)]
fn run(args: Args) -> Result<()> {
    let delimiter = parse_delimiter(&args.delimiter)?;
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| dat_path_for_text(&args.input));

    let input_bytes = fs::read(&args.input)
        .with_context(|| format!("failed to read {}", args.input.display()))?;
    let options = BuildOptions {
        delimiter,
        randomize_offsets: args.randomize_offsets,
        order_offsets: args.order_offsets,
        allow_empty: args.allow_empty,
    };
    let (dat, stats) = build_dat_from_text(&input_bytes, options)?;
    dat.write_to_path(&output)?;

    debug!(
        output = %output.display(),
        record_count = stats.record_count,
        "wrote STRFILE index"
    );

    if !args.silent {
        println!(
            "\"{}\" created\n{} strings\nlongest string: {} bytes\nshortest string: {} bytes",
            output.display(),
            stats.record_count,
            stats.longest_record,
            stats.shortest_record
        );
    }
    Ok(())
}

fn parse_delimiter(value: &str) -> Result<u8> {
    let bytes = value.as_bytes();
    if bytes.len() != 1 {
        bail!("delimiter must be a single byte, got '{value}'");
    }
    Ok(bytes[0])
}
