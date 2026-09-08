#!/usr/bin/env bash
set -euo pipefail

# fetch-stockfish.sh — reproducible macOS universal Stockfish 19 fetcher
# Pin source: third_party/stockfish/manifest.json (verified 2026-09-07)
#          and memory://crowded-chess/decisions/stockfish-19-mac-os-artifact-pin
# Official sources only; never executes the binary; verifies via SHA-256 + file/lipo.

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/third_party/stockfish/manifest.json"
CACHE_DIR="${STOCKFISH_CACHE_DIR:-$ROOT/third_party/stockfish/cache}"
BUNDLE_DIR="${STOCKFISH_BUNDLE_DIR:-$ROOT/third_party/stockfish/bundle}"
BUNDLE_EXE="$BUNDLE_DIR/stockfish-macos-universal"

# --- pin (must match manifest.json + memory pin) ---
VERSION="19"
TAG="sf_19"
COMMIT="edb0d9db6731067ec50ce619ff372b463bc4dd5d"
BINARY_DURABLE_URL="https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-macos-universal.tar.gz"
BINARY_SIZE=82323876
BINARY_SHA256="a1f0e3bcc5a6927a11fe6fc8e54a779754645f3c2bae2cf13420fd1957adaa77"
EXE_SHA256="8eed61129d1493c5d1f2fd9323f0c54c47ac49319911fbde18c6b9c87e8b13c5"
EXE_SIZE=105458632
SOURCE_DURABLE_URL="https://github.com/official-stockfish/Stockfish/archive/refs/tags/sf_19.zip"
SOURCE_SIZE=373314
SOURCE_SHA256="024a509d7af387218bd59c559473a71a1d6b411885010543891495119a671119"
COPYING_SHA256="3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986"

FORCE=0
CLEAN=0
for arg in "$@"; do
  case "$arg" in
    --force) FORCE=1 ;;
    --clean) CLEAN=1 ;;
    -h|--help)
      cat <<HELP
Usage: $0 [--force] [--clean]

Fetches Stockfish $VERSION macOS universal (tag $TAG commit $COMMIT) from
$BINARY_DURABLE_URL

Options:
  --force   re-download even if cached bundle is valid
  --clean   remove bundle and cache then exit
  --help    this message

Env overrides:
  STOCKFISH_CACHE_DIR  default $ROOT/third_party/stockfish/cache
  STOCKFISH_BUNDLE_DIR default $ROOT/third_party/stockfish/bundle

Idempotent: second run re-verifies and exits 0 if hashes + archs match.
Never executes the binary; verifies via file/lipo + SHA-256.
HELP
      exit 0
      ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

if [ "$CLEAN" -eq 1 ]; then
  rm -rf "$BUNDLE_DIR" "$CACHE_DIR"
  echo "cleaned $BUNDLE_DIR $CACHE_DIR"
  exit 0
fi

# verify manifest matches hard-coded pin (single source of truth guard)
if [ -f "$MANIFEST" ]; then
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$MANIFEST" "$BINARY_SHA256" "$SOURCE_SHA256" "$EXE_SHA256" "$COPYING_SHA256" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
checks = [
  (m["binary_archive"]["sha256"], sys.argv[2], "binary_archive.sha256"),
  (m["source_archive"]["sha256"], sys.argv[3], "source_archive.sha256"),
  (m["binary_archive"]["executable_sha256"], sys.argv[4], "binary_archive.executable_sha256"),
  (m["license"]["sha256"], sys.argv[5], "license.sha256"),
]
for a,b,name in checks:
  if a != b:
    print(f"manifest mismatch {name}: manifest {a} != script {b}", file=sys.stderr)
    sys.exit(1)
PY
  fi
fi

# verify Copying.txt tracked file matches pin
TRACKED_COPYING="$ROOT/third_party/stockfish/Copying.txt"
if [ -f "$TRACKED_COPYING" ]; then
  got="$(shasum -a 256 "$TRACKED_COPYING" | awk '{print $1}')"
  if [ "$got" != "$COPYING_SHA256" ]; then
    echo "Copying.txt mismatch: expected $COPYING_SHA256 got $got" >&2
    exit 1
  fi
fi

mkdir -p "$CACHE_DIR" "$BUNDLE_DIR"

ARCHIVE="$CACHE_DIR/stockfish-macos-universal.tar.gz"

need_download=0
if [ ! -f "$ARCHIVE" ]; then
  need_download=1
else
  # verify existing archive before trusting
  got_size="$(wc -c < "$ARCHIVE" | tr -d ' ')"
  got_sha="$(shasum -a 256 "$ARCHIVE" | awk '{print $1}')"
  if [ "$got_size" != "$BINARY_SIZE" ] || [ "$got_sha" != "$BINARY_SHA256" ]; then
    echo "cached archive mismatch (size $got_size vs $BINARY_SIZE, sha $got_sha vs $BINARY_SHA256) — re-downloading" >&2
    need_download=1
  else
    echo "cache hit: $ARCHIVE ($got_size bytes, sha256 $got_sha)"
  fi
fi

if [ "$FORCE" -eq 1 ]; then
  need_download=1
fi

if [ "$need_download" -eq 1 ]; then
  echo "downloading $BINARY_DURABLE_URL -> $ARCHIVE" >&2
  # --fail makes curl exit non-zero on HTTP error; -L follows redirects (GitHub -> Azure signed URL)
  curl -L --fail --retry 3 --retry-delay 2 -o "$ARCHIVE.tmp" "$BINARY_DURABLE_URL"
  got_size="$(wc -c < "$ARCHIVE.tmp" | tr -d ' ')"
  got_sha="$(shasum -a 256 "$ARCHIVE.tmp" | awk '{print $1}')"
  if [ "$got_size" != "$BINARY_SIZE" ]; then
    echo "size mismatch after download: expected $BINARY_SIZE got $got_size" >&2
    rm -f "$ARCHIVE.tmp"
    exit 1
  fi
  if [ "$got_sha" != "$BINARY_SHA256" ]; then
    echo "sha256 mismatch after download: expected $BINARY_SHA256 got $got_sha" >&2
    rm -f "$ARCHIVE.tmp"
    exit 1
  fi
  mv "$ARCHIVE.tmp" "$ARCHIVE"
  echo "verified $ARCHIVE ($got_size bytes, sha256 $got_sha)" >&2
fi

# idempotence: if bundle exe exists and verifies, skip extraction
if [ -f "$BUNDLE_EXE" ] && [ "$FORCE" -eq 0 ]; then
  got_exe_sha="$(shasum -a 256 "$BUNDLE_EXE" | awk '{print $1}')"
  got_exe_size="$(wc -c < "$BUNDLE_EXE" | tr -d ' ')"
  if [ "$got_exe_sha" = "$EXE_SHA256" ] && [ "$got_exe_size" = "$EXE_SIZE" ]; then
    if file "$BUNDLE_EXE" | grep -q "Mach-O universal" && lipo -archs "$BUNDLE_EXE" 2>/dev/null | grep -q "x86_64" && lipo -archs "$BUNDLE_EXE" | grep -q "arm64"; then
      echo "bundle hit: $BUNDLE_EXE already verified (sha256 $got_exe_sha, archs $(lipo -archs "$BUNDLE_EXE"))" >&2
      exit 0
    fi
    echo "bundle exe exists but arch check failed — re-extracting" >&2
  else
    echo "bundle exe mismatch (sha $got_exe_sha vs $EXE_SHA256, size $got_exe_size vs $EXE_SIZE) — re-extracting" >&2
  fi
fi

# extract and verify without execution
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

tar -xzf "$ARCHIVE" -C "$TMPDIR"
# archive top is stockfish/stockfish-macos-universal
SRC_EXE="$TMPDIR/stockfish/stockfish-macos-universal"
if [ ! -f "$SRC_EXE" ]; then
  echo "extracted archive missing $SRC_EXE (contents: $(tar -tzf "$ARCHIVE" | head -n 20 | tr '\n' ' '))" >&2
  exit 1
fi

# verify exe sha/size before installing
got_exe_sha="$(shasum -a 256 "$SRC_EXE" | awk '{print $1}')"
got_exe_size="$(wc -c < "$SRC_EXE" | tr -d ' ')"
if [ "$got_exe_sha" != "$EXE_SHA256" ]; then
  echo "exe sha mismatch after extract: expected $EXE_SHA256 got $got_exe_sha" >&2
  exit 1
fi
if [ "$got_exe_size" != "$EXE_SIZE" ]; then
  echo "exe size mismatch after extract: expected $EXE_SIZE got $got_exe_size" >&2
  exit 1
fi

# verify universal archs via file/lipo without execution
FILE_OUT="$(file "$SRC_EXE")"
echo "$FILE_OUT" >&2
if ! echo "$FILE_OUT" | grep -q "Mach-O universal binary"; then
  echo "file check failed: not a universal binary: $FILE_OUT" >&2
  exit 1
fi
if ! echo "$FILE_OUT" | grep -q "x86_64"; then
  echo "file check failed: missing x86_64 in $FILE_OUT" >&2
  exit 1
fi
if ! echo "$FILE_OUT" | grep -q "arm64"; then
  echo "file check failed: missing arm64 in $FILE_OUT" >&2
  exit 1
fi

LIPO_INFO="$(lipo -info "$SRC_EXE" 2>&1)"
echo "$LIPO_INFO" >&2
if ! echo "$LIPO_INFO" | grep -q "x86_64"; then echo "lipo -info missing x86_64: $LIPO_INFO" >&2; exit 1; fi
if ! echo "$LIPO_INFO" | grep -q "arm64"; then echo "lipo -info missing arm64: $LIPO_INFO" >&2; exit 1; fi

LIPO_ARCHS="$(lipo -archs "$SRC_EXE" 2>&1)"
echo "archs: $LIPO_ARCHS" >&2
if ! echo "$LIPO_ARCHS" | grep -qw "x86_64"; then echo "lipo -archs missing x86_64" >&2; exit 1; fi
if ! echo "$LIPO_ARCHS" | grep -qw "arm64"; then echo "lipo -archs missing arm64" >&2; exit 1; fi

# detailed header for evidence (no execution)
lipo -detailed_info "$SRC_EXE" >&2 || true

# install to bundle with executable permission, preserve quarantine (do not xattr -c)
cp -p "$SRC_EXE" "$BUNDLE_EXE"
chmod 0755 "$BUNDLE_EXE"

# final verify installed bundle
FINAL_SHA="$(shasum -a 256 "$BUNDLE_EXE" | awk '{print $1}')"
if [ "$FINAL_SHA" != "$EXE_SHA256" ]; then
  echo "final bundle sha mismatch: $FINAL_SHA vs $EXE_SHA256" >&2
  exit 1
fi
# ensure no execution attempt left quarantine/signing cleared
echo "installed $BUNDLE_EXE (size $(wc -c < "$BUNDLE_EXE" | tr -d ' ') bytes, sha256 $FINAL_SHA, mode $(stat -f %A "$BUNDLE_EXE" 2>/dev/null || stat -c %a "$BUNDLE_EXE"))" >&2
file "$BUNDLE_EXE" >&2
lipo -info "$BUNDLE_EXE" >&2

echo "ok: Stockfish $VERSION ($TAG $COMMIT) staged at $BUNDLE_EXE" >&2
