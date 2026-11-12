//! Build-time helper for `tauri-plugin-gaiadesk-support`: puts the prebuilt
//! `libgaiadesk_embed` for the target into your `src-tauri/gaiadesk/`, where
//! `tauri.conf.json` bundles it.
//!
//! ```ignore
//! // src-tauri/build.rs
//! fn main() {
//!     gaiadesk_embed_fetch::fetch("gaiadesk").expect("libgaiadesk_embed");
//!     tauri_build::build();
//! }
//! ```
//!
//! It downloads the archive for the target from the gaiadesk-embed-cpp
//! release, **checks its SHA-256 against the hash pinned below** (a mismatch
//! is an error and nothing is unpacked), and copies the library and
//! `LICENSE-BINARY` into the directory. A directory already holding the
//! pinned version is left alone, so this downloads once. It also copies them
//! beside the dev executable (`target/<profile>/gaiadesk/`), where the plugin
//! finds them under `tauri dev`.
//!
//! Environment:
//! - `GAIADESK_EMBED_ROOT`: an unpacked release archive to use instead of
//!   downloading (offline builds, your own mirror).
//! - `GAIADESK_EMBED_BASE_URL`: where the archives are (default: the GitHub
//!   release; `file://` works). Still checked against the pinned hashes.
//!
//! Needs `curl` and `tar` on the build machine (macOS, Linux and Windows 10+
//! ship both; Windows' `tar` unpacks the `.zip`).
//!
//! SPDX-License-Identifier: MIT

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// The library version these hashes pin.
pub const VERSION: &str = "0.3.1";

// BEGIN PINNED HASHES (the same as gaiadesk-embed-cpp's cmake/FetchGaiaDeskEmbed.cmake)
/// `(archive, sha256)` for every published archive of [`VERSION`].
pub const PINNED: &[(&str, &str)] = &[
    (
        "gaiadesk_embed-macos-universal.tar.gz",
        "b859d3e099500af4cec5f065931e6ab6a82c24c2ca9c55aabffcf497b6ef77c5",
    ),
    (
        "gaiadesk_embed-linux-x64.tar.gz",
        "b345a836893d3d889fac50781c8371d9318e82b7d4d0a550c4c03064d53b4aeb",
    ),
    (
        "gaiadesk_embed-win-x64.zip",
        "25c157c78f67286fe4ca46abd108c5dc44b4e630e8d70c92203586927f5067dd",
    ),
];
// END PINNED HASHES

/// The default download location of [`VERSION`]'s archives.
pub fn default_base_url() -> String {
    format!("https://github.com/Gaia-Desk/gaiadesk-embed-cpp/releases/download/v{VERSION}")
}

/// What one target needs: the archive, the library's path inside it, and its
/// file name once placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    pub archive: &'static str,
    pub inner_path: &'static str,
    pub file_name: &'static str,
}

/// The asset for a target (`CARGO_CFG_TARGET_OS` / `_ARCH` values), or
/// `None` where no prebuilt library exists.
pub fn asset_for(os: &str, arch: &str) -> Option<Asset> {
    match (os, arch) {
        ("macos", "aarch64" | "x86_64") => Some(Asset {
            archive: "gaiadesk_embed-macos-universal.tar.gz",
            inner_path: "lib/libgaiadesk_embed.dylib",
            file_name: "libgaiadesk_embed.dylib",
        }),
        ("windows", "x86_64") => Some(Asset {
            archive: "gaiadesk_embed-win-x64.zip",
            inner_path: "bin/gaiadesk_embed.dll",
            file_name: "gaiadesk_embed.dll",
        }),
        ("linux", "x86_64") => Some(Asset {
            archive: "gaiadesk_embed-linux-x64.tar.gz",
            inner_path: "lib/libgaiadesk_embed.so",
            file_name: "libgaiadesk_embed.so",
        }),
        _ => None,
    }
}

/// The pinned SHA-256 of an archive of [`VERSION`].
pub fn pinned_sha256(archive: &str) -> Option<&'static str> {
    PINNED.iter().find(|(a, _)| *a == archive).map(|(_, h)| *h)
}

/// The SHA-256 of a file, lowercase hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Refuse a file whose SHA-256 is not `expected`.
pub fn verify(path: &Path, expected: &str) -> Result<(), String> {
    let got = sha256_file(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    if got.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!(
            "{} has SHA-256 {got}, expected {expected}: refusing it",
            path.display()
        ))
    }
}

/// For a build script: place the library for the target being built
/// (`CARGO_CFG_TARGET_OS`/`_ARCH`) in `dir` (relative to the crate) and
/// beside the dev executable. Returns the placed library, or `None` (with a
/// cargo warning) on a target with no prebuilt library.
pub fn fetch(dir: impl AsRef<Path>) -> Result<Option<PathBuf>, String> {
    println!("cargo:rerun-if-env-changed=GAIADESK_EMBED_ROOT");
    println!("cargo:rerun-if-env-changed=GAIADESK_EMBED_BASE_URL");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let Some(asset) = asset_for(&os, &arch) else {
        println!("cargo:warning=GaiaDesk: no prebuilt libgaiadesk_embed for {os} {arch}; \"Get help\" will report library_missing");
        return Ok(None);
    };
    let dir = dir.as_ref();
    let lib = fetch_for(asset, dir)?;
    // tauri dev runs target/<profile>/<app>: OUT_DIR is target/<profile>/build/<pkg>/out.
    if let Some(profile_dir) = std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .and_then(|o| o.ancestors().nth(3).map(Path::to_path_buf))
    {
        copy_into(dir, &profile_dir.join("gaiadesk"), asset.file_name)?;
    }
    println!("cargo:rerun-if-changed={}", lib.display());
    Ok(Some(lib))
}

/// Place `asset` in `dir` (downloading, verifying and unpacking it unless
/// `dir` already has the pinned version). Returns the library's path.
pub fn fetch_for(asset: Asset, dir: &Path) -> Result<PathBuf, String> {
    let sha = pinned_sha256(asset.archive)
        .ok_or_else(|| format!("no pinned SHA-256 for {}", asset.archive))?;
    let lib = dir.join(asset.file_name);
    let stamp = dir.join(".sha256");
    if lib.is_file() && fs::read_to_string(&stamp).is_ok_and(|s| s.trim() == sha) {
        return Ok(lib);
    }
    fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let root = match std::env::var_os("GAIADESK_EMBED_ROOT") {
        Some(r) => PathBuf::from(r),
        None => download_and_unpack(asset, sha, dir)?,
    };
    fs::copy(root.join(asset.inner_path), &lib)
        .map_err(|e| format!("copying {}: {e}", asset.inner_path))?;
    fs::copy(root.join("LICENSE-BINARY"), dir.join("LICENSE-BINARY"))
        .map_err(|e| format!("copying LICENSE-BINARY: {e}"))?;
    if std::env::var_os("GAIADESK_EMBED_ROOT").is_none() {
        let _ = fs::remove_dir_all(&root);
        fs::write(&stamp, sha).map_err(|e| format!("writing {}: {e}", stamp.display()))?;
    }
    Ok(lib)
}

fn download_and_unpack(asset: Asset, sha: &str, dir: &Path) -> Result<PathBuf, String> {
    let base = std::env::var("GAIADESK_EMBED_BASE_URL").unwrap_or_else(|_| default_base_url());
    let url = format!("{}/{}", base.trim_end_matches('/'), asset.archive);
    let work = dir.join(".download");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|e| format!("creating {}: {e}", work.display()))?;
    let archive = work.join(asset.archive);
    run(
        Command::new("curl")
            .args(["-fsSL", "--retry", "3", "-o"])
            .arg(&archive)
            .arg(&url),
        &format!("downloading {url}"),
    )?;
    if let Err(e) = verify(&archive, sha) {
        let _ = fs::remove_dir_all(&work);
        return Err(e);
    }
    let root = work.join("root");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    run(
        Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(&root),
        &format!("unpacking {}", asset.archive),
    )?;
    Ok(root)
}

fn run(cmd: &mut Command, what: &str) -> Result<(), String> {
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("{what} failed ({s})")),
        Err(e) => Err(format!("{what}: {e}")),
    }
}

fn copy_into(from: &Path, to: &Path, file_name: &str) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("creating {}: {e}", to.display()))?;
    for f in [file_name, "LICENSE-BINARY"] {
        fs::copy(from.join(f), to.join(f))
            .map_err(|e| format!("copying {f} to {}: {e}", to.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_target_has_a_pinned_hash() {
        for (os, arch) in [
            ("macos", "aarch64"),
            ("macos", "x86_64"),
            ("windows", "x86_64"),
            ("linux", "x86_64"),
        ] {
            let a = asset_for(os, arch).unwrap();
            let h = pinned_sha256(a.archive).unwrap();
            assert_eq!(h.len(), 64);
            assert!(h
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
            assert!(
                h.bytes().any(|b| b != b'0'),
                "placeholder hash for {}",
                a.archive
            );
        }
        assert_eq!(PINNED.len(), 3);
    }

    #[test]
    fn unsupported_targets_have_no_asset() {
        assert_eq!(asset_for("linux", "aarch64"), None);
        assert_eq!(asset_for("windows", "aarch64"), None);
        assert_eq!(asset_for("ios", "aarch64"), None);
    }

    #[test]
    fn pins_match_the_cpp_repository() {
        // The hashes gaiadesk-embed-cpp's FetchGaiaDeskEmbed.cmake pins for 0.3.1.
        assert_eq!(
            pinned_sha256("gaiadesk_embed-macos-universal.tar.gz"),
            Some("b859d3e099500af4cec5f065931e6ab6a82c24c2ca9c55aabffcf497b6ef77c5")
        );
        assert_eq!(VERSION, "0.3.1");
    }

    #[test]
    fn verify_accepts_the_right_hash_and_refuses_another() {
        let dir = std::env::temp_dir().join(format!("gd-fetch-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x");
        fs::write(&f, b"abc").unwrap();
        // SHA-256("abc")
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify(&f, abc).is_ok());
        assert!(verify(&f, &abc.to_uppercase()).is_ok());
        let err = verify(&f, &"0".repeat(64)).unwrap_err();
        assert!(err.contains("refusing"), "{err}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_placed_library_with_the_pinned_stamp_is_not_downloaded_again() {
        let dir = std::env::temp_dir().join(format!("gd-fetch-cache-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let a = asset_for("linux", "x86_64").unwrap();
        fs::write(dir.join(a.file_name), b"lib").unwrap();
        fs::write(dir.join(".sha256"), pinned_sha256(a.archive).unwrap()).unwrap();
        // No network and no curl call: it returns the placed file as is.
        assert_eq!(fetch_for(a, &dir).unwrap(), dir.join(a.file_name));
        fs::remove_dir_all(&dir).unwrap();
    }
}
