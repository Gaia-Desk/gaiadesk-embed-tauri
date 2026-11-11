//! A logo (path, resource, URL, data URL or `$appIcon`) → the base64 PNG the
//! library takes, downscaled to fit its limit. A logo that still cannot be
//! used is the caller's `warning`, never a refused session.

use std::io::{Cursor, Read};
use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::error::{Error, Result};
use crate::http;

/// The library's limit.
pub const MAX_LOGO_BYTES: usize = 256 * 1024;
/// What we read or download before downscaling.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
/// `logo: "$appIcon"`: the app's own window icon.
pub const APP_ICON: &str = "$appIcon";

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

fn bad(msg: impl Into<String>) -> Error {
    Error::new("bad_logo", msg)
}

/// PNG bytes → base64, downscaled until they fit [`MAX_LOGO_BYTES`]. Pure.
pub fn logo_base64(bytes: &[u8]) -> Result<String> {
    if !bytes.starts_with(PNG_MAGIC) {
        return Err(bad("the logo must be a PNG"));
    }
    if bytes.len() <= MAX_LOGO_BYTES {
        return Ok(STANDARD.encode(bytes));
    }
    let (mut rgba, mut w, mut h) = decode(bytes)?;
    loop {
        let png = encode(&rgba, w, h)?;
        if png.len() <= MAX_LOGO_BYTES {
            return Ok(STANDARD.encode(png));
        }
        if w < 32 || h < 32 {
            return Err(bad(format!(
                "the logo does not fit in {MAX_LOGO_BYTES} bytes even downscaled"
            )));
        }
        (rgba, w, h) = halve(&rgba, w, h);
    }
}

/// A PNG → 8-bit RGBA, its width and height.
fn decode(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let fail = |e: png::DecodingError| bad(format!("the logo is not a readable PNG: {e}"));
    // The decoder's default 64 MiB allocation limit stops a decompression bomb.
    let mut d = png::Decoder::new(Cursor::new(bytes));
    d.set_transformations(png::Transformations::normalize_to_color8());
    let mut r = d.read_info().map_err(fail)?;
    let mut buf = vec![
        0;
        r.output_buffer_size()
            .ok_or_else(|| bad("the logo is too large"))?
    ];
    let info = r.next_frame(&mut buf).map_err(fail)?;
    buf.truncate(info.buffer_size());
    let px = (info.width * info.height) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(bad("the logo's palette could not be expanded")),
    };
    if rgba.len() != px * 4 {
        return Err(bad("the logo's pixels could not be read"));
    }
    Ok((rgba, info.width, info.height))
}

/// 8-bit RGBA → PNG bytes.
pub fn encode(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    let fail = |e: png::EncodingError| bad(format!("could not encode the logo: {e}"));
    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.set_compression(png::Compression::High);
    let mut wr = e.write_header().map_err(fail)?;
    wr.write_image_data(rgba).map_err(fail)?;
    wr.finish().map_err(fail)?;
    Ok(out)
}

/// Half the size, each pixel the average of a 2×2 block. Pure.
fn halve(rgba: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let at = |x: u32, y: u32, c: usize| {
        rgba[((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize + c] as u32
    };
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        for x in 0..nw {
            for c in 0..4 {
                let s = at(2 * x, 2 * y, c)
                    + at(2 * x + 1, 2 * y, c)
                    + at(2 * x, 2 * y + 1, c)
                    + at(2 * x + 1, 2 * y + 1, c);
                out.push(((s + 2) / 4) as u8);
            }
        }
    }
    (out, nw, nh)
}

/// The app's window icon (RGBA from Tauri), if it has one.
pub type AppIcon<'a> = Option<(&'a [u8], u32, u32)>;

/// Read a logo: `$appIcon`, `data:image/png;base64,…`, `https://…`, an
/// absolute path, or a path relative to `resource_dir` (the app's bundled
/// resources).
pub fn load_logo(spec: &str, resource_dir: Option<&Path>, app_icon: AppIcon<'_>) -> Result<String> {
    let bytes = if spec == APP_ICON {
        let (rgba, w, h) = app_icon
            .ok_or_else(|| bad("the app has no window icon (bundle > icon in tauri.conf.json)"))?;
        encode(rgba, w, h)?
    } else if let Some(b64) = spec.strip_prefix("data:image/png;base64,") {
        STANDARD
            .decode(b64.trim())
            .map_err(|e| bad(format!("the logo data URL is not base64: {e}")))?
    } else if spec.starts_with("https://") {
        http::get_limited(spec, MAX_SOURCE_BYTES + 1)
            .map_err(|e| bad(format!("could not download the logo: {}", e.message)))?
    } else {
        let p = Path::new(spec);
        let p = match resource_dir {
            Some(r) if p.is_relative() => r.join(p),
            _ => p.to_path_buf(),
        };
        let mut v = Vec::new();
        std::fs::File::open(&p)
            .and_then(|f| f.take(MAX_SOURCE_BYTES as u64 + 1).read_to_end(&mut v))
            .map_err(|e| bad(format!("could not read the logo {}: {e}", p.display())))?;
        v
    };
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(bad(format!("the logo is over {MAX_SOURCE_BYTES} bytes")));
    }
    logo_base64(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        encode(&[10, 20, 30, 255], 1, 1).unwrap()
    }

    /// A noisy RGBA image (compresses badly), `n`×`n`.
    fn noise(n: u32) -> Vec<u8> {
        let mut x: u32 = 2463534242;
        (0..n * n * 4)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    #[test]
    fn small_pngs_pass_through_as_base64() {
        assert_eq!(logo_base64(&png()).unwrap(), STANDARD.encode(png()));
        assert_eq!(logo_base64(b"GIF89a").unwrap_err().code, "bad_logo");
    }

    #[test]
    fn an_oversized_png_is_downscaled_to_fit() {
        let big = encode(&noise(512), 512, 512).unwrap();
        assert!(big.len() > MAX_LOGO_BYTES, "{}", big.len());
        let out = STANDARD.decode(logo_base64(&big).unwrap()).unwrap();
        assert!(out.len() <= MAX_LOGO_BYTES);
        let (_, w, h) = decode(&out).unwrap();
        assert!(w < 512 && w == h, "{w}x{h}");
    }

    #[test]
    fn a_broken_oversized_png_is_refused_not_panicked() {
        let mut v = PNG_MAGIC.to_vec();
        v.resize(MAX_LOGO_BYTES + 1, 7);
        assert_eq!(logo_base64(&v).unwrap_err().code, "bad_logo");
    }

    #[test]
    fn halving_averages_blocks() {
        let px = [
            0, 0, 0, 255, 100, 100, 100, 255, 0, 0, 0, 255, 100, 100, 100, 255,
        ];
        assert_eq!(halve(&px, 2, 2), (vec![50, 50, 50, 255], 1, 1));
        let (_, w, h) = halve(&noise(5), 5, 5);
        assert_eq!((w, h), (2, 2));
    }

    #[test]
    fn grey_and_rgb_pngs_decode_to_rgba() {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 1, 1);
        e.set_color(png::ColorType::Grayscale);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&[9]).unwrap();
        w.finish().unwrap();
        assert_eq!(decode(&out).unwrap(), (vec![9, 9, 9, 255], 1, 1));
    }

    #[test]
    fn data_url_relative_absolute_paths_and_the_app_icon() {
        let b64 = STANDARD.encode(png());
        assert_eq!(
            load_logo(&format!("data:image/png;base64,{b64}"), None, None).unwrap(),
            b64
        );

        let dir = std::env::temp_dir().join(format!("gd-logo-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("icons")).unwrap();
        std::fs::write(dir.join("icons/logo.png"), png()).unwrap();
        assert_eq!(load_logo("icons/logo.png", Some(&dir), None).unwrap(), b64);
        assert_eq!(
            load_logo(dir.join("icons/logo.png").to_str().unwrap(), None, None).unwrap(),
            b64
        );
        assert_eq!(
            load_logo("icons/missing.png", Some(&dir), None)
                .unwrap_err()
                .code,
            "bad_logo"
        );
        std::fs::remove_dir_all(&dir).unwrap();

        let icon = [10u8, 20, 30, 255];
        assert_eq!(load_logo(APP_ICON, None, Some((&icon, 1, 1))).unwrap(), b64);
        assert_eq!(
            load_logo(APP_ICON, None, None).unwrap_err().code,
            "bad_logo"
        );
    }
}
