use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};
use clap::{ArgAction, Parser};
use regex::RegexBuilder;
use tracing::{debug, info, instrument, warn};

use rustune::datfile::LengthFilter;
use rustune::discovery::{DiscoveryConfig, discover_weighted_sources};
use rustune::fortune_engine::{
    FileSelectionMode, LoadedSource, calculate_probabilities, collect_matches, load_sources,
    select_random_fortune,
};
use rustune::logging::init_logging;
use rustune::rng::FortuneRng;
use rustune::sources::{SourceSpec, parse_source_specs};

const MIN_WAIT_SECONDS: usize = 6;
const CHARS_PER_SECOND: usize = 20;

#[derive(Debug, Parser)]
#[command(name = "rustune")]
#[command(about = "Rust port of fortune-mod")]
#[command(disable_version_flag = true)]
struct Args {
    #[arg(short = 'a', long = "all", action = ArgAction::SetTrue)]
    allow_any: bool,
    #[arg(short = 'o', long = "offensive", action = ArgAction::SetTrue)]
    offensive_only: bool,
    #[arg(short = 'e', long = "equal", action = ArgAction::SetTrue)]
    equal_probability: bool,
    #[arg(short = 'f', long = "files", action = ArgAction::SetTrue)]
    list_files: bool,
    #[arg(short = 'l', long = "long", action = ArgAction::SetTrue, conflicts_with = "short_only")]
    long_only: bool,
    #[arg(short = 's', long = "short", action = ArgAction::SetTrue, conflicts_with = "long_only")]
    short_only: bool,
    #[arg(short = 'n', long = "length", default_value_t = 160)]
    length: usize,
    #[arg(short = 'm', long = "match")]
    pattern: Option<String>,
    #[arg(short = 'i', long = "ignore-case", action = ArgAction::SetTrue)]
    ignore_case: bool,
    #[arg(short = 'w', long = "wait", action = ArgAction::SetTrue)]
    wait: bool,
    #[arg(short = 'c', long = "show-source", action = ArgAction::SetTrue)]
    show_source: bool,
    #[arg(short = 'u', long = "no-recode", action = ArgAction::SetTrue)]
    no_recode: bool,
    #[arg(short = 'v', long = "version", action = ArgAction::SetTrue)]
    version_only: bool,
    #[arg(long = "verbose", action = ArgAction::SetTrue)]
    verbose: bool,
    #[arg(value_name = "SOURCE")]
    sources: Vec<String>,
}

fn main() {
    let args = Args::parse();
    init_logging(args.verbose, "warn,rustune=info");
    if let Err(err) = run(args) {
        eprintln!("rustune: {err:#}");
        std::process::exit(1);
    }
}

#[instrument(skip_all)]
fn run(args: Args) -> Result<()> {
    if args.version_only {
        println!("rustune {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if args.no_recode {
        warn!("-u/--no-recode requested; locale recode parity is not yet implemented");
    }

    if args.ignore_case && args.pattern.is_none() {
        bail!("-i requires -m <pattern>");
    }

    let source_specs = parse_source_specs(&args.sources)?;
    let discovery_cfg = DiscoveryConfig {
        allow_any: args.allow_any,
        offensive_only: args.offensive_only,
    };
    let discovered = discover_weighted_sources(&source_specs, &discovery_cfg)?;
    let length_filter = compute_length_filter(args.short_only, args.long_only, args.length);
    let loaded = load_sources(&discovered, length_filter)?;
    let probabilities = calculate_probabilities(&loaded, args.equal_probability)?;

    if args.list_files {
        print_probabilities(&source_specs, &loaded, &probabilities)?;
        return Ok(());
    }

    if let Some(pattern) = &args.pattern {
        let matcher = RegexBuilder::new(pattern)
            .case_insensitive(args.ignore_case)
            .build()?;
        let matches = collect_matches(&loaded, &matcher)?;
        if matches.is_empty() {
            return Ok(());
        }

        let mut announced = BTreeSet::new();
        for matched in matches {
            if announced.insert(matched.source_path.clone()) {
                eprintln!("{}", matched.source_path.display());
            }
            print_record(&matched.text)?;
            println!("%");
        }
        return Ok(());
    }

    let mut rng = FortuneRng::from_env()?;
    let selection_mode =
        if args.equal_probability || loaded.iter().any(|entry| entry.explicit_percent.is_some()) {
            FileSelectionMode::ProbabilityPercent
        } else {
            FileSelectionMode::CandidateCount
        };
    let selection = select_random_fortune(&loaded, &probabilities, &mut rng, selection_mode)?;

    if args.show_source {
        println!(
            "({})",
            absolute_display_path(&selection.source_path).display()
        );
        println!("%");
    }
    print_record(&selection.text)?;
    info!(
        source = %selection.source_path.display(),
        index = selection.record_index,
        "fortune emitted"
    );

    if args.wait {
        let sleep_s = wait_seconds_for_text(&selection.text);
        debug!(sleep_s, "sleeping for -w output pacing");
        if sleep_s > 0 {
            thread::sleep(Duration::from_secs(sleep_s as u64));
        }
    }

    Ok(())
}

fn compute_length_filter(short_only: bool, long_only: bool, threshold: usize) -> LengthFilter {
    if short_only {
        LengthFilter::Short { threshold }
    } else if long_only {
        LengthFilter::Long { threshold }
    } else {
        LengthFilter::Any
    }
}

fn print_probabilities(
    source_specs: &[SourceSpec],
    loaded: &[LoadedSource],
    probabilities: &[f64],
) -> Result<()> {
    let mut err = io::stderr().lock();

    if source_specs.len() == 1 && source_specs[0].path.is_dir() {
        let total: f64 = probabilities.iter().sum();
        let top = absolute_display_path(&source_specs[0].path);
        writeln!(err, "{:.2}% {}", total, top.display())?;

        for (entry, probability) in loaded.iter().zip(probabilities.iter()) {
            let rel = if total > 0.0 {
                (probability / total) * 100.0
            } else {
                0.0
            };
            let label = entry
                .db
                .text_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| entry.db.text_path.display().to_string());
            writeln!(err, "    {:.2}% {}", rel, label)?;
        }
        return Ok(());
    }

    for (entry, probability) in loaded.iter().zip(probabilities.iter()) {
        let abs = absolute_display_path(&entry.db.text_path);
        writeln!(err, "{:.2}% {}", probability, abs.display())?;
    }
    Ok(())
}

fn print_record(text: &str) -> Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(text.as_bytes())?;
    if !text.ends_with('\n') {
        out.write_all(b"\n")?;
    }
    Ok(())
}

fn wait_seconds_for_text(text: &str) -> usize {
    let chars = text.chars().count();
    let cps = chars.div_ceil(CHARS_PER_SECOND);
    cps.max(MIN_WAIT_SECONDS)
}

fn absolute_display_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}
