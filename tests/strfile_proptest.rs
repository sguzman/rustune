use proptest::prelude::*;

use rustune::datfile::DatFile;
use rustune::strfile_builder::{BuildOptions, build_dat_from_text};

proptest! {
    #[test]
    fn dat_roundtrip_for_generated_records(records in proptest::collection::vec("[A-Za-z0-9 .,!?]{1,50}", 1..30)) {
        let mut text = Vec::new();
        for (idx, record) in records.iter().enumerate() {
            text.extend_from_slice(record.as_bytes());
            text.push(b'\n');
            if idx + 1 != records.len() {
                text.extend_from_slice(b"%\n");
            }
        }

        let (dat, stats) = build_dat_from_text(&text, BuildOptions::default()).expect("build dat");
        prop_assert_eq!(stats.record_count, records.len());
        prop_assert_eq!(dat.offsets.len(), records.len());
        prop_assert!(dat.offsets.windows(2).all(|w| w[0] < w[1]));

        let encoded = dat.to_bytes().expect("encode dat");
        let decoded = DatFile::read_from_bytes(&encoded).expect("decode dat");
        prop_assert_eq!(decoded.offsets, dat.offsets);
        prop_assert_eq!(decoded.header.numstr, records.len() as u32);
    }
}
