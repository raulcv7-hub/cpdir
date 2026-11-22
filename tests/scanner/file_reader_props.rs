//! Property-based tests verifying bounded UTF-8 ingestion over arbitrary disk byte streams.

use cpdir::config::limits::HARD_READ_BYTE_CAP;
use cpdir::domain::FileDiscardReason;
use cpdir::scanner::{ReadFileOutcome, read_bounded_utf8};
use proptest::prelude::*;

use crate::common::workspace::WorkspaceBuilder;

proptest! {
    #[test]
    fn arbitrary_disk_bytes_match_strict_bounded_utf8_contract(
        bytes in prop::collection::vec(any::<u8>(), 0..3_000),
    ) {
        let ws = WorkspaceBuilder::new();
        let path = ws.write_bytes("fuzz.txt", &bytes);
        let cap = 2_048u64;
        let outcome = read_bounded_utf8(&path, cap);

        if bytes.len() > 2_048 {
            prop_assert_eq!(
                outcome,
                ReadFileOutcome::Discarded(FileDiscardReason::ExceedsHardCap {
                    byte_len: u64::try_from(bytes.len()).expect("len fits u64"),
                })
            );
        } else if bytes.starts_with(&[0xFF, 0xFE])
            || bytes.starts_with(&[0xFE, 0xFF])
            || bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF])
        {
            prop_assert_eq!(
                outcome,
                ReadFileOutcome::Discarded(FileDiscardReason::InvalidUtf8)
            );
        } else if bytes.contains(&0) {
            prop_assert_eq!(outcome, ReadFileOutcome::Discarded(FileDiscardReason::Binary));
        } else if let Ok(text) = std::str::from_utf8(&bytes) {
            let expected = text.strip_prefix('\u{FEFF}').unwrap_or(text).to_owned();
            prop_assert_eq!(
                outcome,
                ReadFileOutcome::Text {
                    byte_len: u64::try_from(bytes.len()).expect("len fits u64"),
                    content: expected,
                }
            );
        } else {
            prop_assert_eq!(
                outcome,
                ReadFileOutcome::Discarded(FileDiscardReason::InvalidUtf8)
            );
        }
    }

    #[test]
    fn arbitrary_valid_utf8_without_nulls_roundtrips_literally_with_optional_bom(
        raw_text in any::<String>(),
        prepend_bom in any::<bool>(),
    ) {
        let clean = raw_text.replace('\0', "");
        let on_disk = if prepend_bom {
            format!("\u{FEFF}{clean}")
        } else {
            clean.clone()
        };
        let ws = WorkspaceBuilder::new();
        let path = ws.write_text("roundtrip.rs", &on_disk);
        let outcome = read_bounded_utf8(&path, HARD_READ_BYTE_CAP);
        let expected = on_disk.strip_prefix('\u{FEFF}').unwrap_or(&on_disk).to_owned();

        prop_assert_eq!(
            outcome,
            ReadFileOutcome::Text {
                byte_len: u64::try_from(on_disk.len()).expect("len fits u64"),
                content: expected,
            }
        );
    }
}
