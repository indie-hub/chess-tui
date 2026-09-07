# Asset provenance — SpicyGame Pixel Chess Pieces

The 12 PNG sprites in `sprites/` are the exact standard-role sprites from the
free asset pack **Pixel Chess Pieces** by SpicyGame.

## Source

- Pack page: https://spicygame.itch.io/chess-pieces
- License: Creative Commons Zero v1.0 Universal (CC0)
  https://itch.io/game-assets/assets-cc0
- Pack release: PNG, Version 3 (2025-12-28), upload id `11418675`
- Pack archive sha256:
  `b71002574f894f38e60323f84d1e4bc7ca6039919110d6b2fd0b390e6da60401`

## Download method

Obtained non-interactively from the pack's `download_url` endpoint and the
itch.io `file/<upload_id>` API (the itch download gate). No pixels were
redrawn, traced, downsampled, or replaced. The vendored files are byte-for-byte
copies of the `color/` directory entries from the downloaded archive, renamed
with an explicit `white_`/`black_` side prefix.

## Vendored sprites (from `color/`)

Each sprite is a 16x16 palette PNG. Per-file sha256 as vendored:

| File | sha256 |
| --- | --- |
| white_king.png | 05742e4d4b7d630951253c31da308d66cc52f5cade6b9095e200f35ffc20a742 |
| white_queen.png | 193883cddb9dc38c91bdf793b9ff2e70c6b6f581214a8cfc3cdd3b5ab1b16b88 |
| white_rook.png | 5373726bc21eb3d0a0337ec80bcb72dd2e288337834c3b08ae6a168d887925ff |
| white_bishop.png | 5329105995e44bd53ee8645b86dfda07b6e68df46af676a90ad1aba210e8ff15 |
| white_knight.png | 9abe602f4bc32edd8f5b0c819f82e88972279eb0792da759f76c682f9be078cb |
| white_pawn.png | 5985267bfb128d25e187eba9e80dc60a8e2ca308a7a65273e2dc3e523099b781 |
| black_king.png | d468b0624b105cfbc505d009ade1ef851d19cb0d84b7ebd56237b48701e4bad4 |
| black_queen.png | ffd70c3aa15a291dacc244382992ed4566354a764dccf66834317744bd7633fc |
| black_rook.png | ba36aa764140edbeab840e9fc4bf3ebba866d8b3c80cec3ce2e8791cde5a0840 |
| black_bishop.png | 73e1a56ba9ce3364043970d84605aad6e4d9e1e337523a502511becca653961c |
| black_knight.png | d5a703c2d857081d61237d3bbe63f31281eab2e4532ea12b0797191820e69ba1 |
| black_pawn.png | e6fc1090ad72b27bd721faf8857f54f360b0b52aad3a3ddf603b9382bb1330d4 |

## CC0 license text

> The person who associated a work with this deed has dedicated the work to the
> public domain by waiving all of their rights to the work worldwide under
> copyright law, including all related and neighboring rights, to the extent
> allowed by law. You can copy, modify, distribute and perform the work, even
> for commercial purposes, all without asking permission.
>
> https://creativecommons.org/publicdomain/zero/1.0/