//! Procedurally drawn icons (a coloured disc with a power symbol).
//!
//! No external artwork is used: every pixel comes from the geometry below.
//! `build.rs` uses this to write the `.ico` embedded as the exe icon and the
//! app uses it to create tray icons at the exact size Windows asks for.
//! Depends on `std` only.

/// RGB colour.
pub type Rgb = [u8; 3];

/// On (running).
pub const GREEN: Rgb = [0x2e, 0x9e, 0x44];
/// Off.
pub const GREY: Rgb = [0x7d, 0x7d, 0x7d];
/// Error.
pub const RED: Rgb = [0xd0, 0x3a, 0x2f];

/// Sizes written into the exe's `.ico`.
pub const ICO_SIZES: [u32; 7] = [16, 20, 24, 32, 40, 48, 64];

const SUPERSAMPLE: u32 = 4;

/// Renders a `size`×`size` RGBA image, rows top to bottom.
pub fn render(size: u32, fill: Rgb) -> Vec<u8> {
    let border = darken(fill);
    let mut out = vec![0u8; (size * size * 4) as usize];
    let step = 1.0 / (size * SUPERSAMPLE) as f32;
    for py in 0..size {
        for px in 0..size {
            let (mut disc, mut ring, mut glyph) = (0u32, 0u32, 0u32);
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let x = (px * SUPERSAMPLE + sx) as f32 * step + step / 2.0;
                    let y = (py * SUPERSAMPLE + sy) as f32 * step + step / 2.0;
                    match sample(x, y) {
                        Sample::Outside => {}
                        Sample::Border => ring += 1,
                        Sample::Disc => disc += 1,
                        Sample::Glyph => glyph += 1,
                    }
                }
            }
            let total = (SUPERSAMPLE * SUPERSAMPLE) as f32;
            let covered = (disc + ring + glyph) as f32;
            let i = ((py * size + px) * 4) as usize;
            if covered == 0.0 {
                continue;
            }
            // Average the colours of the covered samples, alpha = coverage.
            let mut rgb = [0f32; 3];
            for (c, slot) in rgb.iter_mut().enumerate() {
                *slot = (fill[c] as f32 * disc as f32
                    + border[c] as f32 * ring as f32
                    + 255.0 * glyph as f32)
                    / covered;
            }
            out[i] = rgb[0].round() as u8;
            out[i + 1] = rgb[1].round() as u8;
            out[i + 2] = rgb[2].round() as u8;
            out[i + 3] = ((covered / total) * 255.0).round() as u8;
        }
    }
    out
}

enum Sample {
    Outside,
    Border,
    Disc,
    Glyph,
}

fn sample(x: f32, y: f32) -> Sample {
    let (dx, dy) = (x - 0.5, y - 0.5);
    let r = (dx * dx + dy * dy).sqrt();
    if r > 0.48 {
        return Sample::Outside;
    }
    if is_power_glyph(dx, dy, r) {
        return Sample::Glyph;
    }
    if r > 0.43 {
        Sample::Border
    } else {
        Sample::Disc
    }
}

/// IEC 5009 style power symbol: an open ring with a vertical bar.
fn is_power_glyph(dx: f32, dy: f32, r: f32) -> bool {
    const RING_R: f32 = 0.21;
    const HALF_W: f32 = 0.045;
    // Ring, open at the top (angle measured from straight up).
    if (r - RING_R).abs() <= HALF_W {
        let angle_from_up = dx.atan2(-dy).abs();
        if angle_from_up > 0.75 {
            return true;
        }
    }
    // Vertical bar from above the ring to the centre, with round caps.
    let (top, bottom) = (-0.29f32, -0.02f32);
    let cy = dy.clamp(top, bottom);
    let d = (dx * dx + (dy - cy) * (dy - cy)).sqrt();
    d <= HALF_W
}

fn darken(c: Rgb) -> Rgb {
    [
        (c[0] as u16 * 3 / 4) as u8,
        (c[1] as u16 * 3 / 4) as u8,
        (c[2] as u16 * 3 / 4) as u8,
    ]
}

/// One icon image in the format stored inside `.ico` files and accepted by
/// `CreateIconFromResourceEx`: a BITMAPINFOHEADER with doubled height,
/// 32-bit BGRA pixels bottom-up, then a 1-bpp AND mask (all zero, alpha wins).
pub fn icon_image(size: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), (size * size * 4) as usize, "pixel buffer size");
    let mask_stride = size.div_ceil(32) * 4;
    let xor_len = size * size * 4;
    let mask_len = mask_stride * size;
    let mut out = Vec::with_capacity((40 + xor_len + mask_len) as usize);
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(size as i32).to_le_bytes()); // biWidth
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // biHeight (XOR + AND)
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    out.extend_from_slice(&(xor_len + mask_len).to_le_bytes()); // biSizeImage
    out.extend_from_slice(&[0u8; 16]); // resolution + palette fields
    for row in (0..size).rev() {
        for col in 0..size {
            let i = ((row * size + col) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    out.resize(out.len() + mask_len as usize, 0);
    out
}

/// A complete `.ico` file containing every `(size, image)` pair.
pub fn ico_file(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type = icon
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in images {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bit count
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in images {
        out.extend_from_slice(data);
    }
    out
}

/// The `.ico` embedded in the exe (green, all [`ICO_SIZES`]).
pub fn app_ico() -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = ICO_SIZES
        .iter()
        .map(|&s| (s, icon_image(s, &render(s, GREEN))))
        .collect();
    ico_file(&images)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_has_transparent_corners_and_opaque_glyph() {
        let s = 32;
        let px = render(s, GREEN);
        assert_eq!(px.len(), (s * s * 4) as usize);
        assert_eq!(px[3], 0, "top-left corner is transparent");
        // Bottom of the power ring (centre column, y ≈ 0.5 + 0.21) is white.
        let (x, y) = (16usize, ((0.5 + 0.21) * s as f32) as usize);
        let i = (y * s as usize + x) * 4;
        assert_eq!(px[i + 3], 255);
        assert!(px[i] > 200 && px[i + 1] > 200 && px[i + 2] > 200);
        // A point inside the disc but off the glyph keeps the fill colour.
        let (x, y) = (5usize, 16usize);
        let i = (y * s as usize + x) * 4;
        assert_eq!(&px[i..i + 3], &GREEN);
    }

    #[test]
    fn colours_differ_per_state() {
        assert_ne!(render(16, GREEN), render(16, GREY));
        assert_ne!(render(16, GREY), render(16, RED));
    }

    #[test]
    fn icon_image_layout() {
        let s = 20;
        let img = icon_image(s, &render(s, RED));
        let mask_stride = 4; // 20 px -> 1 DWORD per row
        assert_eq!(
            img.len(),
            40 + (s * s * 4) as usize + mask_stride * s as usize
        );
        assert_eq!(u32::from_le_bytes(img[0..4].try_into().unwrap()), 40);
        assert_eq!(
            i32::from_le_bytes(img[8..12].try_into().unwrap()),
            40,
            "height doubled"
        );
        assert_eq!(u16::from_le_bytes(img[14..16].try_into().unwrap()), 32);
    }

    #[test]
    fn ico_directory_points_at_images() {
        let ico = app_ico();
        assert_eq!(&ico[0..4], &[0, 0, 1, 0]);
        let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
        assert_eq!(count, ICO_SIZES.len());
        for (n, &size) in ICO_SIZES.iter().enumerate() {
            let e = 6 + n * 16;
            assert_eq!(ico[e] as u32, size);
            let len = u32::from_le_bytes(ico[e + 8..e + 12].try_into().unwrap()) as usize;
            let off = u32::from_le_bytes(ico[e + 12..e + 16].try_into().unwrap()) as usize;
            assert!(off + len <= ico.len());
            assert_eq!(
                u32::from_le_bytes(ico[off..off + 4].try_into().unwrap()),
                40
            );
        }
    }
}
