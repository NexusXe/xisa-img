//! `xisa-img`: the standard image's host tool (manual 14.5).
//!
//! ```text
//! xisa-img seal <image> [-o <out>]  fill in the file size and the CRC-32C,
//!                                   the two fields a link cannot compute,
//!                                   then check the image (in place
//!                                   without -o)
//! xisa-img check <image>            print the header, or why a loader
//!                                   refuses the image
//! xisa-img --version                this tool's version and the contract
//!                                   version it seals for
//! ```
//!
//! The link lays out the rest of the header, and `llvm-objcopy -O binary`
//! makes the flat file. Both commands also refuse a contract version other
//! than this crate's (0.x versions match exactly, 14.1). Exit status 0 for
//! a good image, 1 for a refused one, and 2 for a usage or file error.
#![warn(clippy::pedantic, clippy::nursery)]

use std::process::ExitCode;

use xisa_img::{CONTRACT_VERSION, Header, MACHINE, MajorMinorVersionPair, SUPERVISOR, check, seal};

fn version(v: MajorMinorVersionPair) -> String {
    format!("{}.{}", v.major, v.minor)
}

fn accept(h: &Header) -> Result<(), String> {
    if h.version == CONTRACT_VERSION {
        Ok(())
    } else {
        Err(format!(
            "contract version {}, not {}",
            version(h.version),
            version(CONTRACT_VERSION)
        ))
    }
}

fn describe(h: &Header) -> String {
    let mut levels = Vec::new();
    if h.flags & MACHINE != 0 {
        levels.push("machine");
    }
    if h.flags & SUPERVISOR != 0 {
        levels.push("supervisor");
    }
    format!(
        "contract {}\nfile size {:#x}\nmemory size {:#x}\nentry offset {:#x}\n\
         load alignment {:#x}\nlevels {}\ncrc32c {:#010x}",
        version(h.version),
        h.file_size,
        h.memory_size,
        h.entry,
        h.align,
        levels.join(", "),
        h.crc
    )
}

enum Fail {
    Usage(String),
    Refused(String),
}

fn run() -> Result<String, Fail> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = || {
        Fail::Usage(
            "usage: xisa-img seal <image> [-o <out>] | xisa-img check <image> | xisa-img --version"
                .into(),
        )
    };
    if let [v] = args.as_slice()
        && v == "--version"
    {
        return Ok(format!(
            "xisa-img {} (platform contract {})",
            env!("CARGO_PKG_VERSION"),
            version(CONTRACT_VERSION)
        ));
    }
    let (cmd, path) = match args.as_slice() {
        [c, p, ..] => (c.as_str(), p.as_str()),
        _ => return Err(usage()),
    };
    let out = match &args[2..] {
        [] => None,
        [o, f] if cmd == "seal" && o == "-o" => Some(f.as_str()),
        _ => return Err(usage()),
    };
    let mut bytes = std::fs::read(path).map_err(|e| Fail::Usage(format!("{path}: {e}")))?;
    let refused = |e: &dyn std::fmt::Display| Fail::Refused(format!("{path}: refused: {e}"));
    match cmd {
        "seal" => {
            let h = seal(&mut bytes).map_err(|e| refused(&e))?;
            accept(&h).map_err(|e| refused(&e))?;
            let dest = out.unwrap_or(path);
            std::fs::write(dest, &bytes).map_err(|e| Fail::Usage(format!("{dest}: {e}")))?;
            Ok(format!("{dest}: sealed, crc32c {:#010x}", h.crc))
        }
        "check" => {
            let h = check(&bytes).map_err(|e| refused(&e))?;
            accept(&h).map_err(|e| refused(&e))?;
            Ok(describe(&h))
        }
        _ => Err(usage()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(Fail::Refused(s)) => {
            eprintln!("xisa-img: {s}");
            ExitCode::from(1)
        }
        Err(Fail::Usage(s)) => {
            eprintln!("xisa-img: {s}");
            ExitCode::from(2)
        }
    }
}
