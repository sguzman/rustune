use std::fs;
use std::process::Command;

use tempfile::tempdir;

use rustune::strfile_builder::{BuildOptions, build_dat_from_text};

fn write_indexed_file(path: &std::path::Path, text: &[u8]) {
    fs::write(path, text).expect("write fortune text");
    let (dat, _) = build_dat_from_text(text, BuildOptions::default()).expect("build dat");
    dat.write_to_path(&std::path::PathBuf::from(format!("{}.dat", path.display())))
        .expect("write dat");
}

#[test]
fn list_files_writes_formatted_probabilities_to_stderr() {
    let tmp = tempdir().expect("tempdir");
    let alpha = tmp.path().join("alpha");
    let beta = tmp.path().join("beta");
    write_indexed_file(
        &alpha,
        b"Rust keeps moving.\n%\nParsers should be strict.\n%\nLogs are your friend.\n",
    );
    write_indexed_file(
        &beta,
        b"Small binaries, sharp tools.\n%\nParity first, modern internals.\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rustune"))
        .arg("-f")
        .arg(&alpha)
        .arg(&beta)
        .output()
        .expect("run fortune");
    assert!(output.status.success());
    assert!(output.stdout.is_empty());

    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    let alpha_abs = fs::canonicalize(&alpha).expect("alpha abs");
    let beta_abs = fs::canonicalize(&beta).expect("beta abs");
    let expected = format!(
        "60.00% {}\n40.00% {}\n",
        alpha_abs.display(),
        beta_abs.display()
    );
    assert_eq!(stderr, expected);
}

#[test]
fn deterministic_seed_matches_expected_selection() {
    let tmp = tempdir().expect("tempdir");
    let alpha = tmp.path().join("alpha");
    let beta = tmp.path().join("beta");
    write_indexed_file(
        &alpha,
        b"Rust keeps moving.\n%\nParsers should be strict.\n%\nLogs are your friend.\n",
    );
    write_indexed_file(
        &beta,
        b"Small binaries, sharp tools.\n%\nParity first, modern internals.\n",
    );

    let out0 = Command::new(env!("CARGO_BIN_EXE_rustune"))
        .env("FORTUNE_MOD_RAND_HARD_CODED_VALS", "0")
        .arg(&alpha)
        .arg(&beta)
        .output()
        .expect("run fortune seed 0");
    assert!(out0.status.success());
    assert_eq!(
        String::from_utf8(out0.stdout).expect("stdout"),
        "Parsers should be strict.\n"
    );

    let out1 = Command::new(env!("CARGO_BIN_EXE_rustune"))
        .env("FORTUNE_MOD_RAND_HARD_CODED_VALS", "1")
        .arg("-e")
        .arg(&alpha)
        .arg(&beta)
        .arg("-n")
        .arg("120")
        .output()
        .expect("run fortune seed 1");
    assert!(out1.status.success());
    assert_eq!(
        String::from_utf8(out1.stdout).expect("stdout"),
        "Logs are your friend.\n"
    );
}

#[test]
fn show_source_prints_banner_with_separator() {
    let tmp = tempdir().expect("tempdir");
    let alpha = tmp.path().join("alpha");
    write_indexed_file(
        &alpha,
        b"Rust keeps moving.\n%\nParsers should be strict.\n%\nLogs are your friend.\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rustune"))
        .env("FORTUNE_MOD_RAND_HARD_CODED_VALS", "0")
        .arg("-c")
        .arg(&alpha)
        .output()
        .expect("run fortune -c");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    let alpha_abs = fs::canonicalize(&alpha).expect("alpha abs");
    let expected = format!("({})\n%\nParsers should be strict.\n", alpha_abs.display());
    assert_eq!(stdout, expected);
}
