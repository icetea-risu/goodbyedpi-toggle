//! `assets/goodbyedpi/goodbyedpi.lock.toml` parsing and SHA-256 checks.
//!
//! This file is compiled twice: as `core::lock` in the app and via
//! `#[path]` inside `build.rs`. It must therefore only depend on `serde`,
//! `toml`, `sha2` and `std`, never on other crate modules.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// What a pinned file is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// `goodbyedpi.exe`
    Exe,
    /// `WinDivert.dll`
    Dll,
    /// `WinDivert64.sys` (signed kernel driver, must stay byte-identical)
    Sys,
    /// Upstream license text.
    License,
    /// Upstream `.cmd` launcher script, kept as a reference for arguments.
    Script,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LockedFile {
    pub role: Role,
    pub name: String,
    pub source: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Lock {
    pub schema: u32,
    pub version: String,
    pub prerelease: bool,
    pub release_url: String,
    pub zip_name: String,
    pub zip_url: String,
    pub zip_sha256: String,
    #[serde(rename = "file")]
    pub files: Vec<LockedFile>,
}

pub const SUPPORTED_SCHEMA: u32 = 1;

/// Payload roles that must be present exactly once.
pub const REQUIRED_PAYLOAD: [(Role, &str); 3] = [
    (Role::Exe, "goodbyedpi.exe"),
    (Role::Dll, "WinDivert.dll"),
    (Role::Sys, "WinDivert64.sys"),
];

impl Lock {
    pub fn parse(text: &str) -> Result<Lock, String> {
        let lock: Lock =
            toml::from_str(text).map_err(|e| format!("lock file is not valid: {e}"))?;
        lock.validate()?;
        Ok(lock)
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema != SUPPORTED_SCHEMA {
            return Err(format!(
                "lock schema {} is not supported (expected {SUPPORTED_SCHEMA})",
                self.schema
            ));
        }
        if !is_sha256_hex(&self.zip_sha256) {
            return Err("zip_sha256 is not a lowercase SHA-256 hex string".into());
        }
        for f in &self.files {
            if !is_sha256_hex(&f.sha256) {
                return Err(format!(
                    "{}: sha256 is not a lowercase SHA-256 hex string",
                    f.name
                ));
            }
            if !is_safe_file_name(&f.name) {
                return Err(format!("{}: file name is not a plain file name", f.name));
            }
        }
        for (role, name) in REQUIRED_PAYLOAD {
            let hits: Vec<_> = self.files.iter().filter(|f| f.role == role).collect();
            if hits.len() != 1 || hits[0].name != name {
                return Err(format!(
                    "lock must contain exactly one {role:?} file named {name}"
                ));
            }
        }
        if !self.files.iter().any(|f| f.role == Role::License) {
            return Err("lock must contain at least one license file".into());
        }
        let mut names: Vec<String> = self
            .files
            .iter()
            .map(|f| f.name.to_ascii_lowercase())
            .collect();
        names.sort();
        names.dedup();
        if names.len() != self.files.len() {
            return Err("lock contains duplicate file names".into());
        }
        Ok(())
    }

    pub fn file(&self, role: Role) -> Option<&LockedFile> {
        self.files.iter().find(|f| f.role == role)
    }
}

/// Lowercase hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut s = String::with_capacity(64);
    for b in digest {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Checks `data` against an expected lowercase hex digest.
pub fn verify(data: &[u8], expected: &str) -> Result<(), String> {
    let got = sha256_hex(data);
    if got == expected {
        Ok(())
    } else {
        Err(format!("SHA-256 mismatch: expected {expected}, got {got}"))
    }
}

pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A name without directory parts or characters Windows treats specially.
pub fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    fn sample(extra: &str) -> String {
        format!(
            r#"schema = 1
version = "0.2.3rc3"
prerelease = true
release_url = "https://example.invalid/r"
zip_name = "g.zip"
zip_url = "https://example.invalid/g.zip"
zip_sha256 = "{H}"
[[file]]
role = "exe"
name = "goodbyedpi.exe"
source = "g/x86_64/goodbyedpi.exe"
sha256 = "{H}"
[[file]]
role = "dll"
name = "WinDivert.dll"
source = "g/x86_64/WinDivert.dll"
sha256 = "{H}"
[[file]]
role = "sys"
name = "WinDivert64.sys"
source = "g/x86_64/WinDivert64.sys"
sha256 = "{H}"
[[file]]
role = "license"
name = "LICENSE-upstream-LICENSE"
source = "g/LICENSE"
sha256 = "{H}"
{extra}"#
        )
    }

    #[test]
    fn parses_valid_lock() {
        let lock = Lock::parse(&sample("")).unwrap();
        assert_eq!(lock.version, "0.2.3rc3");
        assert_eq!(lock.files.len(), 4);
        assert_eq!(lock.file(Role::Sys).unwrap().name, "WinDivert64.sys");
    }

    #[test]
    fn rejects_missing_payload() {
        let text = sample("").replace("role = \"dll\"", "role = \"script\"");
        assert!(Lock::parse(&text).unwrap_err().contains("Dll"));
    }

    #[test]
    fn rejects_path_in_name() {
        let text = sample("").replace("LICENSE-upstream-LICENSE", "..\\\\evil.txt");
        assert!(Lock::parse(&text).is_err());
    }

    #[test]
    fn rejects_bad_hash_and_duplicates() {
        let bad = sample("").replacen(H, "ABC", 1);
        assert!(Lock::parse(&bad).is_err());
        let dup = sample(&format!(
            "[[file]]\nrole = \"license\"\nname = \"license-upstream-license\"\nsource = \"x\"\nsha256 = \"{H}\"\n"
        ));
        assert!(Lock::parse(&dup).unwrap_err().contains("duplicate"));
    }

    #[test]
    fn rejects_unknown_schema() {
        let text = sample("").replace("schema = 1", "schema = 2");
        assert!(Lock::parse(&text).is_err());
    }

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(verify(
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
        .is_ok());
        assert!(verify(
            b"abd",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
        .is_err());
    }
}
