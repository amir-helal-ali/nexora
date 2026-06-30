//! Semantic versioning (SemVer) — strict MAJOR.MINOR.PATCH parsing.
//!
//! See Nexora Engineering Specification, Part 5 (VERSIONING SYSTEM).
//! All packages follow strict SemVer. Breaking changes require migration
//! scripts. Multiple versions can coexist.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

/// Error from parsing a SemVer string.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ParseVersionError {
    /// Empty string.
    #[error("empty version string")]
    Empty,
    /// Wrong number of components.
    #[error("expected 3 numeric components (MAJOR.MINOR.PATCH), got {0}")]
    WrongComponentCount(usize),
    /// Non-numeric component.
    #[error("component {field} is not a valid number: {value}")]
    NonNumeric {
        /// Which field.
        field: &'static str,
        /// The bad value.
        value: String,
    },
}

/// A SemVer version `MAJOR.MINOR.PATCH`.
///
/// Serializes as a string (e.g. `"1.2.3"`) for ergonomic JSON manifests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Version {
    /// Major version (breaking changes).
    pub major: u64,
    /// Minor version (backward-compatible features).
    pub minor: u64,
    /// Patch version (backward-compatible fixes).
    pub patch: u64,
}

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl Version {
    /// Construct a new version.
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Returns `true` if this version is compatible with `other` (same major).
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        self.major == other.major
    }

    /// Bump the patch version.
    pub const fn bump_patch(self) -> Self {
        Self::new(self.major, self.minor, self.patch + 1)
    }

    /// Bump the minor version (resets patch).
    pub const fn bump_minor(self) -> Self {
        Self::new(self.major, self.minor + 1, 0)
    }

    /// Bump the major version (resets minor + patch).
    pub const fn bump_major(self) -> Self {
        Self::new(self.major + 1, 0, 0)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for Version {
    type Err = ParseVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Err(ParseVersionError::Empty);
        }
        // Strip optional leading 'v'.
        let s = s.trim().trim_start_matches('v');
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(ParseVersionError::WrongComponentCount(parts.len()));
        }
        let parse = |s: &str, field: &'static str| -> Result<u64, ParseVersionError> {
            s.parse::<u64>().map_err(|_| ParseVersionError::NonNumeric {
                field,
                value: s.to_string(),
            })
        };
        Ok(Self {
            major: parse(parts[0], "major")?,
            minor: parse(parts[1], "minor")?,
            patch: parse(parts[2], "patch")?,
        })
    }
}

/// A version range for dependency declarations (e.g. `^1.2.0`, `~2.0.0`, `>=1.0.0`).
///
/// Serializes as a string (e.g. `"^1.2.0"`, `"*"`, `"1.0.0"`) for ergonomic
/// JSON manifests. Deserializes from either a string or the structured form.
#[derive(Clone, Debug, PartialEq)]
pub enum VersionRange {
    /// Exact version.
    Exact(Version),
    /// Caret: compatible with this version (same major). `^1.2.0` = `>=1.2.0, <2.0.0`.
    Caret(Version),
    /// Tilde: same minor. `~1.2.0` = `>=1.2.0, <1.3.0`.
    Tilde(Version),
    /// Greater-than-or-equal.
    Gte(Version),
    /// Less-than.
    Lt(Version),
    /// Any version.
    Any,
}

impl Serialize for VersionRange {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for VersionRange {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl VersionRange {
    /// Returns `true` if `version` satisfies this range.
    pub fn matches(&self, version: &Version) -> bool {
        match self {
            Self::Exact(v) => version == v,
            Self::Caret(v) => version.major == v.major && version >= v,
            Self::Tilde(v) => {
                version.major == v.major && version.minor == v.minor && version >= v
            }
            Self::Gte(v) => version >= v,
            Self::Lt(v) => version < v,
            Self::Any => true,
        }
    }

    /// Parse a range string like `^1.2.0`, `~2.0.0`, `>=1.0.0`, `1.0.0`, `*`.
    pub fn parse(s: &str) -> Result<Self, ParseVersionError> {
        let s = s.trim();
        if s == "*" || s.is_empty() {
            return Ok(Self::Any);
        }
        if let Some(rest) = s.strip_prefix("^") {
            return Ok(Self::Caret(Version::from_str(rest.trim())?));
        }
        if let Some(rest) = s.strip_prefix("~") {
            return Ok(Self::Tilde(Version::from_str(rest.trim())?));
        }
        if let Some(rest) = s.strip_prefix(">=") {
            return Ok(Self::Gte(Version::from_str(rest.trim())?));
        }
        if let Some(rest) = s.strip_prefix("<") {
            return Ok(Self::Lt(Version::from_str(rest.trim())?));
        }
        Ok(Self::Exact(Version::from_str(s)?))
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(v) => write!(f, "{}", v),
            Self::Caret(v) => write!(f, "^{}", v),
            Self::Tilde(v) => write!(f, "~{}", v),
            Self::Gte(v) => write!(f, ">={}", v),
            Self::Lt(v) => write!(f, "<{}", v),
            Self::Any => f.write_str("*"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let v: Version = "1.2.3".parse().unwrap();
        assert_eq!(v, Version::new(1, 2, 3));
        let v: Version = "v2.0.0".parse().unwrap();
        assert_eq!(v, Version::new(2, 0, 0));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!("1.2".parse::<Version>().is_err());
        assert!("1.2.3.4".parse::<Version>().is_err());
        assert!("a.b.c".parse::<Version>().is_err());
        assert!("".parse::<Version>().is_err());
    }

    #[test]
    fn ordering() {
        let v1 = Version::new(1, 0, 0);
        let v2 = Version::new(1, 0, 1);
        let v3 = Version::new(1, 1, 0);
        let v4 = Version::new(2, 0, 0);
        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 < v4);
    }

    #[test]
    fn compatibility() {
        let v1 = Version::new(1, 2, 3);
        let v2 = Version::new(1, 5, 0);
        let v3 = Version::new(2, 0, 0);
        assert!(v1.is_compatible_with(&v2));
        assert!(!v1.is_compatible_with(&v3));
    }

    #[test]
    fn bumps() {
        let v = Version::new(1, 2, 3);
        assert_eq!(v.bump_patch(), Version::new(1, 2, 4));
        assert_eq!(v.bump_minor(), Version::new(1, 3, 0));
        assert_eq!(v.bump_major(), Version::new(2, 0, 0));
    }

    #[test]
    fn range_exact() {
        let r = VersionRange::parse("1.2.3").unwrap();
        assert!(r.matches(&Version::new(1, 2, 3)));
        assert!(!r.matches(&Version::new(1, 2, 4)));
    }

    #[test]
    fn range_caret() {
        let r = VersionRange::parse("^1.2.0").unwrap();
        assert!(r.matches(&Version::new(1, 2, 0)));
        assert!(r.matches(&Version::new(1, 9, 9)));
        assert!(!r.matches(&Version::new(2, 0, 0)));
        assert!(!r.matches(&Version::new(1, 1, 9)));
    }

    #[test]
    fn range_tilde() {
        let r = VersionRange::parse("~1.2.0").unwrap();
        assert!(r.matches(&Version::new(1, 2, 0)));
        assert!(r.matches(&Version::new(1, 2, 9)));
        assert!(!r.matches(&Version::new(1, 3, 0)));
    }

    #[test]
    fn range_any() {
        let r = VersionRange::parse("*").unwrap();
        assert!(r.matches(&Version::new(0, 0, 1)));
        assert!(r.matches(&Version::new(99, 99, 99)));
    }

    #[test]
    fn range_gte_lt() {
        let r = VersionRange::parse(">=1.0.0").unwrap();
        assert!(r.matches(&Version::new(1, 0, 0)));
        assert!(r.matches(&Version::new(2, 0, 0)));
        assert!(!r.matches(&Version::new(0, 9, 9)));
        let r = VersionRange::parse("<2.0.0").unwrap();
        assert!(r.matches(&Version::new(1, 9, 9)));
        assert!(!r.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn display_roundtrip() {
        let v = Version::new(1, 2, 3);
        assert_eq!(v.to_string(), "1.2.3");
        let r = VersionRange::Caret(v);
        assert_eq!(r.to_string(), "^1.2.3");
    }
}
