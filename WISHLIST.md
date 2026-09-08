# Wishlist

Wishlist items are planning notes, not implementation authorization.

## Milestone 3: Game sessions

### New-game configuration

- Choose the human side: White, Black, or random.
- Choose an approximate opponent Elo.
- Choose unlimited play or a timed preset.
- Support an optional increment for timed games, such as `5+3`.

### Clocks

- Show separate White and Black clocks during timed games.
- Run only the active side's clock using monotonic time.
- Send remaining time and increment to Stockfish through UCI.
- End the game immediately when a player runs out of time.

### Material

- Show the pieces captured by each player.
- Show the material balance, such as `White +3`.
- Keep material balance distinct from Stockfish's positional evaluation.

### Results

- Show a prominent `YOU WIN`, `YOU LOSE`, or `DRAW` banner.
- Include the result reason: checkmate, timeout, resignation, repetition,
  fifty-move claim, stalemate, or insufficient material.
- Offer rematch, configure a new game, and quit actions.

### Related additions

- Allow the human player to resign.
- Allow a rematch with the same settings and an option to swap colors.
- Describe the Elo setting as an approximate playing-strength target.

### Deferred

- PGN saving and loading.
- Takebacks.
- Opening names.
- Engine evaluation bar.
- Custom clock configuration beyond presets and increment.
- Persistent settings.
