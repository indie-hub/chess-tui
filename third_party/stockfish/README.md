# Stockfish 19 — macOS and Windows bundles (reproducible, GPL-3.0)

This directory holds **only** reproducible pin metadata and GPL text. The
executables and their archives are **never committed**; they live in
gitignored `bundle/` and `cache/` and are fetched either by
`src/fetch.rs` automatically on first run (macOS, Windows x86-64, Windows
arm64) or by `scripts/fetch-stockfish.sh` manually (macOS only today).

## macOS pin (independently verified 2026-09-07)

Source: `memory://crowded-chess/decisions/stockfish-19-mac-os-artifact-pin`
(`basic-memory/decisions/Stockfish 19 macOS artifact pin.md`)

- **Version:** 19 — tag `sf_19` — commit `edb0d9db6731067ec50ce619ff372b463bc4dd5d` (bench 2497913, published 2026-09-05T08:33:17Z)
- **Binary (macOS universal):**
  `https://github.com/official-stockfish/Stockfish/releases/download/sf_19/stockfish-macos-universal.tar.gz`
  — 82,323,876 bytes — SHA-256 `a1f0e3bcc5a6927a11fe6fc8e54a779754645f3c2bae2cf13420fd1957adaa77`
  — executable `stockfish/stockfish-macos-universal` inside tar, 105,458,632 bytes,
  SHA-256 `8eed61129d1493c5d1f2fd9323f0c54c47ac49319911fbde18c6b9c87e8b13c5`,
  universal `x86_64` + `arm64` (file/lipo verified, never executed)
- **Source zip:**
  `https://github.com/official-stockfish/Stockfish/archive/refs/tags/sf_19.zip`
  (codeload `https://codeload.github.com/official-stockfish/Stockfish/zip/refs/tags/sf_19`)
  — 373,314 bytes — SHA-256 `024a509d7af387218bd59c559473a71a1d6b411885010543891495119a671119`
  — alternate tar.gz `.../sf_19.tar.gz` 291,206 bytes `519b653d...`
  — top prefix `Stockfish-sf_19/` — contains `Copying.txt`, `README.md`, full `src/`
- **License:** GPL-3.0 — `Copying.txt` 35,149 bytes SHA-256 `3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986`,
  present in both archives; `CITATION.cff` also `GPL-3.0`
- **Authenticity:** No GPG/signature or published checksum file for `sf_19`
  (checked release page + API assets). Verify via HTTPS origin + local SHA-256 only.
- **Download page (official):** `https://stockfishchess.org/download/` links to the
  `releases/latest/download/...` URL which redirects to `releases/download/sf_19/...`
  and then to a temporary Azure signed URL; the durable URL is the
  `releases/download/sf_19/...` form recorded above.

Machine-readable pin: `third_party/stockfish/manifest.json`
Durable pointer: `third_party/stockfish/SOURCE_POINTER.txt`
GPL text: `third_party/stockfish/Copying.txt` (tracked verbatim)

## Windows pins (independently verified 2026-09-08)

Same tag/commit as the macOS pin above (`sf_19`,
`edb0d9db6731067ec50ce619ff372b463bc4dd5d`). Both assets are release-wide zip
archives containing the full source tree plus one prebuilt executable each;
GitHub's own reported release-asset digest was cross-checked against a
locally computed SHA-256 for both and matched.

- **x86-64:** `stockfish-windows-x86-64-universal.zip` — 81,431,614 bytes —
  SHA-256 `3c8bf1f9ea66a09350a40df4f632288285ac206d99f33ab5842c408fc30b48a7`
  — executable `stockfish/stockfish-windows-x86-64-universal.exe` inside the
  zip, 103,046,300 bytes, SHA-256
  `45bc8e4969147db9c2eb533810637994619bff0eacc81ccfd9854394901bcbd0`,
  `PE32+ executable (console) x86-64 ..., for MS Windows` (`file`-verified,
  never executed)
- **arm64:** `stockfish-windows-arm64-universal.zip` — 80,190,536 bytes —
  SHA-256 `8372ad3f0d7276deb2c70f801f541ec7db463219fc6d9c7592864e542aa4f401`
  — executable `stockfish/stockfish-windows-arm64-universal.exe` inside the
  zip, 100,303,360 bytes, SHA-256
  `3b5881df3d6f92817cf6664a6a18a473b50d424c71a1247fe4268090db281413`,
  `PE32+ executable (console) Aarch64, for MS Windows` (`file`-verified,
  never executed)
- Both archives' `Copying.txt` are byte-identical (same SHA-256) to the
  macOS pin's, confirming the same commit/license text across platforms.
- Windows binaries auto-fetch through `src/fetch.rs` on first run;
  `scripts/fetch-stockfish.sh` does not cover Windows yet (it is a bash
  script, and the automatic Rust path already gives Windows the same
  first-run convenience macOS gets, so a `.ps1` equivalent has not been
  written).

Machine-readable pin: `third_party/stockfish/manifest.json`
(`windows_x86_64_binary`, `windows_arm64_binary`)
Decision record: `basic-memory/decisions/Stockfish 19 Windows artifact pin.md`

## Usage

Fetch + verify + stage executable (idempotent, no execution):

```sh
bash scripts/fetch-stockfish.sh
# or
./scripts/fetch-stockfish.sh
```

Result: `third_party/stockfish/bundle/stockfish-macos-universal` (mode 0755,
quarantine preserved, never added to PATH).

Force re-download:

```sh
bash scripts/fetch-stockfish.sh --force
```

Clean bundle/cache:

```sh
bash scripts/fetch-stockfish.sh --clean
```

Environment overrides (optional):

- `STOCKFISH_CACHE_DIR` — cache dir for tar.gz (default `third_party/stockfish/cache`)
- `STOCKFISH_BUNDLE_DIR` — staging dir for executable (default `third_party/stockfish/bundle`)

## Verify without executing

After `fetch`, without running:

```sh
file third_party/stockfish/bundle/stockfish-macos-universal
lipo -info third_party/stockfish/bundle/stockfish-macos-universal
lipo -archs third_party/stockfish/bundle/stockfish-macos-universal
shasum -a 256 third_party/stockfish/bundle/stockfish-macos-universal
```

Must show `Mach-O universal binary with 2 architectures: [x86_64] [arm64]`
and `8eed61129d1493c5d1f2fd9323f0c54c47ac49319911fbde18c6b9c87e8b13c5`.

## Packaging

Future release packaging **must not** copy the binary into git. Use the fetch
script in CI / `cargo bundle` / `cargo dist` steps:

```sh
bash scripts/fetch-stockfish.sh   # populates bundle/
# then include in package:
#   third_party/stockfish/bundle/stockfish-macos-universal  ->  Resources/engine/
#   third_party/stockfish/Copying.txt                        ->  Resources/engine/
#   third_party/stockfish/SOURCE_POINTER.txt                 ->  Resources/engine/
#   third_party/stockfish/manifest.json                      ->  Resources/engine/
```

Or set `STOCKFISH_PATH` at runtime to override bundled location. Never add to
`PATH`; never clear quarantine/signing.

## Idempotence & failure modes

- Second run without `--force` re-verifies existing bundle/cache and exits 0
  if hashes + architectures match.
- Corrupted download or SHA mismatch aborts before extraction (`--force` re-downloads).
- `file`/`lipo` check fails if universal slice missing.
- No network: script fails with `curl --fail` error; no partial bundle left.

## No source overlap

This directory and `scripts/` never touch `src/`, `Cargo.toml`, `Cargo.lock`,
`README.md`, `assets/`, tests, `.code4me/`, or memory — verified via
`find src -type f | xargs shasum` before/after.
