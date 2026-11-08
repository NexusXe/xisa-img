//! The xisa standard image (manual 14.5): a flat binary that starts with a
//! 64-byte header, which a loader places at any multiple of its load
//! alignment. The checks here are the ones every loader makes; the
//! contract version and the handoff level depend on the loader and are
//! left to it.
//!
//! `no_std` and dependency-free: firmware, simulators, and host tools share
//! it. The `xisa-img` binary seals and checks images on the host.
#![no_std]
#![feature(const_trait_impl)]
#![feature(const_convert)]
#![warn(clippy::pedantic, clippy::nursery)]

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MajorMinorVersionPair {
    pub major: u32,
    pub minor: u32,
}

#[allow(clippy::many_single_char_names, reason = "sorry")]
impl MajorMinorVersionPair {
    #[must_use]
    pub const fn from_be_bytes(b: [u8; 8]) -> Self {
        Self {
            major: u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            minor: u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
        }
    }
    #[must_use]
    pub const fn from_le_bytes(b: [u8; 8]) -> Self {
        Self {
            major: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            minor: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        }
    }
    #[must_use]
    pub const fn from_ne_bytes(b: [u8; 8]) -> Self {
        if cfg!(target_endian = "little") {
            Self::from_le_bytes(b)
        } else {
            Self::from_be_bytes(b)
        }
    }

    #[must_use]
    pub const fn to_be_bytes(self) -> [u8; 8] {
        let [a, b, c, d] = self.major.to_be_bytes();
        let [e, f, g, h] = self.minor.to_be_bytes();
        [a, b, c, d, e, f, g, h]
    }
    #[must_use]
    pub const fn to_le_bytes(self) -> [u8; 8] {
        let [e, f, g, h] = self.major.to_le_bytes();
        let [a, b, c, d] = self.minor.to_le_bytes();

        [a, b, c, d, e, f, g, h]
    }
    #[must_use]
    pub const fn to_ne_bytes(self) -> [u8; 8] {
        if cfg!(target_endian = "little") {
            self.to_le_bytes()
        } else {
            self.to_be_bytes()
        }
    }
}

const impl From<u64> for MajorMinorVersionPair {
    fn from(v: u64) -> Self {
        Self {
            major: (v >> 32) as u32,
            minor: (v & 0xFFFF_FFFF) as u32,
        }
    }
}

const impl From<MajorMinorVersionPair> for u64 {
    fn from(p: MajorMinorVersionPair) -> Self {
        (p.major as u64) << 32 | p.minor as u64
    }
}

/// The magic at offset 0.
pub const MAGIC: [u8; 8] = *b"XISA_IMG";
/// The header's size and the lowest entry offset.
pub const HEADER_SIZE: usize = 64;
/// The platform contract version this crate seals and accepts.
pub const CONTRACT_VERSION: MajorMinorVersionPair = MajorMinorVersionPair {
    major: parse(env!("CARGO_PKG_VERSION_MAJOR")),
    minor: parse(env!("CARGO_PKG_VERSION_MINOR")),
};
/// Flag bit 0: the image runs at machine level (bare metal).
pub const MACHINE: u64 = 1;
/// Flag bit 1: the image runs at supervisor level (as a guest). A generic
/// image sets both.
pub const SUPERVISOR: u64 = 2;
/// The load alignment's range, 64 KiB to 1 GiB.
pub const MIN_ALIGN: u64 = 64 << 10;
pub const MAX_ALIGN: u64 = 1 << 30;

const FILE_SIZE_AT: usize = 16;
const CRC_AT: usize = 56;

const fn parse(s: &str) -> u32 {
    match konst::primitive::parse_u32(s) {
        Ok(v) => v,
        Err(_) => panic!("version component not a number"),
    }
}

/// The header's fields after the magic, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// The contract version the image was built for.
    pub version: MajorMinorVersionPair,
    /// Bytes in the image file, header included.
    pub file_size: u64,
    /// Bytes the image occupies from its load address.
    pub memory_size: u64,
    /// The entry point's offset from the image's start.
    pub entry: u64,
    pub align: u64,
    /// [`MACHINE`] and [`SUPERVISOR`].
    pub flags: u64,
    /// CRC-32C of the first `file_size` bytes with this field zero.
    pub crc: u64,
}

/// Why a loader refuses an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Shorter than the header.
    Short,
    Magic,
    /// The file size field is not the file's length.
    FileSize {
        header: u64,
        actual: u64,
    },
    /// The memory size is below the file size.
    MemorySize,
    /// The entry offset is not a multiple of 8, below 64, or past the file.
    Entry,
    /// The load alignment is not a power of two from 64 KiB to 1 GiB.
    Align,
    /// No flag bit set, or a bit other than 0 and 1.
    Flags,
    /// The stored CRC is not the file's (or its upper half is not zero).
    Crc {
        stored: u64,
        computed: u32,
    },
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Short => write!(f, "shorter than the {HEADER_SIZE}-byte header"),
            Self::Magic => f.write_str("no XISA_IMG magic"),
            Self::FileSize { header, actual } => {
                write!(f, "file size {header} in the header, {actual} in the file")
            }
            Self::MemorySize => f.write_str("memory size below the file size"),
            Self::Entry => {
                f.write_str("entry offset not a multiple of 8, below 64, or past the file")
            }
            Self::Align => f.write_str("load alignment not a power of two from 64 KiB to 1 GiB"),
            Self::Flags => f.write_str("flags: no level bit set, or a reserved bit set"),
            Self::Crc { stored, computed } => {
                write!(
                    f,
                    "CRC-32C {stored:#x} in the header, {computed:#010x} computed"
                )
            }
        }
    }
}

impl core::error::Error for Error {}

fn field(bytes: &[u8], at: usize) -> u64 {
    let mut w = [0; 8];
    w.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(w)
}

impl Header {
    /// The fields of `bytes`' header, unchecked except for the magic.
    ///
    /// # Errors
    /// [`Error::Short`] or [`Error::Magic`].
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < HEADER_SIZE {
            return Err(Error::Short);
        }
        if bytes[..8] != MAGIC {
            return Err(Error::Magic);
        }
        Ok(Self {
            version: field(bytes, 8).into(),
            file_size: field(bytes, FILE_SIZE_AT),
            memory_size: field(bytes, 24),
            entry: field(bytes, 32),
            align: field(bytes, 40),
            flags: field(bytes, 48),
            crc: field(bytes, CRC_AT),
        })
    }

    /// The header's 64 bytes, magic included.
    #[must_use]
    pub fn bytes(&self) -> [u8; HEADER_SIZE] {
        let mut out = [0; HEADER_SIZE];
        out[..8].copy_from_slice(&MAGIC);
        let fields: [u64; 7] = [
            self.version.into(),
            self.file_size,
            self.memory_size,
            self.entry,
            self.align,
            self.flags,
            self.crc,
        ];
        for (i, v) in fields.iter().enumerate() {
            out[8 + 8 * i..16 + 8 * i].copy_from_slice(&v.to_le_bytes());
        }
        out
    }
}

/// CRC-32C (Castagnoli, polynomial `0x1EDC6F41` reflected), one byte at a
/// time.
const TABLE: [u32; 256] = {
    let mut t = [0; 256];
    let mut i = 0;
    while i < 256 {
        #[allow(clippy::cast_possible_truncation, reason = "below 256")]
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                c >> 1 ^ 0x82F6_3B78
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

fn update(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        crc = crc >> 8 ^ TABLE[((crc ^ u32::from(b)) & 0xFF) as usize];
    }
    crc
}

/// CRC-32C with initial value and final XOR all ones (iSCSI's CRC).
#[must_use]
pub fn crc32c(data: &[u8]) -> u32 {
    !update(!0, data)
}

/// The image's CRC: over `bytes` with the CRC field read as zero.
fn image_crc(bytes: &[u8]) -> u32 {
    let c = update(!0, &bytes[..CRC_AT]);
    let c = update(c, &[0; 8]);
    !update(c, &bytes[HEADER_SIZE..])
}

/// Check `bytes` as every loader does (14.5): the magic, well-formedness,
/// and the CRC. The contract version and the level flags are the
/// loader's to accept.
///
/// # Errors
/// The first rule the image breaks.
pub fn check(bytes: &[u8]) -> Result<Header, Error> {
    let h = Header::read(bytes)?;
    let actual = bytes.len() as u64;
    if h.file_size != actual {
        return Err(Error::FileSize {
            header: h.file_size,
            actual,
        });
    }
    if h.memory_size < h.file_size {
        return Err(Error::MemorySize);
    }
    if h.entry % 8 != 0 || h.entry < HEADER_SIZE as u64 || h.entry >= h.file_size {
        return Err(Error::Entry);
    }
    if !h.align.is_power_of_two() || !(MIN_ALIGN..=MAX_ALIGN).contains(&h.align) {
        return Err(Error::Align);
    }
    if h.flags == 0 || h.flags & !(MACHINE | SUPERVISOR) != 0 {
        return Err(Error::Flags);
    }
    let computed = image_crc(bytes);
    if h.crc != u64::from(computed) {
        return Err(Error::Crc {
            stored: h.crc,
            computed,
        });
    }
    Ok(h)
}

/// Fill in the file size (the length of `bytes`) and the CRC, the two
/// fields a link cannot compute, then check the result.
///
/// # Errors
/// The image breaks a rule the link's fields decide (memory size, entry,
/// alignment, flags), or is too short for a header.
pub fn seal(bytes: &mut [u8]) -> Result<Header, Error> {
    Header::read(bytes)?;
    let len = bytes.len() as u64;
    bytes[FILE_SIZE_AT..FILE_SIZE_AT + 8].copy_from_slice(&len.to_le_bytes());
    bytes[CRC_AT..CRC_AT + 8].copy_from_slice(&[0; 8]);
    let crc = u64::from(image_crc(bytes));
    bytes[CRC_AT..CRC_AT + 8].copy_from_slice(&crc.to_le_bytes());
    check(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The check value every CRC-32C implementation shares.
    #[test]
    fn crc32c_check_value() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
        assert_eq!(crc32c(b""), 0);
    }

    fn image() -> [u8; 128] {
        let h = Header {
            version: CONTRACT_VERSION,
            file_size: 0,
            memory_size: 0x1000,
            entry: 64,
            align: MIN_ALIGN,
            flags: MACHINE,
            crc: 0,
        };
        let mut b = [0xA5; 128];
        b[..HEADER_SIZE].copy_from_slice(&h.bytes());
        b
    }

    #[test]
    fn seal_then_check() {
        let mut b = image();
        let h = seal(&mut b).unwrap();
        assert_eq!((h.file_size, h.memory_size, h.entry), (128, 0x1000, 64));
        assert_eq!(h.crc >> 32, 0);
        // The CRC is the plain CRC-32C of the file with the field zero.
        let mut z = b;
        z[CRC_AT..].fill(0);
        z[HEADER_SIZE..].copy_from_slice(&b[HEADER_SIZE..]);
        assert_eq!(h.crc, u64::from(crc32c(&z)));
        assert_eq!(check(&b), Ok(h));
        assert_eq!(Header::read(&b), Ok(h));
    }

    /// Each rule of 14.5, broken once.
    #[test]
    fn refusals() {
        let sealed = |f: &dyn Fn(&mut [u8; 128])| {
            let mut b = image();
            f(&mut b);
            seal(&mut b)
        };
        let set = |b: &mut [u8; 128], at: usize, v: u64| {
            b[at..at + 8].copy_from_slice(&v.to_le_bytes());
        };
        assert_eq!(check(&image()[..63]), Err(Error::Short));
        assert_eq!(sealed(&|b| b[0] = b'x'), Err(Error::Magic));
        assert_eq!(sealed(&|b| set(b, 24, 127)), Err(Error::MemorySize));
        assert!(sealed(&|b| set(b, 24, 128)).is_ok());
        for e in [0, 56, 60, 72 + 4, 128] {
            assert_eq!(sealed(&|b| set(b, 32, e)), Err(Error::Entry), "{e}");
        }
        assert!(sealed(&|b| set(b, 32, 120)).is_ok());
        for a in [0, MIN_ALIGN / 2, MIN_ALIGN + 8, 3 << 20, MAX_ALIGN * 2] {
            assert_eq!(sealed(&|b| set(b, 40, a)), Err(Error::Align), "{a:#x}");
        }
        assert!(sealed(&|b| set(b, 40, MAX_ALIGN)).is_ok());
        for fl in [0, 4, 1 << 63, SUPERVISOR | 8] {
            assert_eq!(sealed(&|b| set(b, 48, fl)), Err(Error::Flags), "{fl:#x}");
        }
        assert!(sealed(&|b| set(b, 48, MACHINE | SUPERVISOR)).is_ok());

        let mut b = image();
        seal(&mut b).unwrap();
        assert!(matches!(check(&b[..120]), Err(Error::FileSize { .. })));
        for at in [8, 100, 127] {
            let mut c = b;
            c[at] ^= 1;
            assert!(matches!(check(&c), Err(Error::Crc { .. })), "{at}");
        }
        let mut c = b;
        c[CRC_AT + 4] = 1;
        assert!(matches!(check(&c), Err(Error::Crc { .. })), "upper half");
    }
}
