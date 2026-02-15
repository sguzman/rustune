use std::fs;

use tempfile::tempdir;

use rustune::datfile::FortuneFile;
use rustune::strfile_builder::{BuildOptions, build_dat_from_text};

#[test]
fn build_dat_and_reopen_records() {
    let tmp = tempdir().expect("tempdir");
    let text_path = tmp.path().join("sample");
    let dat_path = tmp.path().join("sample.dat");
    let text = b"first fortune\n%\nsecond fortune\n";
    fs::write(&text_path, text).expect("write text file");

    let (dat, stats) = build_dat_from_text(text, BuildOptions::default()).expect("build");
    dat.write_to_path(&dat_path).expect("write dat");
    assert_eq!(stats.record_count, 2);

    let opened = FortuneFile::open(&text_path).expect("open fortune file");
    assert_eq!(opened.record_count(), 2);
    assert_eq!(
        opened.record_text_lossy(0).expect("record 0"),
        "first fortune\n"
    );
    assert_eq!(
        opened.record_text_lossy(1).expect("record 1"),
        "second fortune\n"
    );
}
