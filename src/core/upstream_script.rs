//! Reads GoodbyeDPI arguments out of the official `.cmd` launcher scripts
//! that ship in the release zip.
//!
//! The DNS redirection arguments used by the tray app are not typed in by
//! hand: `build.rs` extracts them from `2_any_country_dnsredir.cmd` (kept in
//! `assets/goodbyedpi/` by the fetch script) so they always match what the
//! GoodbyeDPI author recommends for the pinned release.
//!
//! Compiled both in the app and in `build.rs`; depends on `std` only.

use std::net::IpAddr;

/// DNS-related options GoodbyeDPI accepts (goodbyedpi.c long_options).
pub const DNS_OPTIONS: [&str; 4] = ["--dns-addr", "--dns-port", "--dnsv6-addr", "--dnsv6-port"];

/// Splits a cmd.exe command line into tokens. Double quotes group text and
/// are removed; this is enough for the simple launcher scripts upstream ships.
pub fn tokenize(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(cur);
    }
    out
}

/// Returns the `(option, value)` pairs for every DNS option on the single
/// line of `script` that runs `goodbyedpi.exe`.
pub fn dns_args(script: &str) -> Result<Vec<(String, String)>, String> {
    let mut found: Option<Vec<(String, String)>> = None;
    for line in script.lines() {
        let tokens = tokenize(line);
        let Some(exe_pos) = tokens
            .iter()
            .position(|t| t.to_ascii_lowercase().ends_with("goodbyedpi.exe"))
        else {
            continue;
        };
        let mut pairs = Vec::new();
        let mut i = exe_pos + 1;
        while i < tokens.len() {
            let t = tokens[i].as_str();
            if DNS_OPTIONS.contains(&t) {
                let value = tokens
                    .get(i + 1)
                    .ok_or_else(|| format!("{t} has no value in: {line}"))?;
                validate(t, value)?;
                pairs.push((t.to_string(), value.clone()));
                i += 2;
            } else {
                i += 1;
            }
        }
        if pairs.is_empty() {
            continue;
        }
        if found.is_some() {
            return Err("more than one goodbyedpi.exe line with DNS options".into());
        }
        found = Some(pairs);
    }
    let pairs = found.ok_or("no goodbyedpi.exe line with DNS options found")?;
    if !pairs.iter().any(|(k, _)| k == "--dns-addr") {
        return Err("script has DNS options but no --dns-addr".into());
    }
    Ok(pairs)
}

fn validate(option: &str, value: &str) -> Result<(), String> {
    match option {
        "--dns-addr" => match value.parse::<IpAddr>() {
            Ok(IpAddr::V4(_)) => Ok(()),
            _ => Err(format!("--dns-addr {value} is not an IPv4 address")),
        },
        "--dnsv6-addr" => match value.parse::<IpAddr>() {
            Ok(IpAddr::V6(_)) => Ok(()),
            _ => Err(format!("--dnsv6-addr {value} is not an IPv6 address")),
        },
        "--dns-port" | "--dnsv6-port" => match value.parse::<u16>() {
            Ok(p) if p > 0 => Ok(()),
            _ => Err(format!("{option} {value} is not a port number")),
        },
        _ => Err(format!("unknown option {option}")),
    }
}

/// Picks the reference script that holds the DNS arguments for "any country".
pub fn is_any_country_dns_script(file_name: &str) -> bool {
    let n = file_name.to_ascii_lowercase();
    n.ends_with(".cmd")
        && n.contains("any_country")
        && n.contains("dnsredir")
        && !n.contains("service")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_quotes() {
        assert_eq!(
            tokenize(r#"start "" goodbyedpi.exe -9"#),
            vec!["start", "", "goodbyedpi.exe", "-9"]
        );
        assert_eq!(tokenize(r#"a "b c"d"#), vec!["a", "b cd"]);
    }

    #[test]
    fn extracts_dns_pairs_from_launcher_line() {
        let script = "@ECHO OFF\r\nPUSHD \"%~dp0\"\r\nstart \"\" goodbyedpi.exe -9 --dns-addr 1.2.3.4 --dns-port 1253 --dnsv6-addr 2001:db8::1 --dnsv6-port 1253\r\nPOPD\r\n";
        let pairs = dns_args(script).unwrap();
        assert_eq!(
            pairs,
            vec![
                ("--dns-addr".to_string(), "1.2.3.4".to_string()),
                ("--dns-port".to_string(), "1253".to_string()),
                ("--dnsv6-addr".to_string(), "2001:db8::1".to_string()),
                ("--dnsv6-port".to_string(), "1253".to_string()),
            ]
        );
    }

    #[test]
    fn rejects_scripts_without_dns_or_with_bad_values() {
        assert!(dns_args("goodbyedpi.exe -9\r\n").is_err());
        assert!(dns_args("goodbyedpi.exe --dns-addr 2001:db8::1\n").is_err());
        assert!(dns_args("goodbyedpi.exe --dns-addr 1.2.3.4 --dns-port 0\n").is_err());
        assert!(dns_args("goodbyedpi.exe --dns-addr\n").is_err());
        let twice = "goodbyedpi.exe --dns-addr 1.2.3.4\ngoodbyedpi.exe --dns-addr 1.2.3.5\n";
        assert!(dns_args(twice).is_err());
    }

    #[test]
    fn picks_any_country_script() {
        assert!(is_any_country_dns_script(
            "upstream-2_any_country_dnsredir.cmd"
        ));
        assert!(!is_any_country_dns_script(
            "upstream-1_russia_blacklist_dnsredir.cmd"
        ));
        assert!(!is_any_country_dns_script(
            "upstream-service_install_any_country_dnsredir.cmd"
        ));
    }
}
