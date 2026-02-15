use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Serialize;
use tracing::{debug, instrument, warn};

use rustune::logging::init_logging;

#[derive(Debug, Parser)]
#[command(name = "fortune-parity")]
#[command(about = "Oracle-based parity harness for fortune")]
struct Args {
    #[arg(long, default_value = "/usr/bin/fortune")]
    oracle: PathBuf,
    #[arg(long)]
    subject: Option<PathBuf>,
    #[arg(long)]
    strfile: Option<PathBuf>,
    #[arg(long, default_value = "tests/corpus")]
    corpus_dir: PathBuf,
    #[arg(long)]
    json_out: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    verbose: bool,
}

#[derive(Debug, Clone)]
struct Case {
    name: &'static str,
    category: &'static str,
    args: Vec<String>,
    seed: Option<u64>,
}

#[derive(Debug, Serialize)]
struct CaseResult {
    name: String,
    category: String,
    pass: bool,
    oracle_status: Option<i32>,
    subject_status: Option<i32>,
    args: Vec<String>,
    diff_excerpt: Option<String>,
}

#[derive(Debug, Serialize)]
struct CategoryScore {
    total: usize,
    passed: usize,
    weight: f64,
    weighted_score: f64,
}

#[derive(Debug, Serialize)]
struct ParityReport {
    oracle: String,
    subject: String,
    total_cases: usize,
    passed_cases: usize,
    weighted_percent: f64,
    category_scores: BTreeMap<String, CategoryScore>,
    results: Vec<CaseResult>,
}

fn main() {
    let args = Args::parse();
    init_logging(args.verbose, "warn,rustune=info,fortune_parity=info");
    if let Err(err) = run(args) {
        eprintln!("fortune-parity: {err:#}");
        std::process::exit(1);
    }
}

#[instrument(skip_all)]
fn run(args: Args) -> Result<()> {
    let subject = resolve_subject_path(args.subject)?;
    let strfile = resolve_strfile_path(args.strfile, &subject);
    ensure_corpus_dat_files(&args.corpus_dir, &strfile)?;

    if !args.oracle.is_file() {
        bail!("oracle binary '{}' does not exist", args.oracle.display());
    }
    if !subject.is_file() {
        bail!("subject binary '{}' does not exist", subject.display());
    }

    let cases = build_cases(&args.corpus_dir);
    let mut results = Vec::with_capacity(cases.len());
    for case in cases {
        results.push(run_case(&args.oracle, &subject, &case)?);
    }

    let report = build_report(&args.oracle, &subject, results);
    print_markdown_report(&report);
    if let Some(out) = args.json_out {
        let json = serde_json::to_string_pretty(&report)?;
        fs::write(&out, json).with_context(|| format!("failed writing {}", out.display()))?;
    }
    Ok(())
}

fn resolve_subject_path(subject: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = subject {
        return Ok(path);
    }
    let current_exe = std::env::current_exe()?;
    let sibling = current_exe
        .parent()
        .map(|p| p.join("rustune"))
        .ok_or_else(|| anyhow::anyhow!("cannot derive subject rustune path"))?;
    Ok(sibling)
}

fn resolve_strfile_path(strfile: Option<PathBuf>, subject: &Path) -> PathBuf {
    if let Some(path) = strfile {
        path
    } else {
        subject
            .parent()
            .map(|p| p.join("strfile"))
            .unwrap_or_else(|| PathBuf::from("strfile"))
    }
}

#[instrument(skip_all)]
fn ensure_corpus_dat_files(corpus_dir: &Path, strfile_bin: &Path) -> Result<()> {
    if !corpus_dir.exists() {
        fs::create_dir_all(corpus_dir)
            .with_context(|| format!("failed creating {}", corpus_dir.display()))?;
    }
    write_default_corpus(corpus_dir)?;

    for entry in fs::read_dir(corpus_dir)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.ends_with(".dat") || name.ends_with(".u8") {
            continue;
        }
        let dat = PathBuf::from(format!("{}.dat", path.display()));
        if dat.exists() {
            continue;
        }
        debug!(file = %path.display(), "creating missing corpus .dat file");
        let status = Command::new(strfile_bin)
            .arg("-s")
            .arg(&path)
            .arg(&dat)
            .status()
            .with_context(|| format!("failed running {}", strfile_bin.display()))?;
        if !status.success() {
            bail!("failed generating dat for {}", path.display());
        }
    }
    Ok(())
}

fn write_default_corpus(corpus_dir: &Path) -> Result<()> {
    let alpha = corpus_dir.join("alpha");
    let beta = corpus_dir.join("beta");
    if !alpha.exists() {
        fs::write(
            &alpha,
            b"Rust keeps moving.\n%\nParsers should be strict.\n%\nLogs are your friend.\n",
        )?;
    }
    if !beta.exists() {
        fs::write(
            &beta,
            b"Small binaries, sharp tools.\n%\nParity first, modern internals.\n",
        )?;
    }
    Ok(())
}

fn build_cases(corpus_dir: &Path) -> Vec<Case> {
    let alpha = corpus_dir.join("alpha").display().to_string();
    let beta = corpus_dir.join("beta").display().to_string();
    let directory = corpus_dir.display().to_string();

    vec![
        Case {
            name: "list files",
            category: "cli_parse",
            args: vec!["-f".to_string(), alpha.clone(), beta.clone()],
            seed: None,
        },
        Case {
            name: "directory discovery",
            category: "file_discovery",
            args: vec!["-f".to_string(), directory],
            seed: None,
        },
        Case {
            name: "selection seed 0",
            category: "selection_semantics",
            args: vec![alpha.clone(), beta.clone()],
            seed: Some(0),
        },
        Case {
            name: "selection equal seed 1",
            category: "selection_semantics",
            args: vec![
                "-e".to_string(),
                alpha.clone(),
                beta.clone(),
                "-n".to_string(),
                "120".to_string(),
            ],
            seed: Some(1),
        },
        Case {
            name: "dat reading short mode",
            category: "dat_reading",
            args: vec!["-s".to_string(), "-n".to_string(), "24".to_string(), alpha],
            seed: Some(2),
        },
        Case {
            name: "regex mode",
            category: "regex_mode",
            args: vec!["-m".to_string(), "Rust".to_string(), beta],
            seed: None,
        },
        Case {
            name: "strfile compatibility",
            category: "strfile_output",
            args: vec![
                "-f".to_string(),
                corpus_dir.join("alpha").display().to_string(),
            ],
            seed: None,
        },
    ]
}

#[instrument(skip_all, fields(case = case.name))]
fn run_case(oracle: &Path, subject: &Path, case: &Case) -> Result<CaseResult> {
    let oracle_out = run_single(oracle, &case.args, case.seed)
        .with_context(|| format!("oracle failed for case '{}'", case.name))?;
    let subject_out = run_single(subject, &case.args, case.seed)
        .with_context(|| format!("subject failed for case '{}'", case.name))?;

    let same_status = oracle_out.status == subject_out.status;
    let same_stdout = oracle_out.stdout == subject_out.stdout;
    let same_stderr = oracle_out.stderr == subject_out.stderr;
    let pass = same_status && same_stdout && same_stderr;
    let diff_excerpt = if pass {
        None
    } else {
        Some(build_diff_excerpt(
            &oracle_out.stdout,
            &subject_out.stdout,
            &oracle_out.stderr,
            &subject_out.stderr,
        ))
    };
    Ok(CaseResult {
        name: case.name.to_string(),
        category: case.category.to_string(),
        pass,
        oracle_status: oracle_out.status,
        subject_status: subject_out.status,
        args: case.args.clone(),
        diff_excerpt,
    })
}

struct CommandOutput {
    status: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_single(binary: &Path, args: &[String], seed: Option<u64>) -> Result<CommandOutput> {
    let mut cmd = Command::new(binary);
    cmd.args(args);
    if let Some(seed) = seed {
        cmd.env("FORTUNE_MOD_RAND_HARD_CODED_VALS", seed.to_string());
    }
    let out = cmd.output()?;
    Ok(CommandOutput {
        status: out.status.code(),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

fn build_diff_excerpt(
    oracle_stdout: &[u8],
    subject_stdout: &[u8],
    oracle_stderr: &[u8],
    subject_stderr: &[u8],
) -> String {
    let oracle_stdout = String::from_utf8_lossy(oracle_stdout);
    let subject_stdout = String::from_utf8_lossy(subject_stdout);
    let oracle_stderr = String::from_utf8_lossy(oracle_stderr);
    let subject_stderr = String::from_utf8_lossy(subject_stderr);

    format!(
        "stdout oracle={:?} subject={:?}; stderr oracle={:?} subject={:?}",
        truncate(&oracle_stdout, 80),
        truncate(&subject_stdout, 80),
        truncate(&oracle_stderr, 80),
        truncate(&subject_stderr, 80)
    )
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_string()
    } else {
        format!("{}...", &value[..max])
    }
}

fn build_report(oracle: &Path, subject: &Path, results: Vec<CaseResult>) -> ParityReport {
    let weights = category_weights();
    let mut grouped: BTreeMap<String, Vec<&CaseResult>> = BTreeMap::new();
    for result in &results {
        grouped
            .entry(result.category.clone())
            .or_default()
            .push(result);
    }

    let mut category_scores = BTreeMap::new();
    let mut weighted_percent = 0.0_f64;
    for (category, cases) in grouped {
        let passed = cases.iter().filter(|c| c.pass).count();
        let total = cases.len();
        let weight = *weights.get(category.as_str()).unwrap_or(&0.0);
        let ratio = if total == 0 {
            0.0
        } else {
            (passed as f64) / (total as f64)
        };
        let weighted_score = ratio * weight;
        weighted_percent += weighted_score;
        category_scores.insert(
            category,
            CategoryScore {
                total,
                passed,
                weight,
                weighted_score,
            },
        );
    }

    let passed_cases = results.iter().filter(|r| r.pass).count();
    ParityReport {
        oracle: oracle.display().to_string(),
        subject: subject.display().to_string(),
        total_cases: results.len(),
        passed_cases,
        weighted_percent,
        category_scores,
        results,
    }
}

fn category_weights() -> BTreeMap<&'static str, f64> {
    BTreeMap::from([
        ("cli_parse", 10.0),
        ("file_discovery", 20.0),
        ("dat_reading", 20.0),
        ("selection_semantics", 25.0),
        ("regex_mode", 15.0),
        ("strfile_output", 10.0),
    ])
}

fn print_markdown_report(report: &ParityReport) {
    println!("# fortune parity report");
    println!();
    println!("- oracle: `{}`", report.oracle);
    println!("- subject: `{}`", report.subject);
    println!("- passed: {}/{}", report.passed_cases, report.total_cases);
    println!("- weighted parity: {:.2}%", report.weighted_percent);
    println!();
    println!("## category scores");
    for (name, score) in &report.category_scores {
        println!(
            "- {}: {}/{} (weight {:.1}, score {:.2})",
            name, score.passed, score.total, score.weight, score.weighted_score
        );
    }
    println!();
    println!("## failures");
    let mut failures = 0usize;
    for case in report.results.iter().filter(|c| !c.pass).take(10) {
        failures += 1;
        println!(
            "- {} [{}]: {}",
            case.name,
            case.category,
            case.diff_excerpt.as_deref().unwrap_or("output mismatch")
        );
    }
    if failures == 0 {
        warn!("all parity harness cases passed");
        println!("- none");
    }
}
