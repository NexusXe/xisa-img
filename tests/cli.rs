//! The `xisa-img` binary: seal, check, refusals, and exit statuses.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use xisa_img::{CONTRACT_VERSION, HEADER_SIZE, Header, MACHINE, MIN_ALIGN, MajorMinorVersionPair};

fn tool(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xisa-img"))
        .args(args)
        .output()
        .expect("run xisa-img")
}

/// A 256-byte unsealed image (file size and CRC left zero, as a link
/// leaves them) at `name` in the test's scratch directory.
fn unsealed(name: &str, version: MajorMinorVersionPair) -> PathBuf {
    let h = Header {
        version,
        file_size: 0,
        memory_size: 0x2000,
        entry: 64,
        align: MIN_ALIGN,
        flags: MACHINE,
        crc: 0,
    };
    let mut b = vec![0x5A; 256];
    b[..HEADER_SIZE].copy_from_slice(&h.bytes());
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&p, b).expect("write the image");
    p
}

fn s(p: &Path) -> &str {
    p.to_str().expect("utf-8 path")
}

#[test]
fn seal_in_place_then_check() {
    let p = unsealed("in_place.img", CONTRACT_VERSION);
    let o = tool(&["seal", s(&p)]);
    assert!(o.status.success(), "{o:?}");
    let o = tool(&["check", s(&p)]);
    assert!(o.status.success(), "{o:?}");
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(text.contains("file size 0x100"), "{text}");
    assert!(text.contains("levels machine"), "{text}");
}

#[test]
fn seal_to_another_file_leaves_the_input() {
    let p = unsealed("input.img", CONTRACT_VERSION);
    let out = p.with_file_name("output.img");
    let o = tool(&["seal", s(&p), "-o", s(&out)]);
    assert!(o.status.success(), "{o:?}");
    assert_eq!(tool(&["check", s(&out)]).status.code(), Some(0));
    // The input is still unsealed: its CRC is not the file's.
    assert_eq!(tool(&["check", s(&p)]).status.code(), Some(1));
}

#[test]
fn refusals_exit_1() {
    let p = unsealed("corrupt.img", CONTRACT_VERSION);
    assert!(tool(&["seal", s(&p)]).status.success());
    let mut b = std::fs::read(&p).unwrap();
    b[200] ^= 1;
    std::fs::write(&p, b).unwrap();
    let o = tool(&["check", s(&p)]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8(o.stderr).unwrap().contains("CRC-32C"));
}

#[test]
fn another_contract_version_is_refused() {
    let p = unsealed(
        "old.img",
        MajorMinorVersionPair {
            major: CONTRACT_VERSION.major,
            minor: CONTRACT_VERSION.minor - 1,
        },
    );
    let o = tool(&["seal", s(&p)]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        String::from_utf8(o.stderr)
            .unwrap()
            .contains("contract version")
    );
}

#[test]
fn usage_and_file_errors_exit_2() {
    assert_eq!(tool(&[]).status.code(), Some(2));
    assert_eq!(tool(&["frob", "x"]).status.code(), Some(2));
    assert_eq!(
        tool(&["check", "/nonexistent/x.img"]).status.code(),
        Some(2)
    );
}

#[test]
fn version_names_the_contract() {
    let o = tool(&["--version"]);
    assert!(o.status.success());
    let text = String::from_utf8(o.stdout).unwrap();
    assert_eq!(
        text.trim(),
        format!(
            "xisa-img {} (platform contract {}.{})",
            env!("CARGO_PKG_VERSION"),
            CONTRACT_VERSION.major,
            CONTRACT_VERSION.minor
        )
    );
}
