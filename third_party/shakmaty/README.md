# shakmaty — chess rules and move generation (GPL-3.0-or-later)

This directory tracks only the license text for `shakmaty`, the Rust crate
this project depends on for legal move generation, check/checkmate/stalemate
detection, and FEN/SAN handling (see `src/game.rs`). The crate itself is not
vendored here; it is fetched and built normally by Cargo from crates.io.

## Pin

- Crate: `shakmaty` version `0.30.1` (pinned in `Cargo.lock`)
- Source: https://github.com/niklasf/shakmaty
- License: `GPL-3.0-or-later`, declared in the crate's own `Cargo.toml`
  (`workspace.package.license`)
- License text: `COPYING`, vendored verbatim from
  https://raw.githubusercontent.com/niklasf/shakmaty/main/COPYING
  — sha256 `8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903`

## Why this project is GPL-3.0-or-later

Linking against a GPL-3.0-or-later library makes the resulting binary a
combined work, so this project is licensed GPL-3.0-or-later as a whole; see
the repository root `LICENSE`. Stockfish (`third_party/stockfish/`) is the
other GPL-3.0 dependency involved here, though it is run as a separate
subprocess rather than linked in.
