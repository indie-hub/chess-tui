# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). There are no tagged
releases yet, so everything so far is grouped under Unreleased.

## [Unreleased]

### Added

- Keyboard-driven chess TUI on Ratatui and Crossterm: cursor movement,
  selection, and move entry with arrows or `hjkl`.
- Legal move generation, castling, en passant, and all four promotion
  choices via Shakmaty, with automatic check, checkmate, stalemate, and
  insufficient-material detection.
- Automatic draw at fivefold repetition or the 75-move rule; manual draw
  claims (`d`) for threefold repetition and the fifty-move rule, usable
  before an intended move or while the promotion chooser is open.
- Stockfish integration: a pinned, SHA-256-verified binary fetch script
  (`scripts/fetch-stockfish.sh`), subprocess UCI play against the engine,
  and legality validation of engine moves before they are played.
- Automatic first-run fetch, verification, and staging of the Stockfish
  binary on macOS, Windows x86-64, and Windows arm64 (`src/fetch.rs`), so
  `cargo run` alone can play against the engine with no manual step;
  `scripts/fetch-stockfish.sh` remains available for manual pre-fetch on
  macOS.
- GPL-3.0-or-later licensing: `LICENSE`, `SPDX-License-Identifier` headers
  on all first-party source, vendored license text for the `shakmaty` and
  Stockfish GPL dependencies, and CC0 attribution for the SpicyGame sprite
  pack.
- New-game configuration screen (`n`): choose White, Black, or a random
  side, and set Stockfish Skill Level (0-20); the board orients to the
  human player's side.
- Side-switch shortcut (`s`) to toggle colour and restart immediately.
- Captured-piece and material-balance display in the side panel, as
  fixed-order Q/R/B/N/P grouped counts per colour.
- Pixel-exact half-block rendering of the vendored SpicyGame sprite pack,
  decoded ahead of time into static RGBA data with no runtime image
  dependency.
- Resize prompt for terminals smaller than 170x68.

### Changed

- New-game configuration accepts lowercase `n` as well as uppercase.
- Captured-material formatting iterated from free-form counts to fixed
  Q/R/B/N/P groups, then to the current White/Black grouped layout, based
  on visual review.
