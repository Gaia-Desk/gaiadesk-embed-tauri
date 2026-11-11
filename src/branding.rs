//! A logo (path, resource, URL or data URL) → the base64 PNG the library takes.

use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::error::{Error, Result};
use crate::http;

/// The library's limit.
pub const MAX_LOGO_BYTES: usize = 256 * 1024;

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// PNG bytes → base64, refusing anything else or anything too big. Pure.
pub fn logo_base64(bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_LOGO_BYTES {
        return Err(Error::new("bad_logo", format!("the logo is {} bytes; at most {MAX_LOGO_BYTES}", bytes.len())));
    }
    if !bytes.starts_with(PNG_MAGIC) {
        return Err(Error::new("bad_logo", "the logo must be a PNG"));
    }
    Ok(STANDARD.encode(bytes))
}

/// Read a logo: `data:image/png;base64,…`, `https://…`, an absolute path, or
/// a path relative to `resource_dir` (the app's bundled resources).
pub fn load_logo(spec: &str, resource_dir: Option<&Path>) -> Result<String> {
    let bytes = if let Some(b64) = spec.strip_prefix("data:image/png;base64,") {
        STANDARD.decode(b64.trim()).map_err(|e| Error::new("bad_logo", format!("the logo data URL is not base64: {e}")))?
    } else if spec.starts_with("https://") {
        http::get_limited(spec, MAX_LOGO_BYTES + 1).map_err(|e| Error::new("bad_logo", format!("could not download the logo: {}", e.message)))?
    } else {
        let p = Path::new(spec);
        let p = match resource_dir {
            Some(r) if p.is_relative() => r.join(p),
            _ => p.to_path_buf(),
        };
        std::fs::read(&p).map_err(|e| Error::new("bad_logo", format!("could not read the logo {}: {e}", p.display())))?
    };
    logo_base64(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut v = PNG_MAGIC.to_vec();
        v.extend_from_slice(b"rest");
        v
    }

    #[test]
    fn png_bytes_become_base64() {
        assert_eq!(logo_base64(&png()).unwrap(), STANDARD.encode(png()));
    }

    #[test]
    fn not_a_png_or_too_big_is_refused() {
        assert_eq!(logo_base64(b"GIF89a").unwrap_err().code, "bad_logo");
        let mut big = png();
        big.resize(MAX_LOGO_BYTES + 1, 0);
        assert!(logo_base64(&big).unwrap_err().message.contains("at most"));
        big.truncate(MAX_LOGO_BYTES);
        assert!(logo_base64(&big).is_ok());
    }

    #[test]
    fn data_url_relative_and_absolute_paths() {
        let b64 = STANDARD.encode(png());
        assert_eq!(load_logo(&format!("data:image/png;base64,{b64}"), None).unwrap(), b64);

        let dir = std::env::temp_dir().join(format!("gd-logo-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("icons")).unwrap();
        std::fs::write(dir.join("icons/logo.png"), png()).unwrap();
        assert_eq!(load_logo("icons/logo.png", Some(&dir)).unwrap(), b64);
        assert_eq!(load_logo(dir.join("icons/logo.png").to_str().unwrap(), None).unwrap(), b64);
        assert_eq!(load_logo("icons/missing.png", Some(&dir)).unwrap_err().code, "bad_logo");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
