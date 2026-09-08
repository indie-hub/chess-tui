// SPDX-License-Identifier: GPL-3.0-or-later
//
// First-run automatic fetch of a pinned, SHA-256-verified Stockfish binary,
// ported from scripts/fetch-stockfish.sh so a plain `cargo run` can play
// against the engine without a manual step. Supported only where a verified
// pin exists (macOS universal, Windows x86-64, Windows arm64 today);
// unsupported platforms keep the existing "no engine, local two-player"
// fallback untouched by this module (see main.rs). Downloading and
// extracting shell out to the same trusted system tools the script uses
// (curl, tar); hashing uses the sha2 crate directly (already a project
// dependency for the sprite-checksum tests) instead of the platform-specific
// shasum/certutil tools that would otherwise need separate, locale-sensitive
// output parsing per OS. The binary is never executed as part of
// verification.

use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveFormat {
    TarGz,
    Zip,
}

pub(crate) struct Pin {
    pub(crate) archive_url: &'static str,
    pub(crate) archive_format: ArchiveFormat,
    pub(crate) archive_size: u64,
    pub(crate) archive_sha256: &'static str,
    pub(crate) executable_in_archive: &'static str,
    pub(crate) executable_size: u64,
    pub(crate) executable_sha256: &'static str,
    // Filename the verified executable is staged under in
    // third_party/stockfish/bundle/; also the name engine::resolve_engine_path
    // looks for on this platform.
    pub(crate) staged_name: &'static str,
}

// Keep in sync with third_party/stockfish/manifest.json and
// scripts/fetch-stockfish.sh; drift is caught by the
// fetch_pin_matches_tracked_manifest test in tests.rs.
pub(crate) const STOCKFISH_19_MACOS: Pin = Pin {
    archive_url: "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-macos-universal.tar.gz",
    archive_format: ArchiveFormat::TarGz,
    archive_size: 82_323_876,
    archive_sha256: "a1f0e3bcc5a6927a11fe6fc8e54a779754645f3c2bae2cf13420fd1957adaa77",
    executable_in_archive: "stockfish/stockfish-macos-universal",
    executable_size: 105_458_632,
    executable_sha256: "8eed61129d1493c5d1f2fd9323f0c54c47ac49319911fbde18c6b9c87e8b13c5",
    staged_name: "stockfish-macos-universal",
};

// Independently downloaded and hashed from the same sf_19 tag (commit
// edb0d9db6731067ec50ce619ff372b463bc4dd5d) as the macOS pin above; see
// basic-memory/decisions/Stockfish 19 Windows artifact pin.md.
pub(crate) const STOCKFISH_19_WINDOWS_X86_64: Pin = Pin {
    archive_url: "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-windows-x86-64-universal.zip",
    archive_format: ArchiveFormat::Zip,
    archive_size: 81_431_614,
    archive_sha256: "3c8bf1f9ea66a09350a40df4f632288285ac206d99f33ab5842c408fc30b48a7",
    executable_in_archive: "stockfish/stockfish-windows-x86-64-universal.exe",
    executable_size: 103_046_300,
    executable_sha256: "45bc8e4969147db9c2eb533810637994619bff0eacc81ccfd9854394901bcbd0",
    staged_name: "stockfish-windows-x86-64-universal.exe",
};

pub(crate) const STOCKFISH_19_WINDOWS_ARM64: Pin = Pin {
    archive_url: "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-windows-arm64-universal.zip",
    archive_format: ArchiveFormat::Zip,
    archive_size: 80_190_536,
    archive_sha256: "8372ad3f0d7276deb2c70f801f541ec7db463219fc6d9c7592864e542aa4f401",
    executable_in_archive: "stockfish/stockfish-windows-arm64-universal.exe",
    executable_size: 100_303_360,
    executable_sha256: "3b5881df3d6f92817cf6664a6a18a473b50d424c71a1247fe4268090db281413",
    staged_name: "stockfish-windows-arm64-universal.exe",
};

#[derive(Debug)]
pub(crate) enum FetchError {
    Download(String),
    Verify(String),
    Extract(String),
    Io(std::io::Error),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Download(detail) => write!(f, "download failed: {detail}"),
            FetchError::Verify(detail) => write!(f, "verification failed: {detail}"),
            FetchError::Extract(detail) => write!(f, "extraction failed: {detail}"),
            FetchError::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

// The pin for this compiled target, or None where no binary has been
// independently verified yet (anything but macOS/Windows x86-64/Windows
// arm64 today).
pub(crate) fn selected_pin() -> Option<&'static Pin> {
    if cfg!(target_os = "macos") {
        Some(&STOCKFISH_19_MACOS)
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some(&STOCKFISH_19_WINDOWS_X86_64)
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        Some(&STOCKFISH_19_WINDOWS_ARM64)
    } else {
        None
    }
}

pub(crate) fn is_supported_platform() -> bool {
    selected_pin().is_some()
}

// The filename engine::resolve_engine_path looks for under
// third_party/stockfish/bundle/ on this platform, or None where there is no
// pin to stage in the first place.
pub(crate) fn staged_binary_name() -> Option<&'static str> {
    selected_pin().map(|pin| pin.staged_name)
}

pub(crate) fn ensure_staged() -> Result<PathBuf, FetchError> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pin = selected_pin()
        .ok_or_else(|| FetchError::Verify("no verified Stockfish pin for this platform".into()))?;
    ensure_staged_in(&manifest_dir, pin)
}

// Stages a verified Stockfish executable at
// `<manifest_dir>/third_party/stockfish/bundle/<pin.staged_name>`, the same
// path engine::resolve_engine_path_from looks for. Idempotent: a bundle that
// already matches the pin is returned immediately with no network access; a
// cached archive that already matches the pin skips the download too.
pub(crate) fn ensure_staged_in(manifest_dir: &Path, pin: &Pin) -> Result<PathBuf, FetchError> {
    let bundle_dir = manifest_dir.join("third_party/stockfish/bundle");
    let cache_dir = manifest_dir.join("third_party/stockfish/cache");
    let bundle_exe = bundle_dir.join(pin.staged_name);

    if matches_pin(&bundle_exe, pin.executable_size, pin.executable_sha256) {
        return Ok(bundle_exe);
    }

    fs::create_dir_all(&cache_dir).map_err(FetchError::Io)?;
    fs::create_dir_all(&bundle_dir).map_err(FetchError::Io)?;

    let archive_name = pin
        .archive_url
        .rsplit('/')
        .next()
        .unwrap_or("stockfish-archive");
    let archive = cache_dir.join(archive_name);
    if !matches_pin(&archive, pin.archive_size, pin.archive_sha256) {
        eprintln!(
            "[chess] fetching Stockfish (~{} MB, first run only)...",
            pin.archive_size / 1_000_000
        );
        download(pin.archive_url, &archive)?;
        verify_pin(&archive, pin.archive_size, pin.archive_sha256, "archive")?;
    }

    // std::process::id() alone collides across threads in the same test
    // binary (all unit tests share one process), which raced concurrently
    // running fetch tests against the same extraction directory; the counter
    // makes every call's directory unique regardless of caller concurrency.
    static EXTRACT_SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = EXTRACT_SEQ.fetch_add(1, Ordering::Relaxed);
    let extract_dir = std::env::temp_dir().join(format!(
        "chess-tui-stockfish-fetch-{}-{}",
        std::process::id(),
        seq
    ));
    let _ = fs::remove_dir_all(&extract_dir);
    fs::create_dir_all(&extract_dir).map_err(FetchError::Io)?;
    let _cleanup = RemoveDirOnDrop(extract_dir.clone());

    // -xzf and -xf both run through the same bsdtar (libarchive) binary that
    // ships on macOS and on Windows 10 1803+/11; -z is a gzip hint (correct
    // for the .tar.gz macOS archive) and would be wrong for the Windows
    // .zip archives, whose format bsdtar autodetects from plain -xf.
    let extract_flag = match pin.archive_format {
        ArchiveFormat::TarGz => "-xzf",
        ArchiveFormat::Zip => "-xf",
    };
    run(
        Command::new("tar")
            .arg(extract_flag)
            .arg(&archive)
            .arg("-C")
            .arg(&extract_dir),
        "tar",
    )
    .map_err(FetchError::Extract)?;

    let src_exe = extract_dir.join(pin.executable_in_archive);
    if !src_exe.is_file() {
        return Err(FetchError::Extract(format!(
            "{} missing from extracted archive",
            pin.executable_in_archive
        )));
    }
    verify_pin(
        &src_exe,
        pin.executable_size,
        pin.executable_sha256,
        "executable",
    )?;
    verify_universal_binary(&src_exe)?;

    fs::copy(&src_exe, &bundle_exe).map_err(FetchError::Io)?;
    set_executable(&bundle_exe)?;
    verify_pin(
        &bundle_exe,
        pin.executable_size,
        pin.executable_sha256,
        "staged executable",
    )?;

    eprintln!("[chess] Stockfish staged at {}", bundle_exe.display());
    Ok(bundle_exe)
}

struct RemoveDirOnDrop(PathBuf);

impl Drop for RemoveDirOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn matches_pin(path: &Path, expected_size: u64, expected_sha256: &str) -> bool {
    file_size(path).ok() == Some(expected_size)
        && sha256_file(path).ok().as_deref() == Some(expected_sha256)
}

fn verify_pin(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
    label: &str,
) -> Result<(), FetchError> {
    let size = file_size(path)?;
    if size != expected_size {
        return Err(FetchError::Verify(format!(
            "{label} size {size} != expected {expected_size}"
        )));
    }
    let sha256 = sha256_file(path)?;
    if sha256 != expected_sha256 {
        return Err(FetchError::Verify(format!(
            "{label} sha256 {sha256} != expected {expected_sha256}"
        )));
    }
    Ok(())
}

fn file_size(path: &Path) -> Result<u64, FetchError> {
    fs::metadata(path).map(|m| m.len()).map_err(FetchError::Io)
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, FetchError> {
    let mut file = fs::File::open(path).map_err(FetchError::Io)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf).map_err(FetchError::Io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn download(url: &str, dest: &Path) -> Result<(), FetchError> {
    let tmp = dest.with_extension("tmp");
    // -L follows redirects (GitHub -> signed release-asset URL); --fail turns
    // HTTP errors into a non-zero exit instead of writing an error page. curl
    // ships by default on macOS and on Windows 10 1803+/11.
    run(
        Command::new("curl")
            .arg("-L")
            .arg("--fail")
            .arg("--retry")
            .arg("3")
            .arg("--retry-delay")
            .arg("2")
            .arg("-o")
            .arg(&tmp)
            .arg(url),
        "curl",
    )
    .map_err(FetchError::Download)?;
    fs::rename(&tmp, dest).map_err(FetchError::Io)
}

fn run(cmd: &mut Command, name: &str) -> Result<(), String> {
    let status = cmd
        .status()
        .map_err(|err| format!("could not run {name}: {err}"))?;
    if !status.success() {
        return Err(format!("{name} exited with {status}"));
    }
    Ok(())
}

// Verifies the extracted executable is the expected macOS universal binary
// without ever executing it, matching scripts/fetch-stockfish.sh's file/lipo
// checks. This is a redundant sanity check on top of the SHA-256 match above
// (which already guarantees a byte-for-byte match against the pin); it is
// only meaningful for the macOS pin's fat-binary format, so it is a no-op on
// every other platform, including Windows, whose PE executables have no
// equivalent multi-architecture check to make.
#[cfg(target_os = "macos")]
fn verify_universal_binary(path: &Path) -> Result<(), FetchError> {
    let file_out = Command::new("file")
        .arg(path)
        .output()
        .map_err(|err| FetchError::Verify(format!("could not run file: {err}")))?;
    let file_text = String::from_utf8_lossy(&file_out.stdout);
    if !file_text.contains("Mach-O universal binary") {
        return Err(FetchError::Verify(format!(
            "not a universal binary: {file_text}"
        )));
    }
    let archs_out = Command::new("lipo")
        .arg("-archs")
        .arg(path)
        .output()
        .map_err(|err| FetchError::Verify(format!("could not run lipo: {err}")))?;
    let archs = String::from_utf8_lossy(&archs_out.stdout);
    let has_arch = |name: &str| archs.split_whitespace().any(|arch| arch == name);
    if !has_arch("x86_64") || !has_arch("arm64") {
        return Err(FetchError::Verify(format!(
            "missing required architecture, lipo -archs reported: {archs}"
        )));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn verify_universal_binary(_path: &Path) -> Result<(), FetchError> {
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), FetchError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(FetchError::Io)
}

// Windows has no execute-permission bit to set; a staged .exe is already
// runnable.
#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), FetchError> {
    Ok(())
}
