//! NXP frame flags.
//!
//! See RFC §2.3 — flags are a 16-bit bitfield carried in every frame header.

use serde::{Deserialize, Serialize};
use std::fmt;

/// 16-bit frame flag bitfield.
///
/// ```rust
/// use nxp_core::FrameFlags;
/// let f = FrameFlags::ENCRYPTED | FrameFlags::SIGNED;
/// assert!(f.contains(FrameFlags::ENCRYPTED));
/// assert!(f.contains(FrameFlags::SIGNED));
/// assert!(!f.contains(FrameFlags::COMPRESSED));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrameFlags(u16);

impl FrameFlags {
    /// Payload is zstd-compressed before encryption. (Reserved — not yet implemented.)
    pub const COMPRESSED: Self = Self(1 << 0);
    /// Payload is AEAD-encrypted. Always set after session setup.
    pub const ENCRYPTED: Self = Self(1 << 1);
    /// Ed25519 signature is appended to the frame.
    pub const SIGNED: Self = Self(1 << 2);
    /// Last frame of a stream.
    pub const STREAM_END: Self = Self(1 << 3);
    /// Frame carries an error response.
    pub const ERROR: Self = Self(1 << 4);
    /// Payload contains multiple sub-frames.
    pub const BATCHED: Self = Self(1 << 5);
    /// Uses CBOR instead of MessagePack.
    pub const COMPACT: Self = Self(1 << 6);
    /// Sender requests explicit ACK.
    pub const ACK_REQUIRED: Self = Self(1 << 7);

    /// Empty flagset.
    pub const NONE: Self = Self(0);

    /// Raw `u16` value.
    #[inline]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Construct from raw bits.
    #[inline]
    pub const fn from_bits(v: u16) -> Self {
        Self(v)
    }

    /// Bitwise OR.
    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns `true` if `other` is set in `self`.
    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Returns `true` if no flags are set.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for FrameFlags {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self::union(self, rhs)
    }
}

impl std::ops::BitOrAssign for FrameFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        *self = Self::union(*self, rhs);
    }
}

impl fmt::Debug for FrameFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FrameFlags(0x{:04X}", self.0)?;
        let mut first = true;
        let mut emit = |flag: FrameFlags, name: &str, first: &mut bool| -> fmt::Result {
            if self.contains(flag) {
                if *first {
                    write!(f, "|")?;
                    *first = false;
                } else {
                    write!(f, "|")?;
                }
                f.write_str(name)?;
            }
            Ok(())
        };
        emit(Self::COMPRESSED, "COMPRESSED", &mut first)?;
        emit(Self::ENCRYPTED, "ENCRYPTED", &mut first)?;
        emit(Self::SIGNED, "SIGNED", &mut first)?;
        emit(Self::STREAM_END, "STREAM_END", &mut first)?;
        emit(Self::ERROR, "ERROR", &mut first)?;
        emit(Self::BATCHED, "BATCHED", &mut first)?;
        emit(Self::COMPACT, "COMPACT", &mut first)?;
        emit(Self::ACK_REQUIRED, "ACK_REQUIRED", &mut first)?;
        write!(f, ")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_combinations() {
        let f = FrameFlags::ENCRYPTED | FrameFlags::SIGNED | FrameFlags::ACK_REQUIRED;
        assert!(f.contains(FrameFlags::ENCRYPTED));
        assert!(f.contains(FrameFlags::SIGNED));
        assert!(f.contains(FrameFlags::ACK_REQUIRED));
        assert!(!f.contains(FrameFlags::COMPRESSED));
        assert_eq!(f.bits(), 0b1000_0110);
    }

    #[test]
    fn empty_flags() {
        assert!(FrameFlags::NONE.is_empty());
        assert!(!FrameFlags::ENCRYPTED.is_empty());
    }
}
