//! Official GoodbyeDPI releases the app will launch (`data/known-releases.toml`).
//!
//! `build.rs` parses and validates the TOML with [`parse`] (via `#[path]`)
//! and turns it into the static [`KnownRelease`] table the app links in, so
//! nothing here is read from disk at run time. Depends on `serde`, `toml`,
//! `sha2` and `std` only.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Release entry as stored in `known-releases.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseRecord {
    pub version: String,
    pub prerelease: bool,
    pub release_url: String,
    pub zip_name: String,
    pub zip_url: String,
    pub zip_sha256: String,
    pub exe_source: String,
    pub exe_sha256: String,
    pub dll_source: String,
    pub dll_sha256: String,
    pub sys_source: String,
    pub sys_sha256: String,
    pub dns_script_source: String,
    pub dns_script_sha256: String,
    pub dns_script: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: u32,
    #[serde(default)]
    release: Vec<ReleaseRecord>,
}

pub const SUPPORTED_SCHEMA: u32 = 1;

/// Parses and validates the whole file.
pub fn parse(text: &str) -> Result<Vec<ReleaseRecord>, String> {
    let doc: Document = toml::from_str(text).map_err(|e| format!("known-releases.toml: {e}"))?;
    if doc.schema != SUPPORTED_SCHEMA {
        return Err(format!(
            "known-releases.toml: schema {} is not supported (expected {SUPPORTED_SCHEMA})",
            doc.schema
        ));
    }
    if doc.release.is_empty() {
        return Err("known-releases.toml lists no releases".into());
    }
    for r in &doc.release {
        for (what, h) in [
            ("zip_sha256", &r.zip_sha256),
            ("exe_sha256", &r.exe_sha256),
            ("dll_sha256", &r.dll_sha256),
            ("sys_sha256", &r.sys_sha256),
            ("dns_script_sha256", &r.dns_script_sha256),
        ] {
            if !is_sha256_hex(h) {
                return Err(format!(
                    "{}: {what} is not a lowercase SHA-256 hex string",
                    r.version
                ));
            }
        }
        if sha256_hex(r.dns_script.as_bytes()) != r.dns_script_sha256 {
            return Err(format!(
                "{}: dns_script does not match dns_script_sha256",
                r.version
            ));
        }
        if !r
            .release_url
            .starts_with("https://github.com/ValdikSS/GoodbyeDPI/")
        {
            return Err(format!(
                "{}: release_url is not on the official repository",
                r.version
            ));
        }
    }
    let mut versions: Vec<&str> = doc.release.iter().map(|r| r.version.as_str()).collect();
    versions.sort_unstable();
    versions.dedup();
    if versions.len() != doc.release.len() {
        return Err("known-releases.toml lists a version twice".into());
    }
    Ok(doc.release)
}

/// Release entry compiled into the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownRelease {
    pub version: &'static str,
    pub prerelease: bool,
    pub release_url: &'static str,
    pub exe_sha256: &'static str,
    pub dll_sha256: &'static str,
    pub sys_sha256: &'static str,
    /// `--dns-*` options from the official any-country launcher script.
    pub dns_args: &'static [(&'static str, &'static str)],
}

/// Hashes of the three files the app found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundHashes {
    pub exe: String,
    pub dll: String,
    pub sys: String,
}

/// Result of matching files on disk against the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identification<'a> {
    Known(&'a KnownRelease),
    /// No release matches all three files. Lists, for each file, whether it
    /// matches *any* known release, to tell the user which one is wrong.
    Unknown {
        exe_known: bool,
        dll_known: bool,
        sys_known: bool,
    },
}

pub fn identify<'a>(table: &'a [KnownRelease], found: &FoundHashes) -> Identification<'a> {
    if let Some(r) = table.iter().find(|r| {
        r.exe_sha256 == found.exe && r.dll_sha256 == found.dll && r.sys_sha256 == found.sys
    }) {
        return Identification::Known(r);
    }
    Identification::Unknown {
        exe_known: table.iter().any(|r| r.exe_sha256 == found.exe),
        dll_known: table.iter().any(|r| r.dll_sha256 == found.dll),
        sys_known: table.iter().any(|r| r.sys_sha256 == found.sys),
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

/// Streaming SHA-256 of a reader.
pub fn sha256_reader<R: std::io::Read>(mut r: R) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let mut s = String::with_capacity(64);
    for b in hasher.finalize() {
        let _ = write!(s, "{b:02x}");
    }
    Ok(s)
}

pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn record(version: &str) -> String {
        let script = "goodbyedpi.exe -9 --dns-addr 1.2.3.4\r\n";
        format!(
            r#"[[release]]
version = "{version}"
prerelease = false
release_url = "https://github.com/ValdikSS/GoodbyeDPI/releases/tag/{version}"
zip_name = "g.zip"
zip_url = "https://github.com/ValdikSS/GoodbyeDPI/releases/download/{version}/g.zip"
zip_sha256 = "{A}"
exe_source = "g/x86_64/goodbyedpi.exe"
exe_sha256 = "{A}"
dll_source = "g/x86_64/WinDivert.dll"
dll_sha256 = "{B}"
sys_source = "g/x86_64/WinDivert64.sys"
sys_sha256 = "{C}"
dns_script_source = "g/2_any_country_dnsredir.cmd"
dns_script_sha256 = "{}"
dns_script = {:?}
"#,
            sha256_hex(script.as_bytes()),
            script
        )
    }

    #[test]
    fn parses_records() {
        let text = format!("schema = 1\n{}\n{}", record("0.2.2"), record("0.2.3rc3"));
        let rs = parse(&text).unwrap();
        assert_eq!(rs.len(), 2);
        assert_eq!(rs[1].version, "0.2.3rc3");
        assert!(rs[0].dns_script.contains("--dns-addr"));
    }

    #[test]
    fn rejects_bad_documents() {
        assert!(parse("schema = 1\n").unwrap_err().contains("no releases"));
        assert!(parse(&format!("schema = 2\n{}", record("x"))).is_err());
        let dup = format!("schema = 1\n{}\n{}", record("x"), record("x"));
        assert!(parse(&dup).unwrap_err().contains("twice"));
        let tampered = format!("schema = 1\n{}", record("x")).replace("1.2.3.4", "6.6.6.6");
        assert!(parse(&tampered).unwrap_err().contains("dns_script"));
        let bad_hash = format!("schema = 1\n{}", record("x")).replacen(A, "AA", 1);
        assert!(parse(&bad_hash).is_err());
        let other_repo = format!("schema = 1\n{}", record("x")).replace(
            "release_url = \"https://github.com/ValdikSS",
            "release_url = \"https://github.com/someone",
        );
        assert!(parse(&other_repo).unwrap_err().contains("official"));
        let unknown_key = format!("schema = 1\n{}extra = 1\n", record("x"));
        assert!(parse(&unknown_key).is_err());
    }

    const TABLE: [KnownRelease; 2] = [
        KnownRelease {
            version: "old",
            prerelease: false,
            release_url: "",
            exe_sha256: "e1",
            dll_sha256: "d",
            sys_sha256: "s",
            dns_args: &[],
        },
        KnownRelease {
            version: "new",
            prerelease: true,
            release_url: "",
            exe_sha256: "e2",
            dll_sha256: "d",
            sys_sha256: "s",
            dns_args: &[],
        },
    ];

    fn found(e: &str, d: &str, s: &str) -> FoundHashes {
        FoundHashes {
            exe: e.into(),
            dll: d.into(),
            sys: s.into(),
        }
    }

    #[test]
    fn identifies_exact_triplets_only() {
        assert_eq!(
            identify(&TABLE, &found("e2", "d", "s")),
            Identification::Known(&TABLE[1])
        );
        assert_eq!(
            identify(&TABLE, &found("e2", "x", "s")),
            Identification::Unknown {
                exe_known: true,
                dll_known: false,
                sys_known: true
            }
        );
        assert_eq!(
            identify(&TABLE, &found("zz", "d", "s")),
            Identification::Unknown {
                exe_known: false,
                dll_known: true,
                sys_known: true
            }
        );
    }

    #[test]
    fn streaming_hash_matches_one_shot() {
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        assert_eq!(sha256_reader(&data[..]).unwrap(), sha256_hex(&data));
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
