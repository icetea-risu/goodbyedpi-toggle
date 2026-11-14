//! Reads the file version from a PE image's `VS_FIXEDFILEINFO` block.
//!
//! Used by `build.rs` to show the real WinDivert version in the About box
//! without trusting file names. Depends on `std` only.

/// `VS_FIXEDFILEINFO.dwSignature`.
const SIGNATURE: [u8; 4] = 0xFEEF_04BDu32.to_le_bytes();

/// Returns `(major, minor, build, revision)` of the first `VS_FIXEDFILEINFO`.
pub fn file_version(image: &[u8]) -> Option<(u16, u16, u16, u16)> {
    let pos = image.windows(4).position(|w| w == SIGNATURE)?;
    // dwSignature, dwStrucVersion, dwFileVersionMS, dwFileVersionLS
    let ms = read_u32(image, pos + 8)?;
    let ls = read_u32(image, pos + 12)?;
    Some(((ms >> 16) as u16, ms as u16, (ls >> 16) as u16, ls as u16))
}

/// "2.2.2" style string; the revision is appended only when non-zero.
pub fn file_version_string(image: &[u8]) -> Option<String> {
    let (a, b, c, d) = file_version(image)?;
    Some(if d == 0 {
        format!("{a}.{b}.{c}")
    } else {
        format!("{a}.{b}.{c}.{d}")
    })
}

fn read_u32(buf: &[u8], at: usize) -> Option<u32> {
    let bytes = buf.get(at..at + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(ms: u32, ls: u32) -> Vec<u8> {
        let mut v = vec![0xAAu8; 37];
        v.extend_from_slice(&0xFEEF_04BDu32.to_le_bytes());
        v.extend_from_slice(&0x0001_0000u32.to_le_bytes());
        v.extend_from_slice(&ms.to_le_bytes());
        v.extend_from_slice(&ls.to_le_bytes());
        v.extend_from_slice(&[0u8; 8]);
        v
    }

    #[test]
    fn reads_fixed_file_info() {
        assert_eq!(
            file_version(&image(0x0002_0002, 0x0002_0000)),
            Some((2, 2, 2, 0))
        );
        assert_eq!(
            file_version_string(&image(0x0002_0002, 0x0002_0000)).as_deref(),
            Some("2.2.2")
        );
        assert_eq!(
            file_version_string(&image(0x0001_0004, 0x0003_0007)).as_deref(),
            Some("1.4.3.7")
        );
    }

    #[test]
    fn missing_or_truncated_is_none() {
        assert_eq!(file_version(b"no version here"), None);
        let mut img = image(1, 2);
        img.truncate(37 + 10);
        assert_eq!(file_version(&img), None);
    }
}
